use super::*;
use ae::aegp::{Stream, StreamValue};

// The pinned binding models these bit flags as an enum and panics on zero or
// combined flags. Read the raw bitfield when checking visibility.
pub(super) fn dynamic_flags(in_data: InData, stream: &Stream) -> Result<u32, Error> {
    let pica = in_data.pica_basic_suite_ptr();
    let mut suite: *const ae::sys::AEGP_DynamicStreamSuite4 = std::ptr::null();
    // SAFETY: acquire/use/release occur in one AE callback; no pointers escape.
    unsafe {
        let acquire = (*pica).AcquireSuite.ok_or(Error::MissingSuite)?;
        let release = (*pica).ReleaseSuite.ok_or(Error::MissingSuite)?;
        let err = acquire(
            ae::sys::kAEGPDynamicStreamSuite.as_ptr().cast(),
            ae::sys::kAEGPDynamicStreamSuiteVersion4 as i32,
            (&mut suite as *mut *const ae::sys::AEGP_DynamicStreamSuite4).cast(),
        );
        if err != 0 {
            return Err(Error::MissingSuite);
        }
        let result = (|| {
            let get = (*suite)
                .AEGP_GetDynamicStreamFlags
                .ok_or(Error::MissingSuite)?;
            let mut flags = 0;
            let err = get(stream.as_ptr(), &mut flags);
            if err == 0 {
                Ok(flags)
            } else {
                Err(Error::from(err))
            }
        })();
        release(
            ae::sys::kAEGPDynamicStreamSuite.as_ptr().cast(),
            ae::sys::kAEGPDynamicStreamSuiteVersion4 as i32,
        );
        result
    }
}

/// Own the stream value until every outline/feather read has finished. The
/// pinned Rust binding's new_value() disposes complex values before returning
/// their handles, so it must only be used for primitive stream values.
pub(super) struct OwnedOutline {
    outline: ae::aegp::MaskOutline,
    value: ae::sys::AEGP_StreamValue2,
    suite: *const ae::sys::AEGP_StreamSuite6,
    pica: *mut ae::sys::SPBasicSuite,
}

impl std::ops::Deref for OwnedOutline {
    type Target = ae::aegp::MaskOutline;
    fn deref(&self) -> &Self::Target {
        &self.outline
    }
}

impl Drop for OwnedOutline {
    fn drop(&mut self) {
        // SAFETY: Both suites remain acquired and the value has one owner.
        unsafe {
            if let Some(dispose) = (*self.suite).AEGP_DisposeStreamValue {
                dispose(&mut self.value);
            }
            if let Some(release) = (*self.pica).ReleaseSuite {
                release(
                    ae::sys::kAEGPStreamSuite.as_ptr().cast(),
                    ae::sys::kAEGPStreamSuiteVersion6 as i32,
                );
            }
        }
    }
}

pub(super) fn read_outline(
    in_data: InData,
    stream: &Stream,
    plugin_id: ae::aegp::PluginId,
    mode: ae::aegp::TimeMode,
    time: ae::Time,
) -> Result<OwnedOutline, Error> {
    if stream.stream_type()? != ae::aegp::StreamType::Mask {
        return Err(Error::InvalidParms);
    }
    let pica = in_data.pica_basic_suite_ptr();
    let mut suite: *const ae::sys::AEGP_StreamSuite6 = std::ptr::null();
    // SAFETY: AE supplies the PICA pointer for the current callback. The suite
    // is released on all error paths or transferred to OwnedOutline on success.
    unsafe {
        let acquire = (*pica).AcquireSuite.ok_or(Error::MissingSuite)?;
        let release = (*pica).ReleaseSuite.ok_or(Error::MissingSuite)?;
        let err = acquire(
            ae::sys::kAEGPStreamSuite.as_ptr().cast(),
            ae::sys::kAEGPStreamSuiteVersion6 as i32,
            (&mut suite as *mut *const ae::sys::AEGP_StreamSuite6).cast(),
        );
        if err != 0 {
            return Err(Error::MissingSuite);
        }
        let result = (|| {
            let get = (*suite).AEGP_GetNewStreamValue.ok_or(Error::MissingSuite)?;
            let _dispose = (*suite)
                .AEGP_DisposeStreamValue
                .ok_or(Error::MissingSuite)?;
            let mut value = std::mem::zeroed();
            let err = get(
                plugin_id,
                stream.as_ptr(),
                mode.into(),
                &time.into(),
                0,
                &mut value,
            );
            if err != 0 {
                return Err(Error::from(err));
            }
            Ok(OwnedOutline {
                outline: ae::aegp::MaskOutlineHandle::from_raw(value.val.mask).into(),
                value,
                suite,
                pica,
            })
        })();
        if result.is_err() {
            release(
                ae::sys::kAEGPStreamSuite.as_ptr().cast(),
                ae::sys::kAEGPStreamSuiteVersion6 as i32,
            );
        }
        result
    }
}

#[derive(Clone, Copy, Debug)]
struct Transform([f64; 6]);

impl Transform {
    fn point(self, p: [f64; 2]) -> [f64; 2] {
        let [a, b, c, d, x, y] = self.0;
        [a * p[0] + c * p[1] + x, b * p[0] + d * p[1] + y]
    }
    fn then(self, child: Self) -> Self {
        let [a, b, c, d, x, y] = self.0;
        let [e, f, g, h, u, v] = child.0;
        Self([
            a * e + c * f,
            b * e + d * f,
            a * g + c * h,
            b * g + d * h,
            a * u + c * v + x,
            b * u + d * v + y,
        ])
    }
    fn translate(p: [f64; 2]) -> Self {
        Self([1.0, 0.0, 0.0, 1.0, p[0], p[1]])
    }
    fn rotate(angle: f64) -> Self {
        let (s, c) = angle.to_radians().sin_cos();
        Self([c, s, -s, c, 0.0, 0.0])
    }
}

struct ShapeReader {
    in_data: InData,
    plugin_id: ae::aegp::PluginId,
    time: ae::Time,
    settings: Settings,
    points: Vec<PathPoint>,
    path_index: usize,
    along: f32,
}

pub(super) fn is_shape_layer(in_data: InData) -> Result<bool, Error> {
    if in_data.is_premiere() {
        return Ok(false);
    }
    let interface = ae::aegp::suites::PFInterface::new()?;
    let layer: ae::aegp::Layer = interface.effect_layer(in_data.effect())?.into();
    Ok(layer.object_type()? == ae::aegp::ObjectType::Vector)
}

pub(super) fn path_points(
    in_data: InData,
    plugin_id: Option<ae::aegp::PluginId>,
    settings: Settings,
) -> Result<Option<Vec<PathPoint>>, Error> {
    let Some(plugin_id) = plugin_id else {
        return Ok(None);
    };
    let interface = ae::aegp::suites::PFInterface::new()?;
    let layer: ae::aegp::Layer = interface.effect_layer(in_data.effect())?.into();
    if layer.object_type()? != ae::aegp::ObjectType::Vector {
        return Ok(None);
    }
    let time = interface.convert_effect_to_comp_time(
        in_data.effect(),
        in_data.current_time(),
        in_data.time_scale(),
    )?;
    let root = layer.new_stream_for_layer(plugin_id)?;
    let contents = root.new_stream_by_match_name(plugin_id, "ADBE Root Vectors Group")?;
    let mut reader = ShapeReader {
        in_data,
        plugin_id,
        time,
        settings,
        points: Vec::new(),
        path_index: 0,
        along: 0.0,
    };
    // Shape layers rasterize after their layer transform. Their effect buffer
    // is in composition coordinates, while Contents paths are layer-local.
    let matrix: ae::sys::A_Matrix4 = layer.to_world_xform(time)?.into();
    let m = matrix.mat;
    let transform = Transform([m[0][0], m[0][1], m[1][0], m[1][1], m[3][0], m[3][1]]);
    reader.walk(&contents, transform, 0)?;
    Ok(Some(reader.points))
}

impl ShapeReader {
    fn value(&self, group: &Stream, name: &str) -> Result<StreamValue, Error> {
        group
            .new_stream_by_match_name(self.plugin_id, name)?
            .new_value(
                self.plugin_id,
                ae::aegp::TimeMode::CompTime,
                self.time,
                false,
            )
    }
    fn scalar(&self, group: &Stream, name: &str) -> Result<f64, Error> {
        self.value(group, name)?.try_into()
    }
    fn vector(&self, group: &Stream, name: &str) -> Result<[f64; 2], Error> {
        self.value(group, name)?.try_into()
    }
    fn group_transform(&self, group: &Stream) -> Result<Transform, Error> {
        let t = group.new_stream_by_match_name(self.plugin_id, "ADBE Vector Transform Group")?;
        let anchor = self.vector(&t, "ADBE Vector Anchor")?;
        let position = self.vector(&t, "ADBE Vector Position")?;
        let scale = self.vector(&t, "ADBE Vector Scale")?;
        let angle = self.scalar(&t, "ADBE Vector Rotation")?;
        let skew = self.scalar(&t, "ADBE Vector Skew")?;
        let axis = self.scalar(&t, "ADBE Vector Skew Axis")?;
        Ok(Transform::translate(position)
            .then(Transform::rotate(angle))
            .then(Transform::rotate(-axis))
            .then(Transform([
                1.0,
                0.0,
                -skew.to_radians().tan(),
                1.0,
                0.0,
                0.0,
            ]))
            .then(Transform::rotate(axis))
            .then(Transform([
                scale[0] / 100.0,
                0.0,
                0.0,
                scale[1] / 100.0,
                0.0,
                0.0,
            ]))
            .then(Transform::translate([-anchor[0], -anchor[1]])))
    }
    fn walk(&mut self, contents: &Stream, transform: Transform, depth: usize) -> Result<(), Error> {
        if depth > 64 {
            return Err(Error::InvalidParms);
        }
        for i in 0..contents.num_streams_in_group()? {
            let child: Stream = contents.new_stream_by_index(self.plugin_id, i)?.into();

            let name = child.match_name()?;
            let flags = dynamic_flags(self.in_data, &child)?;
            if flags & ae::sys::AEGP_DynStreamFlag_ACTIVE_EYEBALL as u32 == 0 {
                continue;
            }
            match name.as_str() {
                "ADBE Vector Group" => {
                    let transform = transform.then(self.group_transform(&child)?);
                    let contents =
                        child.new_stream_by_match_name(self.plugin_id, "ADBE Vectors Group")?;
                    self.walk(&contents, transform, depth + 1)?;
                }
                "ADBE Vector Shape - Group" => {
                    let stream =
                        child.new_stream_by_match_name(self.plugin_id, "ADBE Vector Shape")?;
                    let outline = read_outline(
                        self.in_data,
                        &stream,
                        self.plugin_id,
                        ae::aegp::TimeMode::CompTime,
                        self.time,
                    )?;
                    let count = outline.num_segments()?;
                    for i in 0..count {
                        let a = outline.vertex_info(i)?;
                        let b = outline.vertex_info(i + 1)?;
                        self.cubic(outline_cubic(a, b), transform);
                    }
                    self.path_index += 1;
                }
                "ADBE Vector Shape - Rect" => {
                    let size = self.vector(&child, "ADBE Vector Rect Size")?;
                    let position = self.vector(&child, "ADBE Vector Rect Position")?;
                    let roundness = self.scalar(&child, "ADBE Vector Rect Roundness")?;
                    self.rounded_rect(
                        size,
                        roundness,
                        transform.then(Transform::translate(position)),
                    );
                    self.path_index += 1;
                }
                "ADBE Vector Shape - Ellipse" => {
                    let size = self.vector(&child, "ADBE Vector Ellipse Size")?;
                    let position = self.vector(&child, "ADBE Vector Ellipse Position")?;
                    self.ellipse(size, transform.then(Transform::translate(position)));
                    self.path_index += 1;
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn cubic(&mut self, control: [[f64; 2]; 4], transform: Transform) {
        let c = control.map(|p| transform.point(p));
        let length: f64 = c
            .windows(2)
            .map(|p| (p[1][0] - p[0][0]).hypot(p[1][1] - p[0][1]))
            .sum();
        if !length.is_finite() || length <= 1.0e-8 {
            return;
        }
        let steps = (length / (self.settings.stroke_width as f64 * 0.125).clamp(0.5, 2.0))
            .ceil()
            .clamp(1.0, 100_000.0) as usize;
        let mut prev = c[0];
        for i in 0..=steps {
            let (p, mut tangent) = eval_cubic(c, i as f64 / steps as f64);
            if tangent[0].hypot(tangent[1]) < 1.0e-8 {
                tangent = [c[3][0] - c[0][0], c[3][1] - c[0][1]];
            }
            let (tx, ty) = normalize2(tangent[0] as f32, tangent[1] as f32);
            self.along += (p[0] - prev[0]).hypot(p[1] - prev[1]) as f32;
            self.points.push(PathPoint {
                path_index: self.path_index,
                x: p[0] as f32,
                y: p[1] as f32,
                tangent_x: tx,
                tangent_y: ty,

                along: self.along,
                stroke_width: self.settings.stroke_width,
            });
            prev = p;
        }
    }
    fn line(&mut self, a: [f64; 2], b: [f64; 2], t: Transform) {
        self.cubic([a, a, b, b], t);
    }
    fn ellipse(&mut self, size: [f64; 2], t: Transform) {
        let x = size[0].abs() * 0.5;
        let y = size[1].abs() * 0.5;
        const K: f64 = 0.5522847498307936;
        for c in [
            [[0.0, -y], [K * x, -y], [x, -K * y], [x, 0.0]],
            [[x, 0.0], [x, K * y], [K * x, y], [0.0, y]],
            [[0.0, y], [-K * x, y], [-x, K * y], [-x, 0.0]],
            [[-x, 0.0], [-x, -K * y], [-K * x, -y], [0.0, -y]],
        ] {
            self.cubic(c, t);
        }
    }
    fn rounded_rect(&mut self, size: [f64; 2], roundness: f64, t: Transform) {
        let x = size[0].abs() * 0.5;
        let y = size[1].abs() * 0.5;
        let r = roundness.max(0.0).min(x.min(y));
        let k = r * 0.5522847498307936;
        self.line([-x + r, -y], [x - r, -y], t);
        self.cubic(
            [[x - r, -y], [x - r + k, -y], [x, -y + r - k], [x, -y + r]],
            t,
        );
        self.line([x, -y + r], [x, y - r], t);
        self.cubic([[x, y - r], [x, y - r + k], [x - r + k, y], [x - r, y]], t);
        self.line([x - r, y], [-x + r, y], t);
        self.cubic(
            [[-x + r, y], [-x + r - k, y], [-x, y - r + k], [-x, y - r]],
            t,
        );
        self.line([-x, y - r], [-x, -y + r], t);
        self.cubic(
            [
                [-x, -y + r],
                [-x, -y + r - k],
                [-x + r - k, -y],
                [-x + r, -y],
            ],
            t,
        );
    }
}

fn outline_cubic(a: ae::sys::AEGP_MaskVertex, b: ae::sys::AEGP_MaskVertex) -> [[f64; 2]; 4] {
    // AEGP/PF tangents are offsets from their own vertex, just like scripting
    // Shape tangents. Zero handles must leave a polygon's edges straight.
    [
        [a.x, a.y],
        [a.x + a.tan_out_x, a.y + a.tan_out_y],
        [b.x + b.tan_in_x, b.y + b.tan_in_y],
        [b.x, b.y],
    ]
}

fn eval_cubic(c: [[f64; 2]; 4], t: f64) -> ([f64; 2], [f64; 2]) {
    let u = 1.0 - t;
    let point = std::array::from_fn(|i| {
        u * u * u * c[0][i]
            + 3.0 * u * u * t * c[1][i]
            + 3.0 * u * t * t * c[2][i]
            + t * t * t * c[3][i]
    });
    let tangent = std::array::from_fn(|i| {
        3.0 * u * u * (c[1][i] - c[0][i])
            + 6.0 * u * t * (c[2][i] - c[1][i])
            + 3.0 * t * t * (c[3][i] - c[2][i])
    });
    (point, tangent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex(x: f64, y: f64, incoming: [f64; 2], outgoing: [f64; 2]) -> ae::sys::AEGP_MaskVertex {
        ae::sys::AEGP_MaskVertex {
            x,
            y,
            tan_in_x: incoming[0],
            tan_in_y: incoming[1],
            tan_out_x: outgoing[0],
            tan_out_y: outgoing[1],
        }
    }

    #[test]
    fn zero_outline_tangents_preserve_straight_edges_away_from_origin() {
        let a = vertex(50.0, 100.0, [0.0; 2], [0.0; 2]);
        let b = vertex(110.0, 100.0, [0.0; 2], [0.0; 2]);
        let curve = outline_cubic(a, b);
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let (point, tangent) = eval_cubic(curve, t);
            assert_eq!(point[1], 100.0);
            assert_eq!(tangent[1], 0.0);
        }
        assert_eq!(eval_cubic(curve, 0.5).0, [80.0, 100.0]);
    }

    #[test]
    fn outline_handles_are_relative_to_each_own_vertex() {
        let a = vertex(80.0, 40.0, [0.0; 2], [60.0, 0.0]);
        let b = vertex(200.0, 160.0, [0.0, -90.0], [0.0; 2]);
        let curve = outline_cubic(a, b);
        assert_eq!(eval_cubic(curve, 0.5).0, [162.5, 66.25]);
        let shift = [137.0, -59.0];
        let shifted = outline_cubic(
            ae::sys::AEGP_MaskVertex {
                x: a.x + shift[0],
                y: a.y + shift[1],
                ..a
            },
            ae::sys::AEGP_MaskVertex {
                x: b.x + shift[0],
                y: b.y + shift[1],
                ..b
            },
        );
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let (p, tangent) = eval_cubic(curve, t);
            let (q, shifted_tangent) = eval_cubic(shifted, t);
            assert_eq!(q, [p[0] + shift[0], p[1] + shift[1]]);
            assert_eq!(shifted_tangent, tangent);
        }
    }

    #[test]
    fn bezier_curve_passes_through_endpoints_and_midpoint() {
        let curve = [[-90.0, 0.0], [-60.0, -80.0], [60.0, 80.0], [90.0, 0.0]];
        assert_eq!(eval_cubic(curve, 0.0).0, [-90.0, 0.0]);
        assert_eq!(eval_cubic(curve, 0.5).0, [0.0, 0.0]);
        assert_eq!(eval_cubic(curve, 1.0).0, [90.0, 0.0]);
    }

    #[test]
    fn nested_transforms_apply_child_before_parent() {
        let parent = Transform::translate([100.0, 50.0]).then(Transform::rotate(90.0));
        let child = Transform::translate([20.0, -10.0]);
        let p = parent.then(child).point([5.0, 0.0]);
        assert!((p[0] - 110.0).abs() < 1.0e-8);
        assert!((p[1] - 75.0).abs() < 1.0e-8);
    }
}
