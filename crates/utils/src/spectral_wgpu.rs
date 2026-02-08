use after_effects as ae;
use bytemuck::{Pod, Zeroable};
use futures_intrusive::channel::shared::oneshot_channel;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Mutex;
use wgpu::*;

pub struct SpectralOutput {
    pub real: Vec<f32>,
    pub imag: Vec<f32>,
}

pub struct SpectralWgpuContext {
    device: Device,
    queue: Queue,
    pipeline: ComputePipeline,
    layout: BindGroupLayout,
    state: Mutex<HashMap<std::thread::ThreadId, SpectralResources>>,
}

impl SpectralWgpuContext {
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

    pub fn forward_rgba(
        &self,
        width: u32,
        height: u32,
        input_centered_rgba: &[f32],
    ) -> Result<SpectralOutput, ae::Error> {
        self.run(width, height, false, input_centered_rgba, None)
    }

    pub fn inverse_rgba(
        &self,
        width: u32,
        height: u32,
        input_real_rgba: &[f32],
        input_imag_rgba: &[f32],
    ) -> Result<Vec<f32>, ae::Error> {
        let output = self.run(width, height, true, input_real_rgba, Some(input_imag_rgba))?;
        Ok(output.real)
    }

    fn run(
        &self,
        width: u32,
        height: u32,
        inverse: bool,
        input_real_rgba: &[f32],
        input_imag_rgba: Option<&[f32]>,
    ) -> Result<SpectralOutput, ae::Error> {
        if width == 0 || height == 0 {
            return Ok(SpectralOutput {
                real: vec![],
                imag: vec![],
            });
        }

        let expected_len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|v| v.checked_mul(4))
            .ok_or(ae::Error::BadCallbackParameter)?;
        if input_real_rgba.len() != expected_len {
            return Err(ae::Error::BadCallbackParameter);
        }

        let zeros = vec![0.0f32; expected_len];
        let input_imag_rgba = input_imag_rgba.unwrap_or(&zeros);
        if input_imag_rgba.len() != expected_len {
            return Err(ae::Error::BadCallbackParameter);
        }

        let mut state = self.state.lock().unwrap();
        let thread_id = std::thread::current().id();
        let needs_rebuild = match state.get(&thread_id) {
            Some(res) => res.width != width || res.height != height,
            None => true,
        };
        if needs_rebuild {
            state.insert(
                thread_id,
                SpectralResources::new(&self.device, &self.layout, width, height)?,
            );
        }
        let res = state
            .get(&thread_id)
            .ok_or(ae::Error::BadCallbackParameter)?;

        let param_buf = Params {
            size: [width, height, 0, 0],
            mode: [u32::from(inverse), 0, 0, 0],
        };
        self.queue
            .write_buffer(&res.params_buf, 0, bytemuck::bytes_of(&param_buf));
        self.queue
            .write_buffer(&res.in_real_buf, 0, bytemuck::cast_slice(input_real_rgba));
        self.queue
            .write_buffer(&res.in_imag_buf, 0, bytemuck::cast_slice(input_imag_rgba));

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
            pass.dispatch_workgroups(dispatch_dim(width), dispatch_dim(height), 1);
        }
        encoder.copy_buffer_to_buffer(&res.out_real_buf, 0, &res.staging_real_buf, 0, res.bytes);
        encoder.copy_buffer_to_buffer(&res.out_imag_buf, 0, &res.staging_imag_buf, 0, res.bytes);
        self.queue.submit(Some(encoder.finish()));

        let real = map_f32_buffer(&self.device, &res.staging_real_buf, expected_len)?;
        let imag = map_f32_buffer(&self.device, &res.staging_imag_buf, expected_len)?;

        Ok(SpectralOutput { real, imag })
    }
}

struct SpectralResources {
    width: u32,
    height: u32,
    bytes: u64,
    params_buf: Buffer,
    in_real_buf: Buffer,
    in_imag_buf: Buffer,
    out_real_buf: Buffer,
    out_imag_buf: Buffer,
    staging_real_buf: Buffer,
    staging_imag_buf: Buffer,
    bind_group: BindGroup,
}

impl SpectralResources {
    fn new(
        device: &Device,
        layout: &BindGroupLayout,
        width: u32,
        height: u32,
    ) -> Result<Self, ae::Error> {
        let bytes = calc_rgba_f32_bytes(width, height)?;

        let params_buf = device.create_buffer(&BufferDescriptor {
            label: None,
            size: std::mem::size_of::<Params>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let in_real_buf = device.create_buffer(&BufferDescriptor {
            label: None,
            size: bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let in_imag_buf = device.create_buffer(&BufferDescriptor {
            label: None,
            size: bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let out_real_buf = device.create_buffer(&BufferDescriptor {
            label: None,
            size: bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let out_imag_buf = device.create_buffer(&BufferDescriptor {
            label: None,
            size: bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_real_buf = device.create_buffer(&BufferDescriptor {
            label: None,
            size: bytes,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let staging_imag_buf = device.create_buffer(&BufferDescriptor {
            label: None,
            size: bytes,
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
                    resource: in_real_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: in_imag_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: out_real_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: out_imag_buf.as_entire_binding(),
                },
            ],
        });

        Ok(Self {
            width,
            height,
            bytes,
            params_buf,
            in_real_buf,
            in_imag_buf,
            out_real_buf,
            out_imag_buf,
            staging_real_buf,
            staging_imag_buf,
            bind_group,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    size: [u32; 4],
    mode: [u32; 4],
}

fn create_pipeline(device: &Device) -> Result<(ComputePipeline, BindGroupLayout), ae::Error> {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("spectral_dft"),
        source: ShaderSource::Wgsl(Cow::Borrowed(include_str!(
            "spectral_wgpu/shaders/spectral_dft.wgsl"
        ))),
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
            BindGroupLayoutEntry {
                binding: 4,
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

fn map_f32_buffer(device: &Device, buffer: &Buffer, len: usize) -> Result<Vec<f32>, ae::Error> {
    let slice = buffer.slice(..);
    let (sender, receiver) = oneshot_channel();
    slice.map_async(MapMode::Read, move |v| sender.send(v).unwrap());
    let _ = device.poll(wgpu::PollType::wait_indefinitely());

    if let Some(Ok(())) = pollster::block_on(receiver.receive()) {
        let data = slice.get_mapped_range();
        let src: &[f32] = bytemuck::cast_slice(&data);
        let mut out = vec![0.0f32; len];
        out.copy_from_slice(&src[0..len]);
        drop(data);
        buffer.unmap();
        Ok(out)
    } else {
        Err(ae::Error::BadCallbackParameter)
    }
}

fn dispatch_dim(size: u32) -> u32 {
    size.div_ceil(8)
}

fn calc_rgba_f32_bytes(out_w: u32, out_h: u32) -> Result<u64, ae::Error> {
    let pixels = (out_w as u64)
        .checked_mul(out_h as u64)
        .ok_or(ae::Error::BadCallbackParameter)?;
    let bytes = pixels
        .checked_mul(4)
        .and_then(|v| v.checked_mul(std::mem::size_of::<f32>() as u64))
        .ok_or(ae::Error::BadCallbackParameter)?;
    Ok(bytes)
}
