use super::{Prepared, image::Image};
use std::sync::{Mutex, OnceLock};
use wgpu::{self as w, util::DeviceExt};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniform {
    image: [u32; 4],
    flags: [u32; 4],
    field: [u32; 4],
    geometry: [f32; 4],
    exposure: [f32; 4],
    scale: [f32; 4],
    curve: [[f32; 4]; 64],
    displacement: [[f32; 4]; 256],
}
struct Buffers {
    input: w::Buffer,
    field: w::Buffer,
    levels: w::Buffer,
    uniform: w::Buffer,
    output: w::Buffer,
    staging: w::Buffer,
    bind: w::BindGroup,
    capacity: [u64; 3],
}
struct Context {
    device: w::Device,
    queue: w::Queue,
    pipeline: w::ComputePipeline,
    layout: w::BindGroupLayout,
    resources: Mutex<Option<Buffers>>,
}
static GPU: OnceLock<Result<Context, String>> = OnceLock::new();
impl Context {
    fn new() -> Result<Self, String> {
        let instance = w::Instance::new(&w::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&w::RequestAdapterOptions {
            power_preference: w::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        let limits = w::Limits {
            max_storage_buffer_binding_size: adapter.limits().max_storage_buffer_binding_size,
            max_buffer_size: adapter.limits().max_buffer_size,
            ..w::Limits::default()
                .using_resolution(adapter.limits())
                .using_alignment(adapter.limits())
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&w::DeviceDescriptor {
            label: Some("AOD Blur"),
            required_limits: limits,
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        let scope = device.push_error_scope(w::ErrorFilter::Validation);
        let shader = device.create_shader_module(w::ShaderModuleDescriptor {
            label: Some("AOD Blur kernel"),
            source: w::ShaderSource::Wgsl(include_str!("blur.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&w::ComputePipelineDescriptor {
            label: Some("AOD Blur"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(e.to_string());
        }
        let layout = pipeline.get_bind_group_layout(0);
        Ok(Self {
            device,
            queue,
            pipeline,
            layout,
            resources: Mutex::new(None),
        })
    }
    fn buffers(&self, capacity: [u64; 3]) -> Buffers {
        let buffer = |label, size, usage| {
            self.device.create_buffer(&w::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        let input = buffer(
            "blur input",
            capacity[0],
            w::BufferUsages::STORAGE | w::BufferUsages::COPY_DST,
        );
        let field = buffer(
            "blur field",
            capacity[1],
            w::BufferUsages::STORAGE | w::BufferUsages::COPY_DST,
        );
        let levels = self
            .device
            .create_buffer_init(&w::util::BufferInitDescriptor {
                label: Some("blur mip levels"),
                contents: &[0; 32 * 16],
                usage: w::BufferUsages::STORAGE | w::BufferUsages::COPY_DST,
            });
        let uniform = buffer(
            "blur settings",
            std::mem::size_of::<Uniform>() as u64,
            w::BufferUsages::UNIFORM | w::BufferUsages::COPY_DST,
        );
        let output = buffer(
            "blur output",
            capacity[2],
            w::BufferUsages::STORAGE | w::BufferUsages::COPY_SRC,
        );
        let staging = buffer(
            "blur readback",
            capacity[2],
            w::BufferUsages::MAP_READ | w::BufferUsages::COPY_DST,
        );
        let entries = [&input, &field, &levels, &uniform, &output]
            .iter()
            .enumerate()
            .map(|(i, b)| w::BindGroupEntry {
                binding: i as u32,
                resource: b.as_entire_binding(),
            })
            .collect::<Vec<_>>();
        let bind = self.device.create_bind_group(&w::BindGroupDescriptor {
            label: Some("blur bindings"),
            layout: &self.layout,
            entries: &entries,
        });
        Buffers {
            input,
            field,
            levels,
            uniform,
            output,
            staging,
            bind,
            capacity,
        }
    }
    fn render(&self, p: &Prepared) -> Result<Image, String> {
        let src = &p.levels[0];
        let s = &p.settings;
        let mut levels = Vec::<[u32; 4]>::new();
        let mut offset = 0u32;
        for image in &p.levels {
            levels.push([offset, image.width as u32, image.height as u32, 0]);
            offset = offset
                .checked_add(image.pixels.len() as u32)
                .ok_or("GPU pixel count overflow")?;
        }
        let sizes = [
            u64::from(offset) * 16,
            p.field.pixels.len() as u64 * 16,
            src.pixels.len() as u64 * 16,
        ];
        let limits = self.device.limits();
        if levels.len() > 32
            || sizes.iter().any(|&n| {
                n == 0
                    || n > u64::from(limits.max_storage_buffer_binding_size)
                    || n > limits.max_buffer_size
            })
            || (src.width as u32).div_ceil(8) > limits.max_compute_workgroups_per_dimension
            || (src.height as u32).div_ceil(8) > limits.max_compute_workgroups_per_dimension
        {
            return Err("Image exceeds GPU buffer or dispatch limits".into());
        }
        let uniform = Uniform {
            image: [
                src.width as u32,
                src.height as u32,
                s.mode,
                s.samples.clamp(2, 512),
            ],
            flags: [s.edge, s.interpolation, s.curve, levels.len() as u32],
            field: [p.field.width as u32, p.field.height as u32, 0, 0],
            geometry: [s.length, s.angle, s.center[0], s.center[1]],
            exposure: [s.shutter, s.bias, 0., 0.],
            scale: [s.scale[0], s.scale[1], 0., 0.],
            curve: std::array::from_fn(|i| std::array::from_fn(|j| s.curve_table[i * 4 + j])),
            displacement: std::array::from_fn(|i| {
                let a = p.displacement.get(i * 2).copied().unwrap_or([0.; 2]);
                let b = p.displacement.get(i * 2 + 1).copied().unwrap_or([0.; 2]);
                [a[0], a[1], b[0], b[1]]
            }),
        };
        // One bounded reusable allocation per plugin keeps concurrent AE renders from exhausting VRAM.
        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "GPU resource lock poisoned")?;
        if resources
            .as_ref()
            .is_none_or(|r| (0..3).any(|i| r.capacity[i] < sizes[i]))
        {
            *resources = Some(self.buffers(sizes));
        }
        let r = resources.as_ref().unwrap();
        for (image, level) in p.levels.iter().zip(&levels) {
            self.queue.write_buffer(
                &r.input,
                u64::from(level[0]) * 16,
                bytemuck::cast_slice(&image.pixels),
            );
        }
        self.queue
            .write_buffer(&r.field, 0, bytemuck::cast_slice(&p.field.pixels));
        self.queue
            .write_buffer(&r.levels, 0, bytemuck::cast_slice(&levels));
        self.queue
            .write_buffer(&r.uniform, 0, bytemuck::bytes_of(&uniform));
        let scope = self.device.push_error_scope(w::ErrorFilter::Validation);
        let mut encoder = self
            .device
            .create_command_encoder(&w::CommandEncoderDescriptor {
                label: Some("blur dispatch"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&w::ComputePassDescriptor {
                label: Some("blur integration"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &r.bind, &[]);
            pass.dispatch_workgroups(
                (src.width as u32).div_ceil(8),
                (src.height as u32).div_ceil(8),
                1,
            );
        }
        encoder.copy_buffer_to_buffer(&r.output, 0, &r.staging, 0, sizes[2]);
        let (sender, receiver) = std::sync::mpsc::channel();
        encoder.map_buffer_on_submit(&r.staging, w::MapMode::Read, ..sizes[2], move |result| {
            let _ = sender.send(result);
        });
        self.queue.submit([encoder.finish()]);
        let poll = self.device.poll(w::PollType::wait_indefinitely());
        let validation = pollster::block_on(scope.pop());
        if let Some(e) = validation {
            r.staging.unmap();
            return Err(e.to_string());
        }
        poll.map_err(|e| e.to_string())?;
        receiver
            .recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let mapped = r.staging.slice(..sizes[2]).get_mapped_range();
        let result = Image {
            width: src.width,
            height: src.height,
            pixels: bytemuck::cast_slice(&mapped).to_vec(),
        };
        drop(mapped);
        r.staging.unmap();
        Ok(result)
    }
}
pub(super) fn render(p: &Prepared) -> Result<Image, String> {
    match GPU.get_or_init(Context::new) {
        Ok(gpu) => gpu.render(p),
        Err(e) => Err(e.clone()),
    }
}
