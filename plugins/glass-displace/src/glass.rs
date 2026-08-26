use std::f32::consts::TAU;

#[cfg(test)]
thread_local! {
    static PREPARED_PROCEDURAL_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HeightSource {
    Procedural,
    CustomMap,
    InputLuma,
    InputAlpha,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Shape {
    Sphere,
    RoundedRectangle,
    Diamond,
    Ring,
    Facets,
    Shards,
    ImpactGlass,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MapChannel {
    Luma,
    Alpha,
    Red,
    Green,
    Blue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Sampling {
    Nearest,
    Bilinear,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EdgeMode {
    Transparent,
    Clamp,
    Tile,
    Mirror,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DebugView {
    Final,
    Height,
    Normal,
    Displacement,
    FacetId,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProceduralSettings {
    pub shape: Shape,
    pub center: [f32; 2],
    pub size: f32,
    pub aspect: f32,
    pub rotation: f32,
    pub roundness: f32,
    pub ring_width: f32,
    pub facet_size: f32,
    pub facet_amount: f32,
    pub facet_jitter: f32,
    pub crack_width: f32,
    pub crack_depth: f32,
    pub radial_cracks: u32,
    pub crack_branching: f32,
    pub crack_jitter: f32,
    pub stress_rings: u32,
    pub ring_jitter: f32,
    pub impact_falloff: f32,
    pub seed: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct HeightSample {
    pub value: f32,
    pub facet_id: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedProcedural {
    settings: ProceduralSettings,
    half_width: f32,
    half_height: f32,
    rotation_cos: f32,
    rotation_sin: f32,
    rounded_exponent: f32,
    ring_half_band: f32,
    facet_cell_size: f32,
    facet_jitter: f32,
    facet_relief: f32,
    crack_depth: f32,
    impact_radius: f32,
    impact_aspect: f32,
    impact_falloff: f32,
    impact_crack_width: f32,
    impact_sector: f32,
    impact_jitter: f32,
    impact_branching: f32,
    impact_ring_count: u32,
    impact_ring_spacing: f32,
    impact_ring_jitter: f32,
}

impl PreparedProcedural {
    pub(crate) fn new(settings: ProceduralSettings) -> Self {
        #[cfg(test)]
        PREPARED_PROCEDURAL_BUILDS.with(|builds| builds.set(builds.get() + 1));
        let half_width = (settings.size * 0.5).max(1.0e-4);
        let half_height = (half_width / settings.aspect.max(1.0e-4)).max(1.0e-4);
        let rotation_cos = settings.rotation.cos();
        let rotation_sin = settings.rotation.sin();
        let rounded_exponent = 16.0 - settings.roundness.clamp(0.0, 1.0) * 14.0;
        let ring_half_band = settings.ring_width.clamp(0.01, 1.0) * 0.5;
        let facet_cell_size = settings.facet_size.max(1.0);
        let facet_jitter = settings.facet_jitter.clamp(0.0, 1.0);
        let facet_relief = settings.facet_amount.clamp(0.0, 1.0);
        let crack_depth = settings.crack_depth.clamp(0.0, 1.0);
        let impact_radius = (settings.size * 0.5).max(1.0e-4);
        let impact_aspect = settings.aspect.max(1.0e-4);
        let impact_falloff = settings.impact_falloff.clamp(0.0, 1.0);
        let impact_crack_width = settings.crack_width.max(0.0);
        let impact_ray_count = settings.radial_cracks.clamp(3, 96) as f32;
        let impact_sector = TAU / impact_ray_count;
        let impact_jitter = settings.crack_jitter.clamp(0.0, 1.0);
        let impact_branching = settings.crack_branching.clamp(0.0, 1.0);
        let impact_ring_count = settings.stress_rings.min(32);
        let impact_ring_spacing = impact_radius / (impact_ring_count + 1) as f32;
        let impact_ring_jitter = settings.ring_jitter.clamp(0.0, 1.0);
        Self {
            settings,
            half_width,
            half_height,
            rotation_cos,
            rotation_sin,
            rounded_exponent,
            ring_half_band,
            facet_cell_size,
            facet_jitter,
            facet_relief,
            crack_depth,
            impact_radius,
            impact_aspect,
            impact_falloff,
            impact_crack_width,
            impact_sector,
            impact_jitter,
            impact_branching,
            impact_ring_count,
            impact_ring_spacing,
            impact_ring_jitter,
        }
    }

    pub(crate) fn height(&self, x: f32, y: f32) -> HeightSample {
        let dx = x - self.settings.center[0];
        let dy = y - self.settings.center[1];
        let local_x = dx * self.rotation_cos + dy * self.rotation_sin;
        let local_y = -dx * self.rotation_sin + dy * self.rotation_cos;

        match self.settings.shape {
            Shape::Sphere => {
                let nx = local_x / self.half_width;
                let ny = local_y / self.half_height;
                let radius = nx.hypot(ny);
                HeightSample {
                    value: (1.0 - radius * radius).max(0.0).sqrt(),
                    facet_id: 0,
                }
            }
            Shape::RoundedRectangle => {
                let nx = local_x / self.half_width;
                let ny = local_y / self.half_height;
                let distance = (nx.abs().powf(self.rounded_exponent)
                    + ny.abs().powf(self.rounded_exponent))
                .powf(1.0 / self.rounded_exponent);
                HeightSample {
                    value: (1.0 - distance).clamp(0.0, 1.0),
                    facet_id: 0,
                }
            }
            Shape::Diamond => {
                let nx = local_x / self.half_width;
                let ny = local_y / self.half_height;
                HeightSample {
                    value: (1.0 - nx.abs() - ny.abs()).clamp(0.0, 1.0),
                    facet_id: 0,
                }
            }
            Shape::Ring => {
                let nx = local_x / self.half_width;
                let ny = local_y / self.half_height;
                let distance = (nx.hypot(ny) - 0.65).abs();
                HeightSample {
                    value: (1.0 - distance / self.ring_half_band).clamp(0.0, 1.0),
                    facet_id: 0,
                }
            }
            Shape::Facets | Shape::Shards => fracture_field_prepared(local_x, local_y, self),
            Shape::ImpactGlass => impact_glass_prepared(local_x, local_y, self),
        }
    }
}

#[cfg(test)]
pub(crate) fn procedural_height(x: f32, y: f32, settings: ProceduralSettings) -> HeightSample {
    PreparedProcedural::new(settings).height(x, y)
}

fn fracture_field_prepared(x: f32, y: f32, prepared: &PreparedProcedural) -> HeightSample {
    let settings = prepared.settings;
    let cell = voronoi_prepared(
        x,
        y,
        prepared.facet_cell_size,
        prepared.facet_jitter,
        settings.seed,
    );
    let random_height = 0.2 + hash01(cell.id ^ 0xA511_E9B3) * 0.8;
    let mut height = 1.0 + (random_height - 1.0) * prepared.facet_relief;
    if settings.shape == Shape::Shards && settings.crack_width > 0.0 {
        let crack = cell_boundary_crack(cell, settings.crack_width);
        height *= 1.0 - crack * prepared.crack_depth;
    }
    HeightSample {
        value: finite(height).clamp(0.0, 1.0),
        facet_id: cell.id,
    }
}

/// A full-frame tessellated height field shared by the legacy Facets and
/// Shards popup values. `Size` no longer creates an unrelated circular mask;
/// Cell Size is the only spatial scale for these two modes. This preserves the
/// old popup indices while making the two modes a coherent uncracked/cracked
/// pair.
#[cfg(test)]
fn fracture_field(x: f32, y: f32, settings: ProceduralSettings) -> HeightSample {
    let cell = voronoi(
        x,
        y,
        settings.facet_size,
        settings.facet_jitter,
        settings.seed,
    );
    let random_height = 0.2 + hash01(cell.id ^ 0xA511_E9B3) * 0.8;
    let relief = settings.facet_amount.clamp(0.0, 1.0);
    let mut height = 1.0 + (random_height - 1.0) * relief;
    if settings.shape == Shape::Shards && settings.crack_width > 0.0 {
        let crack = cell_boundary_crack(cell, settings.crack_width);
        height *= 1.0 - crack * settings.crack_depth.clamp(0.0, 1.0);
    }
    HeightSample {
        value: finite(height).clamp(0.0, 1.0),
        facet_id: cell.id,
    }
}

/// Produces an impact-origin fracture field from three structures seen in
/// cracked glass: primary radial cracks, shorter branches, and interrupted
/// concentric stress rings. A Voronoi relief underneath them gives each
/// resulting region a stable, deterministic optical normal.
fn impact_glass_prepared(x: f32, y: f32, prepared: &PreparedProcedural) -> HeightSample {
    let settings = prepared.settings;
    let impact_radius = prepared.impact_radius;
    // Work in an isotropic impact space. Aspect changes the ellipse in image
    // space without making crack widths or radial density direction-dependent.
    let impact_x = x;
    let impact_y = y * prepared.impact_aspect;
    let radius = impact_x.hypot(impact_y);
    let normalized_radius = radius / impact_radius;
    if normalized_radius >= 1.0 {
        return HeightSample {
            value: 0.0,
            facet_id: 0,
        };
    }

    let cell = voronoi_prepared(
        impact_x,
        impact_y,
        prepared.facet_cell_size,
        prepared.facet_jitter,
        settings.seed,
    );
    let random_height = 0.2 + hash01(cell.id ^ 0xA511_E9B3) * 0.8;
    let facet_height = 1.0 + (random_height - 1.0) * prepared.facet_relief;

    let edge_falloff = prepared.impact_falloff;
    let envelope = if edge_falloff <= 1.0e-5 {
        1.0
    } else {
        1.0 - smoothstep(1.0 - edge_falloff, 1.0, normalized_radius)
    };

    let structural_crack = impact_crack_mask_prepared(impact_x, impact_y, prepared);
    // Fine polygon boundaries are strongest near the strike and become only
    // subtle facet seams toward the edge.
    let cell_crack = cell_boundary_crack(cell, settings.crack_width * 0.7)
        * (1.0 - normalized_radius).sqrt()
        * 0.45;
    let crack = structural_crack.max(cell_crack).clamp(0.0, 1.0);
    let height = envelope * facet_height * (1.0 - crack * prepared.crack_depth);

    HeightSample {
        value: finite(height).clamp(0.0, 1.0),
        facet_id: cell.id,
    }
}

fn impact_crack_mask_prepared(x: f32, y: f32, prepared: &PreparedProcedural) -> f32 {
    let settings = prepared.settings;
    let impact_radius = prepared.impact_radius;
    let radius = x.hypot(y);
    let theta = y.atan2(x);
    let crack_width = prepared.impact_crack_width;
    if crack_width <= 1.0e-5 {
        return 0.0;
    }

    let sector = prepared.impact_sector;
    let center_ray = (theta / sector).round() as i32;
    let normalized_radius = (radius / impact_radius).clamp(0.0, 1.0);
    let jitter = prepared.impact_jitter;
    let branching = prepared.impact_branching;
    let mut mask: f32 = 0.0;

    // Only nearby angular sectors can affect the pixel, which keeps the
    // procedural field practical even with dozens of radial cracks.
    for candidate in (center_ray - 2)..=(center_ray + 2) {
        let ray_id = cell_hash(candidate, 0, settings.seed ^ 0xD816_3841);
        let base_angle = candidate as f32 * sector
            + (hash01(ray_id ^ 0xB529_7A4D) - 0.5) * sector * jitter * 0.85;
        let phase = hash01(ray_id ^ 0x68E3_1DA4) * TAU;
        let frequency = 1.5 + hash01(ray_id ^ 0x1B56_C4E9) * 3.5;
        let wobble = (normalized_radius * frequency * TAU + phase).sin()
            * sector
            * jitter
            * 0.11
            * (0.25 + normalized_radius * 0.75);
        let ray_angle = base_angle + wobble;
        let angular_distance = wrap_angle(theta - ray_angle).abs() * radius;
        let ray_length = impact_radius * (0.58 + hash01(ray_id ^ 0xC2B2_AE35) * 0.42);
        let length_gate = 1.0
            - smoothstep(
                (ray_length - crack_width * 2.0).max(0.0),
                ray_length + crack_width,
                radius,
            );
        mask = mask.max(crack_profile(angular_distance, crack_width) * length_gate);

        // Two deterministic branch opportunities per primary ray. Branching
        // controls both how many become active and their reach.
        for branch_index in 0..2_u32 {
            let branch_id = ray_id ^ branch_index.wrapping_mul(0x9E37_79B9) ^ 0xA24B_AED4;
            let threshold = 0.12 + hash01(branch_id ^ 0x91E1_0DA5) * 0.72;
            let activation = smoothstep(threshold, (threshold + 0.22).min(1.0), branching);
            if activation <= 0.0 {
                continue;
            }
            let start_radius = impact_radius * (0.18 + hash01(branch_id ^ 0xDB4F_0B91) * 0.48);
            let start_wobble = (start_radius / impact_radius * frequency * TAU + phase).sin()
                * sector
                * jitter
                * 0.11;
            let start_angle = base_angle + start_wobble;
            let start = [
                start_angle.cos() * start_radius,
                start_angle.sin() * start_radius,
            ];
            let direction_sign = if (branch_id & 1) == 0 { -1.0 } else { 1.0 };
            let branch_angle = start_angle
                + direction_sign
                    * (0.16 + hash01(branch_id ^ 0xBB67_AE85) * 0.48)
                    * (0.55 + branching * 0.45);
            let branch_length = impact_radius
                * (0.08 + hash01(branch_id ^ 0x3C6E_F372) * 0.28)
                * (0.45 + branching * 0.55);
            let end = [
                start[0] + branch_angle.cos() * branch_length,
                start[1] + branch_angle.sin() * branch_length,
            ];
            let distance = point_segment_distance([x, y], start, end);
            mask = mask.max(crack_profile(distance, crack_width * 0.72) * activation);
        }
    }

    let ring_count = prepared.impact_ring_count;
    if ring_count > 0 {
        let spacing = prepared.impact_ring_spacing;
        let ring_jitter = prepared.impact_ring_jitter;
        for ring in 1..=ring_count {
            let ring_id = cell_hash(ring as i32, 1, settings.seed ^ 0x243F_6A88);
            let base_radius = impact_radius * (ring as f32 / (ring_count + 1) as f32).powf(0.82);
            let offset = (hash01(ring_id ^ 0x85A3_08D3) - 0.5) * spacing * ring_jitter * 0.65;
            let lobes = 3.0 + (ring_id % 6) as f32;
            let phase = hash01(ring_id ^ 0x1319_8A2E) * TAU;
            let wave = (theta * lobes + phase).sin() * spacing * ring_jitter * 0.22;
            let ring_distance = (radius - base_radius - offset - wave).abs();

            // Break perfect circles into irregular arcs. Adjacent angular
            // segments share a deterministic on/off value.
            let arc_count = 7 + (ring_id % 8) as i32;
            let arc = (((theta + std::f32::consts::PI) / TAU) * arc_count as f32).floor() as i32;
            let arc_id = cell_hash(arc, ring as i32, settings.seed ^ 0x9E37_79B9);
            let arc_gate = smoothstep(0.12, 0.38, hash01(arc_id));
            mask = mask.max(crack_profile(ring_distance, crack_width * 0.72) * arc_gate);
        }
    }

    // A small crushed zone anchors all radial cracks at the strike point.
    let center_radius = impact_radius * (0.012 + 0.025 * jitter);
    mask.max(1.0 - smoothstep(center_radius * 0.35, center_radius, radius))
        .clamp(0.0, 1.0)
}

fn cell_boundary_crack(cell: Voronoi, width: f32) -> f32 {
    if width <= 1.0e-5 {
        return 0.0;
    }
    let boundary_gap = (cell.second_distance - cell.nearest_distance).max(0.0);
    1.0 - smoothstep(0.0, width, boundary_gap)
}

fn crack_profile(distance: f32, width: f32) -> f32 {
    if width <= 1.0e-5 {
        return 0.0;
    }
    1.0 - smoothstep(width * 0.2, width, distance)
}

fn point_segment_distance(point: [f32; 2], start: [f32; 2], end: [f32; 2]) -> f32 {
    let segment = [end[0] - start[0], end[1] - start[1]];
    let length_squared = segment[0].mul_add(segment[0], segment[1] * segment[1]);
    if length_squared <= 1.0e-8 {
        return (point[0] - start[0]).hypot(point[1] - start[1]);
    }
    let offset = [point[0] - start[0], point[1] - start[1]];
    let t = ((offset[0] * segment[0] + offset[1] * segment[1]) / length_squared).clamp(0.0, 1.0);
    (point[0] - (start[0] + segment[0] * t)).hypot(point[1] - (start[1] + segment[1] * t))
}

fn wrap_angle(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI
}

pub(crate) fn central_difference<F>(
    x: f32,
    y: f32,
    radius: f32,
    strength: f32,
    mut height_at: F,
) -> [f32; 2]
where
    F: FnMut(f32, f32) -> f32,
{
    let radius = radius.max(0.25);
    let dx = (height_at(x + radius, y) - height_at(x - radius, y)) / (2.0 * radius);
    let dy = (height_at(x, y + radius) - height_at(x, y - radius)) / (2.0 * radius);
    let nx = -dx * strength;
    let ny = -dy * strength;
    [finite(nx), finite(ny)]
}

pub(crate) fn map_level(value: f32, black: f32, white: f32, invert: bool) -> f32 {
    let denominator = white - black;
    let mut normalized = if denominator.abs() <= 1.0e-6 {
        if value >= white { 1.0 } else { 0.0 }
    } else {
        ((value - black) / denominator).clamp(0.0, 1.0)
    };
    if invert {
        normalized = 1.0 - normalized;
    }
    finite(normalized).clamp(0.0, 1.0)
}

pub(crate) fn facet_color(id: u32) -> [f32; 3] {
    [
        hash01(id ^ 0x68BC_21EB),
        hash01(id ^ 0x02E5_BE93),
        hash01(id ^ 0x967A_889B),
    ]
}

#[derive(Clone, Copy, Debug)]
struct Voronoi {
    nearest_distance: f32,
    second_distance: f32,
    id: u32,
}

#[cfg(test)]
fn voronoi(x: f32, y: f32, cell_size: f32, jitter: f32, seed: u32) -> Voronoi {
    let cell_size = cell_size.max(1.0);
    let jitter = jitter.clamp(0.0, 1.0);
    voronoi_prepared(x, y, cell_size, jitter, seed)
}

fn voronoi_prepared(x: f32, y: f32, cell_size: f32, jitter: f32, seed: u32) -> Voronoi {
    let gx = x / cell_size;
    let gy = y / cell_size;
    let base_x = gx.floor() as i32;
    let base_y = gy.floor() as i32;
    let mut nearest = f32::INFINITY;
    let mut second = f32::INFINITY;
    let mut nearest_id = 0;
    for oy in -1..=1 {
        for ox in -1..=1 {
            let cx = base_x + ox;
            let cy = base_y + oy;
            let id = cell_hash(cx, cy, seed);
            let angle = hash01(id ^ 0x9E37_79B9) * TAU;
            let distance = hash01(id ^ 0xD1B5_4A35) * 0.5 * jitter;
            let feature_x = cx as f32 + 0.5 + angle.cos() * distance;
            let feature_y = cy as f32 + 0.5 + angle.sin() * distance;
            let d = (gx - feature_x).hypot(gy - feature_y) * cell_size;
            if d < nearest {
                second = nearest;
                nearest = d;
                nearest_id = id;
            } else if d < second {
                second = d;
            }
        }
    }
    Voronoi {
        nearest_distance: nearest,
        second_distance: second,
        id: nearest_id,
    }
}

fn cell_hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut value =
        seed ^ (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846C_A68B);
    value ^ (value >> 16)
}

fn hash01(value: u32) -> f32 {
    (cell_hash(value as i32, (value >> 16) as i32, 0xC2B2_AE35) as f64 / u32::MAX as f64) as f32
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if (edge1 - edge0).abs() <= 1.0e-6 {
        return if value < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn finite(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(shape: Shape) -> ProceduralSettings {
        ProceduralSettings {
            shape,
            center: [50.0, 50.0],
            size: 100.0,
            aspect: 1.0,
            rotation: 0.0,
            roundness: 0.5,
            ring_width: 0.25,
            facet_size: 12.0,
            facet_amount: 1.0,
            facet_jitter: 0.8,
            crack_width: 2.0,
            crack_depth: 0.9,
            radial_cracks: 18,
            crack_branching: 0.55,
            crack_jitter: 0.45,
            stress_rings: 6,
            ring_jitter: 0.35,
            impact_falloff: 0.2,
            seed: 42,
        }
    }

    fn legacy_procedural_height(x: f32, y: f32, settings: ProceduralSettings) -> HeightSample {
        let half_width = (settings.size * 0.5).max(1.0e-4);
        let half_height = (half_width / settings.aspect.max(1.0e-4)).max(1.0e-4);
        let dx = x - settings.center[0];
        let dy = y - settings.center[1];
        let cos = settings.rotation.cos();
        let sin = settings.rotation.sin();
        let local_x = dx * cos + dy * sin;
        let local_y = -dx * sin + dy * cos;
        let nx = local_x / half_width;
        let ny = local_y / half_height;
        let radius = nx.hypot(ny);

        match settings.shape {
            Shape::Sphere => HeightSample {
                value: (1.0 - radius * radius).max(0.0).sqrt(),
                facet_id: 0,
            },
            Shape::RoundedRectangle => {
                let exponent = 16.0 - settings.roundness.clamp(0.0, 1.0) * 14.0;
                let distance =
                    (nx.abs().powf(exponent) + ny.abs().powf(exponent)).powf(1.0 / exponent);
                HeightSample {
                    value: (1.0 - distance).clamp(0.0, 1.0),
                    facet_id: 0,
                }
            }
            Shape::Diamond => HeightSample {
                value: (1.0 - nx.abs() - ny.abs()).clamp(0.0, 1.0),
                facet_id: 0,
            },
            Shape::Ring => {
                let half_band = settings.ring_width.clamp(0.01, 1.0) * 0.5;
                let distance = (radius - 0.65).abs();
                HeightSample {
                    value: (1.0 - distance / half_band).clamp(0.0, 1.0),
                    facet_id: 0,
                }
            }
            Shape::Facets | Shape::Shards => fracture_field(local_x, local_y, settings),
            Shape::ImpactGlass => legacy_impact_glass(local_x, local_y, settings),
        }
    }

    fn legacy_impact_glass(x: f32, y: f32, settings: ProceduralSettings) -> HeightSample {
        let impact_radius = (settings.size * 0.5).max(1.0e-4);
        let impact_x = x;
        let impact_y = y * settings.aspect.max(1.0e-4);
        let radius = impact_x.hypot(impact_y);
        let normalized_radius = radius / impact_radius;
        if normalized_radius >= 1.0 {
            return HeightSample {
                value: 0.0,
                facet_id: 0,
            };
        }

        let cell = voronoi(
            impact_x,
            impact_y,
            settings.facet_size,
            settings.facet_jitter,
            settings.seed,
        );
        let random_height = 0.2 + hash01(cell.id ^ 0xA511_E9B3) * 0.8;
        let relief = settings.facet_amount.clamp(0.0, 1.0);
        let facet_height = 1.0 + (random_height - 1.0) * relief;
        let edge_falloff = settings.impact_falloff.clamp(0.0, 1.0);
        let envelope = if edge_falloff <= 1.0e-5 {
            1.0
        } else {
            1.0 - smoothstep(1.0 - edge_falloff, 1.0, normalized_radius)
        };
        let structural_crack =
            impact_crack_mask_prepared(impact_x, impact_y, &PreparedProcedural::new(settings));
        let cell_crack = cell_boundary_crack(cell, settings.crack_width * 0.7)
            * (1.0 - normalized_radius).sqrt()
            * 0.45;
        let crack = structural_crack.max(cell_crack).clamp(0.0, 1.0);
        let height = envelope * facet_height * (1.0 - crack * settings.crack_depth.clamp(0.0, 1.0));
        HeightSample {
            value: finite(height).clamp(0.0, 1.0),
            facet_id: cell.id,
        }
    }

    #[test]
    fn prepared_procedural_is_bitwise_equivalent_to_legacy_shape_math() {
        for shape in [
            Shape::Sphere,
            Shape::RoundedRectangle,
            Shape::Diamond,
            Shape::Ring,
            Shape::Facets,
            Shape::Shards,
            Shape::ImpactGlass,
        ] {
            let mut controls = settings(shape);
            controls.aspect = 1.37;
            controls.rotation = 0.43;
            controls.roundness = 0.63;
            controls.ring_width = 0.31;
            let prepared = PreparedProcedural::new(controls);
            for [x, y] in [
                [3.25, -7.5],
                [36.125, 41.75],
                [50.0, 50.0],
                [63.5, 57.25],
                [112.0, 91.0],
            ] {
                let expected = legacy_procedural_height(x, y, controls);
                let actual = prepared.height(x, y);
                assert_eq!(
                    actual.value.to_bits(),
                    expected.value.to_bits(),
                    "{shape:?}"
                );
                assert_eq!(actual.facet_id, expected.facet_id, "{shape:?}");
            }
        }
    }

    #[test]
    fn prepared_procedural_builds_render_constants_once() {
        let controls = settings(Shape::ImpactGlass);
        PREPARED_PROCEDURAL_BUILDS.with(|builds| builds.set(0));
        let prepared = PreparedProcedural::new(controls);
        for x in 0..16 {
            let _ = prepared.height(x as f32 * 3.25, 42.0);
        }
        PREPARED_PROCEDURAL_BUILDS.with(|builds| assert_eq!(builds.get(), 1));

        PREPARED_PROCEDURAL_BUILDS.with(|builds| builds.set(0));
        for x in 0..16 {
            let _ = procedural_height(x as f32 * 3.25, 42.0, controls);
        }
        PREPARED_PROCEDURAL_BUILDS.with(|builds| assert_eq!(builds.get(), 16));
    }

    #[test]
    fn bounded_shapes_are_high_at_center_and_zero_outside() {
        for shape in [Shape::Sphere, Shape::RoundedRectangle, Shape::Diamond] {
            let controls = settings(shape);
            assert!(procedural_height(50.0, 50.0, controls).value > 0.9);
            assert_eq!(procedural_height(200.0, 50.0, controls).value, 0.0);
        }
    }

    #[test]
    fn ring_peaks_away_from_center() {
        let controls = settings(Shape::Ring);
        assert_eq!(procedural_height(50.0, 50.0, controls).value, 0.0);
        assert!(procedural_height(82.5, 50.0, controls).value > 0.99);
    }

    #[test]
    fn flat_height_has_zero_normal() {
        let normal = central_difference(10.0, 20.0, 1.0, 100.0, |_, _| 0.5);
        assert_eq!(normal, [0.0, 0.0]);
    }

    #[test]
    fn central_difference_detects_ramp_direction() {
        let normal = central_difference(0.0, 0.0, 1.0, 2.0, |x, y| x + y * 2.0);
        assert!((normal[0] + 2.0).abs() < 1.0e-6);
        assert!((normal[1] + 4.0).abs() < 1.0e-6);
    }

    #[test]
    fn voronoi_is_deterministic() {
        let controls = settings(Shape::Facets);
        let first = procedural_height(40.25, 60.75, controls);
        let second = procedural_height(40.25, 60.75, controls);
        assert_eq!(first.value.to_bits(), second.value.to_bits());
        assert_eq!(first.facet_id, second.facet_id);
    }

    #[test]
    fn facets_follow_the_procedural_center() {
        let controls = settings(Shape::Facets);
        let first = procedural_height(40.25, 60.75, controls);
        let mut moved = controls;
        moved.center = [70.0, 80.0];
        let second = procedural_height(60.25, 90.75, moved);
        assert_eq!(first.value.to_bits(), second.value.to_bits());
        assert_eq!(first.facet_id, second.facet_id);
    }

    #[test]
    fn facet_field_is_not_clipped_by_the_size_circle() {
        let controls = settings(Shape::Facets);
        let far_away = procedural_height(5_000.0, -3_000.0, controls);
        assert!(far_away.value > 0.0);
        assert_ne!(far_away.facet_id, 0);
    }

    #[test]
    fn impact_glass_is_bounded_and_deterministic() {
        let controls = settings(Shape::ImpactGlass);
        let first = procedural_height(72.0, 61.0, controls);
        let second = procedural_height(72.0, 61.0, controls);
        assert_eq!(first.value.to_bits(), second.value.to_bits());
        assert_eq!(first.facet_id, second.facet_id);
        assert_ne!(first.facet_id, 0);
        assert_eq!(procedural_height(200.0, 50.0, controls).value, 0.0);
    }

    #[test]
    fn impact_cracks_start_at_the_strike_point() {
        let controls = settings(Shape::ImpactGlass);
        let center = procedural_height(50.0, 50.0, controls);
        assert!(center.value < 0.2);
    }

    #[test]
    fn map_levels_support_inversion_and_reversed_range() {
        assert!((map_level(0.5, 0.0, 1.0, false) - 0.5).abs() < 1.0e-6);
        assert!((map_level(0.25, 0.0, 1.0, true) - 0.75).abs() < 1.0e-6);
        assert!((map_level(0.25, 1.0, 0.0, false) - 0.75).abs() < 1.0e-6);
    }

    #[test]
    fn facet_debug_color_is_stable_and_bounded() {
        let color = facet_color(1234);
        assert!(color.iter().all(|value| (0.0..=1.0).contains(value)));
        assert_eq!(color, facet_color(1234));
    }
}
