use after_effects as ae;
use bytemuck::{Pod, Zeroable};
use futures_intrusive::channel::shared::oneshot_channel;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Mutex;
use wgpu::*;

pub const MAX_CLUSTERS: u32 = 64;

const PIXELS_WORKGROUP: u32 = 256;
const CLUSTERS_WORKGROUP: u32 = 64;
const CONVERGENCE_CHECK_INTERVAL: u32 = 4;

pub struct WgpuRenderParams {
    pub out_w: u32,
    pub out_h: u32,
    pub cluster_count: u32,
    pub max_iterations: u32,
    pub color_space: u32,
    pub rgb_only: bool,
    pub sum_scale: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WgpuRenderStats {
    pub iterations_executed: u32,
    pub converged: bool,
}

pub struct WgpuOutput {
    pub data: Vec<f32>,
    pub stats: WgpuRenderStats,
}

pub struct WgpuContext {
    device: Device,
    queue: Queue,
    clear_labels_pipeline: ComputePipeline,
    assign_pipeline: ComputePipeline,
    update_pipeline: ComputePipeline,
    output_pipeline: ComputePipeline,
    layout: BindGroupLayout,
    state: Mutex<HashMap<std::thread::ThreadId, WgpuResources>>,
}

impl WgpuContext {
    pub fn new() -> Result<Self, String> {
        let power_preference =
            wgpu::PowerPreference::from_env().unwrap_or(PowerPreference::HighPerformance);
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
        .map_err(|err| {
            format!(
                "request_adapter failed: {:?} (backends={:?}, flags={:?})",
                err, instance_desc.backends, instance_desc.flags
            )
        })?;

        let required_features = Features::empty();
        let required_limits = Limits::default()
            .using_resolution(adapter.limits())
            .using_alignment(adapter.limits());

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: None,
            required_features,
            required_limits,
            experimental_features: ExperimentalFeatures::disabled(),
            memory_hints: MemoryHints::Performance,
            trace: Trace::Off,
        }))
        .map_err(|err| format!("request_device failed: {:?}", err))?;

        let pipelines = create_pipelines(&device)
            .map_err(|err| format!("create_pipelines failed: {:?}", err))?;

        Ok(Self {
            device,
            queue,
            clear_labels_pipeline: pipelines.clear_labels_pipeline,
            assign_pipeline: pipelines.assign_pipeline,
            update_pipeline: pipelines.update_pipeline,
            output_pipeline: pipelines.output_pipeline,
            layout: pipelines.layout,
            state: Mutex::new(HashMap::new()),
        })
    }

    pub fn render(
        &self,
        params: &WgpuRenderParams,
        input: &[f32],
        init_centroids: &[[f32; 4]],
    ) -> Result<WgpuOutput, ae::Error> {
        if params.out_w == 0 || params.out_h == 0 || input.is_empty() {
            return Ok(WgpuOutput {
                data: vec![],
                stats: WgpuRenderStats::default(),
            });
        }
        if params.cluster_count == 0 || params.cluster_count > MAX_CLUSTERS {
            return Err(ae::Error::BadCallbackParameter);
        }

        let pixel_count = (params.out_w as usize)
            .checked_mul(params.out_h as usize)
            .ok_or(ae::Error::BadCallbackParameter)?;
        if input.len() != pixel_count * 4 {
            return Err(ae::Error::BadCallbackParameter);
        }

        let cluster_count = params.cluster_count as usize;
        if init_centroids.len() < cluster_count {
            return Err(ae::Error::BadCallbackParameter);
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
                WgpuResources::new(&self.device, &self.layout, params.out_w, params.out_h)?,
            );
        }
        let res = state
            .get(&thread_id)
            .ok_or(ae::Error::BadCallbackParameter)?;

        let mut centroid_data = vec![0.0f32; (MAX_CLUSTERS as usize) * 4];
        for (idx, centroid) in init_centroids.iter().take(cluster_count).enumerate() {
            let base = idx * 4;
            centroid_data[base] = centroid[0];
            centroid_data[base + 1] = centroid[1];
            centroid_data[base + 2] = centroid[2];
            centroid_data[base + 3] = centroid[3];
        }

        let param_buf = Params {
            size: [
                params.out_w,
                params.out_h,
                params.out_w.saturating_mul(params.out_h),
                params.cluster_count,
            ],
            mode: [params.color_space, u32::from(params.rgb_only), 0, 0],
            scale: [params.sum_scale.max(1.0), 0.0, 0.0, 0.0],
        };

        self.queue
            .write_buffer(&res.params_buf, 0, bytemuck::bytes_of(&param_buf));
        self.queue
            .write_buffer(&res.input_buf, 0, bytemuck::cast_slice(input));
        self.queue
            .write_buffer(&res.centroids_buf, 0, bytemuck::cast_slice(&centroid_data));

        let dispatch_pixels = dispatch_dim(pixel_count as u32, PIXELS_WORKGROUP);
        let dispatch_clusters = dispatch_dim(params.cluster_count, CLUSTERS_WORKGROUP);

        let max_iterations = params.max_iterations.max(1);
        let mut iterations_executed = 0u32;
        let mut converged = false;
        let mut did_clear_labels = false;

        while iterations_executed < max_iterations {
            let batch = (max_iterations - iterations_executed).min(CONVERGENCE_CHECK_INTERVAL);
            let mut encoder = self
                .device
                .create_command_encoder(&CommandEncoderDescriptor { label: None });

            if !did_clear_labels {
                {
                    let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                        label: Some("clear_labels"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.clear_labels_pipeline);
                    pass.set_bind_group(0, &res.bind_group, &[]);
                    pass.dispatch_workgroups(dispatch_pixels, 1, 1);
                }
                did_clear_labels = true;
            }

            for _ in 0..batch {
                encoder.clear_buffer(&res.sums_buf, 0, None);
                encoder.clear_buffer(&res.counts_buf, 0, None);
                encoder.clear_buffer(&res.changes_buf, 0, None);

                {
                    let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                        label: Some("assign_accumulate"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.assign_pipeline);
                    pass.set_bind_group(0, &res.bind_group, &[]);
                    pass.dispatch_workgroups(dispatch_pixels, 1, 1);
                }

                {
                    let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                        label: Some("update_centroids"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.update_pipeline);
                    pass.set_bind_group(0, &res.bind_group, &[]);
                    pass.dispatch_workgroups(dispatch_clusters, 1, 1);
                }

                iterations_executed = iterations_executed.saturating_add(1);
            }

            encoder.copy_buffer_to_buffer(
                &res.changes_buf,
                0,
                &res.staging_changes_buf,
                0,
                std::mem::size_of::<u32>() as u64,
            );

            let (sender, receiver) = oneshot_channel();
            encoder.map_buffer_on_submit(&res.staging_changes_buf, MapMode::Read, ..4, move |r| {
                let _ = sender.send(r);
            });

            self.queue.submit(Some(encoder.finish()));
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());

            let changed = if let Some(Ok(())) = pollster::block_on(receiver.receive()) {
                let mapped = res.staging_changes_buf.slice(..4).get_mapped_range();
                let src: &[u32] = bytemuck::cast_slice(&mapped);
                let value = *src.first().unwrap_or(&1u32);
                drop(mapped);
                res.staging_changes_buf.unmap();
                value
            } else {
                return Err(ae::Error::BadCallbackParameter);
            };

            if changed == 0 {
                converged = true;
                break;
            }
        }

        let mut output_encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });
        {
            let mut pass = output_encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("write_output"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.output_pipeline);
            pass.set_bind_group(0, &res.bind_group, &[]);
            pass.dispatch_workgroups(dispatch_pixels, 1, 1);
        }
        output_encoder.copy_buffer_to_buffer(&res.out_buf, 0, &res.staging_buf, 0, res.out_bytes);

        let (sender, receiver) = oneshot_channel();
        output_encoder.map_buffer_on_submit(&res.staging_buf, MapMode::Read, .., move |r| {
            let _ = sender.send(r);
        });

        self.queue.submit(Some(output_encoder.finish()));
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());

        let mut out = vec![0.0f32; pixel_count * 4];
        if let Some(Ok(())) = pollster::block_on(receiver.receive()) {
            let mapped = res.staging_buf.slice(..).get_mapped_range();
            let src: &[f32] = bytemuck::cast_slice(&mapped);
            let out_len = out.len();
            out.copy_from_slice(&src[..out_len]);
            drop(mapped);
            res.staging_buf.unmap();
        } else {
            return Err(ae::Error::BadCallbackParameter);
        }

        Ok(WgpuOutput {
            data: out,
            stats: WgpuRenderStats {
                iterations_executed,
                converged,
            },
        })
    }
}

struct PipelineBundle {
    clear_labels_pipeline: ComputePipeline,
    assign_pipeline: ComputePipeline,
    update_pipeline: ComputePipeline,
    output_pipeline: ComputePipeline,
    layout: BindGroupLayout,
}

struct WgpuResources {
    out_w: u32,
    out_h: u32,
    out_bytes: u64,
    params_buf: Buffer,
    input_buf: Buffer,
    centroids_buf: Buffer,
    sums_buf: Buffer,
    counts_buf: Buffer,
    _labels_buf: Buffer,
    changes_buf: Buffer,
    out_buf: Buffer,
    staging_buf: Buffer,
    staging_changes_buf: Buffer,
    bind_group: BindGroup,
}

impl WgpuResources {
    fn new(
        device: &Device,
        layout: &BindGroupLayout,
        out_w: u32,
        out_h: u32,
    ) -> Result<Self, ae::Error> {
        let out_bytes = calc_pixel_bytes(out_w, out_h)?;
        let labels_bytes = calc_label_bytes(out_w, out_h)?;
        let centroids_bytes = (MAX_CLUSTERS as u64)
            .checked_mul(4)
            .and_then(|v| v.checked_mul(std::mem::size_of::<f32>() as u64))
            .ok_or(ae::Error::BadCallbackParameter)?;
        let sums_bytes = (MAX_CLUSTERS as u64)
            .checked_mul(4)
            .and_then(|v| v.checked_mul(std::mem::size_of::<u32>() as u64))
            .ok_or(ae::Error::BadCallbackParameter)?;
        let counts_bytes = (MAX_CLUSTERS as u64)
            .checked_mul(std::mem::size_of::<u32>() as u64)
            .ok_or(ae::Error::BadCallbackParameter)?;

        let params_buf = device.create_buffer(&BufferDescriptor {
            label: Some("params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let input_buf = device.create_buffer(&BufferDescriptor {
            label: Some("input"),
            size: out_bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let centroids_buf = device.create_buffer(&BufferDescriptor {
            label: Some("centroids"),
            size: centroids_bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sums_buf = device.create_buffer(&BufferDescriptor {
            label: Some("sums"),
            size: sums_bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let counts_buf = device.create_buffer(&BufferDescriptor {
            label: Some("counts"),
            size: counts_bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let labels_buf = device.create_buffer(&BufferDescriptor {
            label: Some("labels"),
            size: labels_bytes,
            usage: BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let changes_buf = device.create_buffer(&BufferDescriptor {
            label: Some("changes"),
            size: std::mem::size_of::<u32>() as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let out_buf = device.create_buffer(&BufferDescriptor {
            label: Some("output"),
            size: out_bytes,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buf = device.create_buffer(&BufferDescriptor {
            label: Some("staging"),
            size: out_bytes,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let staging_changes_buf = device.create_buffer(&BufferDescriptor {
            label: Some("staging_changes"),
            size: std::mem::size_of::<u32>() as u64,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("color_quantize_bind_group"),
            layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: input_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: centroids_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: sums_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: counts_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: labels_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 6,
                    resource: changes_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 7,
                    resource: out_buf.as_entire_binding(),
                },
            ],
        });

        Ok(Self {
            out_w,
            out_h,
            out_bytes,
            params_buf,
            input_buf,
            centroids_buf,
            sums_buf,
            counts_buf,
            _labels_buf: labels_buf,
            changes_buf,
            out_buf,
            staging_buf,
            staging_changes_buf,
            bind_group,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    size: [u32; 4],
    mode: [u32; 4],
    scale: [f32; 4],
}

fn create_pipelines(device: &Device) -> Result<PipelineBundle, ae::Error> {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("color_quantize_compute"),
        source: ShaderSource::Wgsl(Cow::Borrowed(include_str!("shaders/compute.wgsl"))),
    });

    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("color_quantize_bind_layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: BufferSize::new(std::mem::size_of::<Params>() as u64),
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
                    ty: BufferBindingType::Storage { read_only: false },
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
            BindGroupLayoutEntry {
                binding: 5,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 6,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("color_quantize_pipeline_layout"),
        bind_group_layouts: &[&layout],
        immediate_size: 0,
    });

    let clear_labels_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("clear_labels_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("clear_labels"),
        compilation_options: Default::default(),
        cache: Default::default(),
    });

    let assign_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("assign_accumulate_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("assign_accumulate"),
        compilation_options: Default::default(),
        cache: Default::default(),
    });

    let update_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("update_centroids_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("update_centroids"),
        compilation_options: Default::default(),
        cache: Default::default(),
    });

    let output_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("write_output_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("write_output"),
        compilation_options: Default::default(),
        cache: Default::default(),
    });

    Ok(PipelineBundle {
        clear_labels_pipeline,
        assign_pipeline,
        update_pipeline,
        output_pipeline,
        layout,
    })
}

#[inline]
fn dispatch_dim(size: u32, workgroup_size: u32) -> u32 {
    size.div_ceil(workgroup_size)
}

fn calc_pixel_bytes(out_w: u32, out_h: u32) -> Result<u64, ae::Error> {
    let pixels = (out_w as u64)
        .checked_mul(out_h as u64)
        .ok_or(ae::Error::BadCallbackParameter)?;
    let bytes = pixels
        .checked_mul(4)
        .and_then(|v| v.checked_mul(std::mem::size_of::<f32>() as u64))
        .ok_or(ae::Error::BadCallbackParameter)?;
    Ok(bytes)
}

fn calc_label_bytes(out_w: u32, out_h: u32) -> Result<u64, ae::Error> {
    let pixels = (out_w as u64)
        .checked_mul(out_h as u64)
        .ok_or(ae::Error::BadCallbackParameter)?;
    pixels
        .checked_mul(std::mem::size_of::<u32>() as u64)
        .ok_or(ae::Error::BadCallbackParameter)
}
