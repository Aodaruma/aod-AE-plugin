use after_effects as ae;
use bytemuck::{Pod, Zeroable};
use futures_intrusive::channel::shared::oneshot_channel;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Mutex;
use wgpu::*;

pub struct WgpuRenderParams {
    pub out_w: u32,
    pub out_h: u32,
    pub inv_cell_x: f32,
    pub inv_cell_y: f32,
    pub inv_cell_w: f32,
    pub randomness: f32,
    pub seed: u32,
    pub distance_metric: u32,
    pub lp_exp: f32,
    pub smoothness: f32,
    pub output_type: u32,
    pub feature_mode: u32,
    pub position_local: bool,
    pub origin_x: f32,
    pub origin_y: f32,
    pub w_value: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

pub struct WgpuOutput {
    pub data: Vec<f32>,
}

pub struct WgpuContext {
    device: Device,
    queue: Queue,
    pipeline: ComputePipeline,
    layout: BindGroupLayout,
    state: Mutex<HashMap<std::thread::ThreadId, WgpuResources>>,
}

impl WgpuContext {
    pub fn new() -> Result<Self, ae::Error> {
        let power_preference =
            wgpu::PowerPreference::from_env().unwrap_or(PowerPreference::HighPerformance);
        let mut instance_desc = InstanceDescriptor::default();
        if instance_desc.backends.contains(Backends::DX12)
            && instance_desc.flags.contains(InstanceFlags::VALIDATION)
        {
            instance_desc.backends.remove(Backends::DX12);
        }

        let instance = Instance::new(&instance_desc);
        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference,
            ..Default::default()
        }))
        .map_err(|_| ae::Error::BadCallbackParameter)?;

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: None,
            required_features: adapter.features(),
            required_limits: adapter.limits(),
            experimental_features: ExperimentalFeatures::disabled(),
            memory_hints: MemoryHints::Performance,
            trace: Trace::Off,
        }))
        .map_err(|_| ae::Error::BadCallbackParameter)?;

        let (pipeline, layout) = create_pipeline(&device)?;

        Ok(Self {
            device,
            queue,
            pipeline,
            layout,
            state: Mutex::new(HashMap::new()),
        })
    }

    pub fn render(&self, params: &WgpuRenderParams) -> Result<WgpuOutput, ae::Error> {
        if params.out_w == 0 || params.out_h == 0 {
            return Ok(WgpuOutput { data: vec![] });
        }

        let mut state = self.state.lock().unwrap();
        let thread_id = std::thread::current().id();
        let needs_rebuild = match state.get(&thread_id) {
            Some(res) => res.out_w != params.out_w || res.out_h != params.out_h,
            None => true,
        };
        if needs_rebuild {
            state.insert(
                thread_id,
                WgpuResources::new(&self.device, &self.layout, params)?,
            );
        }
        let res = state
            .get(&thread_id)
            .ok_or(ae::Error::BadCallbackParameter)?;

        let param_buf = Params {
            size: [
                params.out_w,
                params.out_h,
                params.distance_metric,
                params.output_type,
            ],
            seed: [params.seed, params.feature_mode, 0, 0],
            cell: [
                params.inv_cell_x,
                params.inv_cell_y,
                params.randomness,
                params.lp_exp,
            ],
            extra: [
                params.inv_cell_w,
                if params.position_local { 1.0 } else { 0.0 },
                params.origin_x,
                params.origin_y,
            ],
            misc: [
                params.smoothness,
                params.w_value,
                params.offset_x,
                params.offset_y,
            ],
        };
        self.queue
            .write_buffer(&res.params_buf, 0, bytemuck::bytes_of(&param_buf));

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &res.bind_group, &[]);
            pass.dispatch_workgroups(dispatch_dim(params.out_w), dispatch_dim(params.out_h), 1);
        }
        encoder.copy_buffer_to_buffer(&res.out_buf, 0, &res.staging_buf, 0, res.out_bytes);
        self.queue.submit(Some(encoder.finish()));

        let buffer_slice = res.staging_buf.slice(..);
        let (sender, receiver) = oneshot_channel();
        buffer_slice.map_async(MapMode::Read, move |v| sender.send(v).unwrap());
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());

        let mut out = vec![0.0f32; (params.out_w * params.out_h * 4) as usize];
        if let Some(Ok(())) = pollster::block_on(receiver.receive()) {
            let data = buffer_slice.get_mapped_range();
            let src: &[f32] = bytemuck::cast_slice(&data);
            let len = out.len();
            out.copy_from_slice(&src[0..len]);
            drop(data);
            res.staging_buf.unmap();
        } else {
            return Err(ae::Error::BadCallbackParameter);
        }

        Ok(WgpuOutput { data: out })
    }
}

struct WgpuResources {
    out_w: u32,
    out_h: u32,
    out_bytes: u64,
    params_buf: Buffer,
    out_buf: Buffer,
    staging_buf: Buffer,
    bind_group: BindGroup,
}

impl WgpuResources {
    fn new(
        device: &Device,
        layout: &BindGroupLayout,
        params: &WgpuRenderParams,
    ) -> Result<Self, ae::Error> {
        let out_bytes = calc_out_bytes(params.out_w, params.out_h)?;

        let params_buf = device.create_buffer(&BufferDescriptor {
            label: None,
            size: std::mem::size_of::<Params>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let out_buf = device.create_buffer(&BufferDescriptor {
            label: None,
            size: out_bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buf = device.create_buffer(&BufferDescriptor {
            label: None,
            size: out_bytes,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: out_buf.as_entire_binding(),
                },
            ],
        });

        Ok(Self {
            out_w: params.out_w,
            out_h: params.out_h,
            out_bytes,
            params_buf,
            out_buf,
            staging_buf,
            bind_group,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    size: [u32; 4],
    seed: [u32; 4],
    cell: [f32; 4],
    extra: [f32; 4],
    misc: [f32; 4],
}

fn create_pipeline(device: &Device) -> Result<(ComputePipeline, BindGroupLayout), ae::Error> {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("voronoi"),
        source: ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/voronoi.wgsl"))),
    });

    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: BufferSize::new(std::mem::size_of::<Params>() as _),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
        label: None,
    });

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&layout],
        immediate_size: 0,
    });

    let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        module: &shader,
        entry_point: Some("main"),
        label: None,
        layout: Some(&pipeline_layout),
        compilation_options: Default::default(),
        cache: Default::default(),
    });

    Ok((pipeline, layout))
}

fn dispatch_dim(size: u32) -> u32 {
    size.div_ceil(16)
}

fn calc_out_bytes(out_w: u32, out_h: u32) -> Result<u64, ae::Error> {
    let pixels = (out_w as u64)
        .checked_mul(out_h as u64)
        .ok_or(ae::Error::BadCallbackParameter)?;
    let bytes = pixels
        .checked_mul(4)
        .and_then(|v| v.checked_mul(std::mem::size_of::<f32>() as u64))
        .ok_or(ae::Error::BadCallbackParameter)?;
    Ok(bytes)
}
