use super::{
    BlendMode, DirectionField, ExpansionSample, InterpolationMode, PixelF32, RenderContext,
    Settings,
};
use ::wgpu::*;
use bytemuck::{Pod, Zeroable};
use futures_intrusive::channel::shared::oneshot_channel;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

const WORKGROUP_SIZE: u32 = 16;
const AUTO_GPU_WORK_THRESHOLD: usize = 250_000;

static WGPU_CONTEXT: OnceLock<Result<Arc<WgpuContext>, String>> = OnceLock::new();

pub(super) fn should_render(context: &RenderContext) -> bool {
    match std::env::var("AOD_PIXEL_EXTEND_GPU").as_deref() {
        Ok("0") | Ok("false") | Ok("off") => return false,
        Ok("1") | Ok("true") | Ok("on") => return true,
        _ => {}
    }

    let samples_per_pixel = context.plan.paths.iter().fold(0usize, |total, path| {
        total.saturating_add(path.steps.iter().fold(0usize, |path_total, step| {
            path_total.saturating_add(
                if step.expansion_radius > super::EPSILON
                    && context.plan.expansion_samples.len() > 1
                {
                    context.plan.expansion_samples.len()
                } else {
                    1
                },
            )
        }))
    });
    context
        .region
        .width()
        .saturating_mul(context.region.height())
        .saturating_mul(samples_per_pixel)
        >= AUTO_GPU_WORK_THRESHOLD
}

pub(super) fn render(
    source: &[PixelF32],
    masks: &[f32],
    width: usize,
    height: usize,
    settings: &Settings,
    context: &RenderContext,
) -> Result<Vec<PixelF32>, String> {
    let gpu = match WGPU_CONTEXT.get_or_init(|| WgpuContext::new().map(Arc::new)) {
        Ok(context) => Arc::clone(context),
        Err(error) => return Err(error.clone()),
    };
    gpu.render(source, masks, width, height, settings, context)
}

struct WgpuContext {
    device: Device,
    queue: Queue,
    limits: Limits,
    pipeline: ComputePipeline,
    layout: BindGroupLayout,
    resources: Mutex<HashMap<std::thread::ThreadId, Arc<Mutex<WgpuResources>>>>,
}

impl WgpuContext {
    fn new() -> Result<Self, String> {
        let power_preference =
            PowerPreference::from_env().unwrap_or(PowerPreference::HighPerformance);
        let mut instance_desc = InstanceDescriptor::default();
        if instance_desc.backends.contains(Backends::DX12)
            && instance_desc.flags.contains(InstanceFlags::VALIDATION)
        {
            instance_desc
                .flags
                .remove(InstanceFlags::VALIDATION | InstanceFlags::GPU_BASED_VALIDATION);
        }

        let instance = Instance::new(&instance_desc);
        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference,
            ..Default::default()
        }))
        .map_err(|error| format!("request_adapter failed: {error:?}"))?;
        let required_limits = Limits::default()
            .using_resolution(adapter.limits())
            .using_alignment(adapter.limits());
        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: Some("AOD_PixelExtend device"),
            required_features: Features::empty(),
            required_limits: required_limits.clone(),
            experimental_features: ExperimentalFeatures::disabled(),
            memory_hints: MemoryHints::Performance,
            trace: Trace::Off,
        }))
        .map_err(|error| format!("request_device failed: {error:?}"))?;

        let (layout, pipeline) = create_pipeline(&device);
        Ok(Self {
            device,
            queue,
            limits: required_limits,
            pipeline,
            layout,
            resources: Mutex::new(HashMap::new()),
        })
    }

    fn render(
        &self,
        source: &[PixelF32],
        masks: &[f32],
        width: usize,
        height: usize,
        settings: &Settings,
        context: &RenderContext,
    ) -> Result<Vec<PixelF32>, String> {
        let pixel_count = width
            .checked_mul(height)
            .ok_or_else(|| "image dimensions overflow".to_owned())?;
        if source.len() != pixel_count || masks.len() != pixel_count {
            return Err("source buffer size does not match image dimensions".to_owned());
        }
        let width_u32 = u32::try_from(width).map_err(|_| "image width exceeds u32".to_owned())?;
        let height_u32 =
            u32::try_from(height).map_err(|_| "image height exceeds u32".to_owned())?;
        let steps_per_path = context
            .plan
            .paths
            .first()
            .map_or(0, |path| path.steps.len());
        if steps_per_path == 0
            || context
                .plan
                .paths
                .iter()
                .any(|path| path.steps.len() != steps_per_path)
        {
            return Err("GPU paths must have equal, nonzero step counts".to_owned());
        }

        let source_data = source
            .iter()
            .map(|pixel| [pixel.red, pixel.green, pixel.blue, pixel.alpha])
            .collect::<Vec<_>>();
        let (direction_data, base_direction, variable_direction) = match &context.direction_field {
            DirectionField::Constant(direction) => (
                vec![[direction.x, direction.y]],
                [direction.x, direction.y],
                false,
            ),
            DirectionField::PerPixel { directions, .. } => (
                directions
                    .iter()
                    .map(|direction| [direction.x, direction.y])
                    .collect(),
                [0.0, 0.0],
                true,
            ),
        };
        if variable_direction
            && direction_data.len()
                != context
                    .region
                    .width()
                    .saturating_mul(context.region.height())
        {
            return Err("direction buffer size does not match the render region".to_owned());
        }
        let steps = context
            .plan
            .paths
            .iter()
            .flat_map(|path| {
                path.steps.iter().map(move |step| GpuPathStep {
                    geometry: [
                        step.source_offset_x,
                        step.source_offset_y,
                        step.expansion_radius,
                        step.falloff,
                    ],
                    metadata: [path.normal_sign, 0.0, 0.0, 0.0],
                })
            })
            .collect::<Vec<_>>();
        let expansion_samples = context
            .plan
            .expansion_samples
            .iter()
            .map(|sample| pack_expansion_sample(*sample))
            .collect::<Vec<_>>();

        let key = ResourceKey {
            pixel_count,
            direction_count: direction_data.len(),
            step_count: steps.len(),
            expansion_count: expansion_samples.len(),
        };
        self.validate_resource_sizes(key, width_u32, height_u32)?;
        let resource = {
            let mut resources = self
                .resources
                .lock()
                .map_err(|_| "GPU resource cache lock was poisoned".to_owned())?;
            Arc::clone(
                resources
                    .entry(std::thread::current().id())
                    .or_insert_with(|| {
                        Arc::new(Mutex::new(WgpuResources::new(
                            &self.device,
                            &self.layout,
                            key,
                        )))
                    }),
            )
        };
        let mut resource = resource
            .lock()
            .map_err(|_| "GPU thread resource lock was poisoned".to_owned())?;
        if !resource.key.can_hold(key) {
            *resource = WgpuResources::new(&self.device, &self.layout, key);
        }

        let params = GpuParams {
            image: [
                width_u32,
                height_u32,
                interpolation_code(settings.interpolation),
                u32::from(variable_direction),
            ],
            region: [
                context.region.min_x as u32,
                context.region.min_y as u32,
                context.region.max_x as u32,
                context.region.max_y as u32,
            ],
            counts: [
                context.plan.paths.len() as u32,
                steps_per_path as u32,
                expansion_samples.len() as u32,
                blend_mode_code(settings.blend_mode),
            ],
            flags: [u32::from(settings.show_source), 0, 0, 0],
            direction: [base_direction[0], base_direction[1], 0.0, 0.0],
            sampling: [
                settings.mitchell_b,
                settings.mitchell_c,
                settings.opacity,
                0.0,
            ],
        };

        self.queue
            .write_buffer(&resource.params, 0, bytemuck::bytes_of(&params));
        self.queue.write_buffer(
            &resource.source,
            0,
            bytemuck::cast_slice(source_data.as_slice()),
        );
        self.queue
            .write_buffer(&resource.masks, 0, bytemuck::cast_slice(masks));
        self.queue.write_buffer(
            &resource.directions,
            0,
            bytemuck::cast_slice(direction_data.as_slice()),
        );
        self.queue
            .write_buffer(&resource.steps, 0, bytemuck::cast_slice(steps.as_slice()));
        self.queue.write_buffer(
            &resource.expansion_samples,
            0,
            bytemuck::cast_slice(expansion_samples.as_slice()),
        );

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("AOD_PixelExtend encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("AOD_PixelExtend compute"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &resource.bind_group, &[]);
            pass.dispatch_workgroups(
                width_u32.div_ceil(WORKGROUP_SIZE),
                height_u32.div_ceil(WORKGROUP_SIZE),
                1,
            );
        }
        let output_bytes = buffer_bytes::<[f32; 4]>(pixel_count);
        encoder.copy_buffer_to_buffer(&resource.output, 0, &resource.staging, 0, output_bytes);

        let (sender, receiver) = oneshot_channel();
        encoder.map_buffer_on_submit(
            &resource.staging,
            MapMode::Read,
            ..output_bytes,
            move |result| {
                let _ = sender.send(result);
            },
        );
        self.queue.submit(Some(encoder.finish()));
        let _ = self.device.poll(PollType::wait_indefinitely());

        let Some(Ok(())) = pollster::block_on(receiver.receive()) else {
            return Err("GPU output readback failed".to_owned());
        };
        let mapped = resource.staging.slice(..output_bytes).get_mapped_range();
        let output_data: &[[f32; 4]] = bytemuck::cast_slice(&mapped);
        let output = output_data
            .iter()
            .take(pixel_count)
            .map(|pixel| PixelF32 {
                alpha: pixel[3],
                red: pixel[0],
                green: pixel[1],
                blue: pixel[2],
            })
            .collect();
        drop(mapped);
        resource.staging.unmap();
        Ok(output)
    }

    fn validate_resource_sizes(
        &self,
        key: ResourceKey,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let storage_limit = self.limits.max_storage_buffer_binding_size as u64;
        for (label, size) in [
            ("source/output", buffer_bytes::<[f32; 4]>(key.pixel_count)),
            ("masks", buffer_bytes::<f32>(key.pixel_count)),
            ("directions", buffer_bytes::<[f32; 2]>(key.direction_count)),
            ("path steps", buffer_bytes::<GpuPathStep>(key.step_count)),
            (
                "expansion samples",
                buffer_bytes::<GpuExpansionSample>(key.expansion_count),
            ),
        ] {
            if size > storage_limit || size > self.limits.max_buffer_size {
                return Err(format!(
                    "{label} buffer requires {size} bytes, exceeding GPU limits"
                ));
            }
        }

        let dispatch_limit = self.limits.max_compute_workgroups_per_dimension;
        if width.div_ceil(WORKGROUP_SIZE) > dispatch_limit
            || height.div_ceil(WORKGROUP_SIZE) > dispatch_limit
        {
            return Err("image dimensions exceed GPU dispatch limits".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ResourceKey {
    pixel_count: usize,
    direction_count: usize,
    step_count: usize,
    expansion_count: usize,
}

impl ResourceKey {
    fn can_hold(self, required: Self) -> bool {
        self.pixel_count >= required.pixel_count
            && self.direction_count >= required.direction_count
            && self.step_count >= required.step_count
            && self.expansion_count >= required.expansion_count
    }
}

struct WgpuResources {
    key: ResourceKey,
    params: Buffer,
    source: Buffer,
    masks: Buffer,
    directions: Buffer,
    steps: Buffer,
    expansion_samples: Buffer,
    output: Buffer,
    staging: Buffer,
    bind_group: BindGroup,
}

impl WgpuResources {
    fn new(device: &Device, layout: &BindGroupLayout, key: ResourceKey) -> Self {
        let source_bytes = buffer_bytes::<[f32; 4]>(key.pixel_count);
        let mask_bytes = buffer_bytes::<f32>(key.pixel_count);
        let direction_bytes = buffer_bytes::<[f32; 2]>(key.direction_count);
        let step_bytes = buffer_bytes::<GpuPathStep>(key.step_count);
        let expansion_bytes = buffer_bytes::<GpuExpansionSample>(key.expansion_count);
        let params = create_buffer(
            device,
            "AOD_PixelExtend params",
            std::mem::size_of::<GpuParams>() as u64,
            BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        );
        let source = create_buffer(
            device,
            "AOD_PixelExtend source",
            source_bytes,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let masks = create_buffer(
            device,
            "AOD_PixelExtend masks",
            mask_bytes,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let directions = create_buffer(
            device,
            "AOD_PixelExtend directions",
            direction_bytes,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let steps = create_buffer(
            device,
            "AOD_PixelExtend steps",
            step_bytes,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let expansion_samples = create_buffer(
            device,
            "AOD_PixelExtend expansion samples",
            expansion_bytes,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let output = create_buffer(
            device,
            "AOD_PixelExtend output",
            source_bytes,
            BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        );
        let staging = create_buffer(
            device,
            "AOD_PixelExtend staging",
            source_bytes,
            BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        );
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("AOD_PixelExtend bind group"),
            layout,
            entries: &[
                bind_entry(0, &params),
                bind_entry(1, &source),
                bind_entry(2, &masks),
                bind_entry(3, &directions),
                bind_entry(4, &steps),
                bind_entry(5, &expansion_samples),
                bind_entry(6, &output),
            ],
        });

        Self {
            key,
            params,
            source,
            masks,
            directions,
            steps,
            expansion_samples,
            output,
            staging,
            bind_group,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuParams {
    image: [u32; 4],
    region: [u32; 4],
    counts: [u32; 4],
    flags: [u32; 4],
    direction: [f32; 4],
    sampling: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuPathStep {
    geometry: [f32; 4],
    metadata: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuExpansionSample {
    values: [f32; 4],
}

fn pack_expansion_sample(sample: ExpansionSample) -> GpuExpansionSample {
    GpuExpansionSample {
        values: [sample.radius_scale, sample.edge_weight, 0.0, 0.0],
    }
}

fn interpolation_code(mode: InterpolationMode) -> u32 {
    match mode {
        InterpolationMode::Nearest => 0,
        InterpolationMode::Bilinear => 1,
        InterpolationMode::Bicubic => 2,
        InterpolationMode::Mitchell => 3,
    }
}

fn blend_mode_code(mode: BlendMode) -> u32 {
    match mode {
        BlendMode::Normal => 0,
        BlendMode::Add => 1,
        BlendMode::Multiply => 2,
        BlendMode::Screen => 3,
        BlendMode::Overlay => 4,
        BlendMode::Darken => 5,
        BlendMode::Lighten => 6,
    }
}

fn buffer_bytes<T>(count: usize) -> u64 {
    (count.max(1) as u64).saturating_mul(std::mem::size_of::<T>() as u64)
}

fn create_buffer(device: &Device, label: &'static str, size: u64, usage: BufferUsages) -> Buffer {
    device.create_buffer(&BufferDescriptor {
        label: Some(label),
        size,
        usage,
        mapped_at_creation: false,
    })
}

fn bind_entry(binding: u32, buffer: &Buffer) -> BindGroupEntry<'_> {
    BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

fn create_pipeline(device: &Device) -> (BindGroupLayout, ComputePipeline) {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("AOD_PixelExtend compute shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/pixel_extend.wgsl"))),
    });
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("AOD_PixelExtend bind group layout"),
        entries: &[
            buffer_layout_entry(
                0,
                BufferBindingType::Uniform,
                std::mem::size_of::<GpuParams>(),
            ),
            buffer_layout_entry(1, BufferBindingType::Storage { read_only: true }, 0),
            buffer_layout_entry(2, BufferBindingType::Storage { read_only: true }, 0),
            buffer_layout_entry(3, BufferBindingType::Storage { read_only: true }, 0),
            buffer_layout_entry(4, BufferBindingType::Storage { read_only: true }, 0),
            buffer_layout_entry(5, BufferBindingType::Storage { read_only: true }, 0),
            buffer_layout_entry(6, BufferBindingType::Storage { read_only: false }, 0),
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("AOD_PixelExtend pipeline layout"),
        bind_group_layouts: &[&layout],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("AOD_PixelExtend pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: Default::default(),
    });
    (layout, pipeline)
}

fn buffer_layout_entry(
    binding: u32,
    ty: BufferBindingType,
    minimum_size: usize,
) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: BufferSize::new(minimum_size as u64),
        },
        count: None,
    }
}
