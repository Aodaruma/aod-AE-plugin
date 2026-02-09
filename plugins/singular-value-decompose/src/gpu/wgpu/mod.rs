use after_effects as ae;
use bytemuck::{Pod, Zeroable};
use futures_intrusive::channel::shared::oneshot_channel;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Mutex;
use wgpu::*;

pub struct WgpuRenderParams {
    pub width: u32,
    pub height: u32,
    pub rank: u32,
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

    pub fn render(
        &self,
        params: &WgpuRenderParams,
        u_data: &[f32],
        v_data: &[f32],
    ) -> Result<WgpuOutput, ae::Error> {
        if params.width == 0 || params.height == 0 || params.rank == 0 {
            return Ok(WgpuOutput { data: vec![] });
        }

        let u_len = calc_u_len(params.width, params.height, params.rank)?;
        let v_len = calc_v_len(params.width, params.rank)?;
        if u_data.len() != u_len || v_data.len() != v_len {
            return Err(ae::Error::BadCallbackParameter);
        }

        let mut state = self.state.lock().unwrap();
        let thread_id = std::thread::current().id();
        let needs_rebuild = match state.get(&thread_id) {
            Some(res) => {
                res.width != params.width || res.height != params.height || res.rank != params.rank
            }
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
            size: [params.width, params.height, params.rank, 0],
        };
        self.queue
            .write_buffer(&res.params_buf, 0, bytemuck::bytes_of(&param_buf));
        self.queue
            .write_buffer(&res.u_buf, 0, bytemuck::cast_slice(u_data));
        self.queue
            .write_buffer(&res.v_buf, 0, bytemuck::cast_slice(v_data));

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
            pass.dispatch_workgroups(dispatch_dim(params.width), dispatch_dim(params.height), 1);
        }
        encoder.copy_buffer_to_buffer(&res.out_buf, 0, &res.staging_buf, 0, res.out_bytes);
        self.queue.submit(Some(encoder.finish()));

        let buffer_slice = res.staging_buf.slice(..);
        let (sender, receiver) = oneshot_channel();
        buffer_slice.map_async(MapMode::Read, move |v| sender.send(v).unwrap());
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());

        let out_len = calc_out_len(params.width, params.height)?;
        let mut out = vec![0.0f32; out_len];
        if let Some(Ok(())) = pollster::block_on(receiver.receive()) {
            let data = buffer_slice.get_mapped_range();
            let src: &[f32] = bytemuck::cast_slice(&data);
            out.copy_from_slice(&src[0..out_len]);
            drop(data);
            res.staging_buf.unmap();
        } else {
            return Err(ae::Error::BadCallbackParameter);
        }

        Ok(WgpuOutput { data: out })
    }
}

struct WgpuResources {
    width: u32,
    height: u32,
    rank: u32,
    out_bytes: u64,
    params_buf: Buffer,
    u_buf: Buffer,
    v_buf: Buffer,
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
        let out_bytes = calc_out_bytes(params.width, params.height)?;
        let u_bytes = calc_u_bytes(params.width, params.height, params.rank)?;
        let v_bytes = calc_v_bytes(params.width, params.rank)?;

        let params_buf = device.create_buffer(&BufferDescriptor {
            label: None,
            size: std::mem::size_of::<Params>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let u_buf = device.create_buffer(&BufferDescriptor {
            label: None,
            size: u_bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let v_buf = device.create_buffer(&BufferDescriptor {
            label: None,
            size: v_bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
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
                    resource: u_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: v_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: out_buf.as_entire_binding(),
                },
            ],
        });

        Ok(Self {
            width: params.width,
            height: params.height,
            rank: params.rank,
            out_bytes,
            params_buf,
            u_buf,
            v_buf,
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
}

fn create_pipeline(device: &Device) -> Result<(ComputePipeline, BindGroupLayout), ae::Error> {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("svd_reconstruct"),
        source: ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/svd_reconstruct.wgsl"))),
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
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 3,
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

fn calc_out_len(width: u32, height: u32) -> Result<usize, ae::Error> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|v| v.checked_mul(4))
        .ok_or(ae::Error::BadCallbackParameter)
}

fn calc_out_bytes(width: u32, height: u32) -> Result<u64, ae::Error> {
    let len = calc_out_len(width, height)? as u64;
    len.checked_mul(std::mem::size_of::<f32>() as u64)
        .ok_or(ae::Error::BadCallbackParameter)
}

fn calc_u_len(width: u32, height: u32, rank: u32) -> Result<usize, ae::Error> {
    let _ = width;
    (4usize)
        .checked_mul(height as usize)
        .and_then(|v| v.checked_mul(rank as usize))
        .ok_or(ae::Error::BadCallbackParameter)
}

fn calc_u_bytes(width: u32, height: u32, rank: u32) -> Result<u64, ae::Error> {
    let len = calc_u_len(width, height, rank)? as u64;
    len.checked_mul(std::mem::size_of::<f32>() as u64)
        .ok_or(ae::Error::BadCallbackParameter)
}

fn calc_v_len(width: u32, rank: u32) -> Result<usize, ae::Error> {
    (4usize)
        .checked_mul(rank as usize)
        .and_then(|v| v.checked_mul(width as usize))
        .ok_or(ae::Error::BadCallbackParameter)
}

fn calc_v_bytes(width: u32, rank: u32) -> Result<u64, ae::Error> {
    let len = calc_v_len(width, rank)? as u64;
    len.checked_mul(std::mem::size_of::<f32>() as u64)
        .ok_or(ae::Error::BadCallbackParameter)
}
