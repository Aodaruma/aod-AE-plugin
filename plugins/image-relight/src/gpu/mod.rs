use bytemuck::{Pod, Zeroable};
use futures_intrusive::channel::shared::oneshot_channel;
use std::borrow::Cow;
use std::sync::Mutex;
use wgpu::*;

const WORKGROUP_WIDTH: u32 = 16;
const WORKGROUP_HEIGHT: u32 = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GpuBoundaryCondition {
    Dirichlet,
    Neumann,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SolveParams {
    pub width: usize,
    pub height: usize,
    pub condition: GpuBoundaryCondition,
    pub iterations: usize,
    pub lambda_squared: f32,
    pub weight_x: f32,
    pub weight_y: f32,
    pub omega: f32,
}

pub(crate) struct WgpuContext {
    device: Device,
    queue: Queue,
    limits: Limits,
    red_pipeline: ComputePipeline,
    black_pipeline: ComputePipeline,
    layout: BindGroupLayout,
    // One reusable allocation keeps repeated frame sizes cheap without retaining VRAM for every
    // render thread or every size AE has requested in the past.
    resources: Mutex<Option<WgpuResources>>,
}

impl WgpuContext {
    pub(crate) fn new() -> Result<Self, String> {
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
        validate_compute_limits(&required_limits)?;

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: Some("AOD_ImageRelight Poisson device"),
            required_features: Features::empty(),
            required_limits: required_limits.clone(),
            experimental_features: ExperimentalFeatures::disabled(),
            memory_hints: MemoryHints::Performance,
            trace: Trace::Off,
        }))
        .map_err(|error| format!("request_device failed: {error:?}"))?;

        let (layout, red_pipeline, black_pipeline) = create_pipelines(&device);
        Ok(Self {
            device,
            queue,
            limits: required_limits,
            red_pipeline,
            black_pipeline,
            layout,
            resources: Mutex::new(None),
        })
    }

    pub(crate) fn solve(
        &self,
        labels: &[u32],
        boundary: &[bool],
        rhs: &[f32],
        params: SolveParams,
    ) -> Result<Vec<f32>, String> {
        let pixel_count = params
            .width
            .checked_mul(params.height)
            .ok_or_else(|| "image dimensions overflow".to_owned())?;
        if labels.len() != pixel_count || boundary.len() != pixel_count || rhs.len() != pixel_count
        {
            return Err("Poisson input buffers do not match image dimensions".to_owned());
        }
        if pixel_count == 0 {
            return Ok(Vec::new());
        }
        validate_solver_params(params, rhs)?;

        let width = u32::try_from(params.width)
            .map_err(|_| "image width exceeds the GPU u32 coordinate range".to_owned())?;
        let height = u32::try_from(params.height)
            .map_err(|_| "image height exceeds the GPU u32 coordinate range".to_owned())?;
        let output_bytes = buffer_bytes::<f32>(pixel_count)?;
        self.validate_resource_sizes(output_bytes, width, height)?;

        // WGSL storage buffers have no portable bool element representation.
        let boundary_words = boundary
            .iter()
            .map(|&value| u32::from(value))
            .collect::<Vec<_>>();
        let gpu_params = GpuParams {
            image: [
                width,
                height,
                match params.condition {
                    GpuBoundaryCondition::Dirichlet => 0,
                    GpuBoundaryCondition::Neumann => 1,
                },
                0,
            ],
            coefficients: [
                params.lambda_squared,
                params.weight_x,
                params.weight_y,
                params.omega,
            ],
        };

        let mut resources = self
            .resources
            .lock()
            .map_err(|_| "GPU Poisson resource lock was poisoned".to_owned())?;
        let dimensions_changed = resources
            .as_ref()
            .is_none_or(|resource| resource.width != width || resource.height != height);
        if dimensions_changed {
            *resources = Some(WgpuResources::new(
                &self.device,
                &self.layout,
                width,
                height,
                output_bytes,
            ));
        }
        let resource = resources
            .as_mut()
            .ok_or_else(|| "GPU Poisson resources were not initialized".to_owned())?;

        self.queue
            .write_buffer(&resource.params, 0, bytemuck::bytes_of(&gpu_params));
        self.queue
            .write_buffer(&resource.labels, 0, bytemuck::cast_slice(labels));
        self.queue.write_buffer(
            &resource.boundary,
            0,
            bytemuck::cast_slice(boundary_words.as_slice()),
        );
        self.queue
            .write_buffer(&resource.rhs, 0, bytemuck::cast_slice(rhs));

        let dispatch_x = width.div_ceil(WORKGROUP_WIDTH);
        let dispatch_y = height.div_ceil(WORKGROUP_HEIGHT);
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("AOD_ImageRelight Poisson encoder"),
            });
        encoder.clear_buffer(&resource.heights, 0, Some(output_bytes));
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("AOD_ImageRelight red-black SOR"),
                timestamp_writes: None,
            });
            pass.set_bind_group(0, &resource.bind_group, &[]);
            for _ in 0..params.iterations.max(1) {
                pass.set_pipeline(&self.red_pipeline);
                pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
                pass.set_pipeline(&self.black_pipeline);
                pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
            }
        }
        encoder.copy_buffer_to_buffer(&resource.heights, 0, &resource.staging, 0, output_bytes);

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
            return Err("GPU Poisson output readback failed".to_owned());
        };
        let mapped = resource.staging.slice(..output_bytes).get_mapped_range();
        let mapped_heights: &[f32] = bytemuck::cast_slice(&mapped);
        if mapped_heights.len() < pixel_count {
            drop(mapped);
            resource.staging.unmap();
            return Err("GPU Poisson output was shorter than expected".to_owned());
        }
        let output = mapped_heights[..pixel_count].to_vec();
        drop(mapped);
        resource.staging.unmap();
        Ok(output)
    }

    fn validate_resource_sizes(
        &self,
        buffer_size: u64,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let storage_limit = self.limits.max_storage_buffer_binding_size as u64;
        if buffer_size > storage_limit || buffer_size > self.limits.max_buffer_size {
            return Err(format!(
                "each Poisson image buffer requires {buffer_size} bytes, exceeding GPU limits"
            ));
        }

        let dispatch_limit = self.limits.max_compute_workgroups_per_dimension;
        if width.div_ceil(WORKGROUP_WIDTH) > dispatch_limit
            || height.div_ceil(WORKGROUP_HEIGHT) > dispatch_limit
        {
            return Err("image dimensions exceed GPU dispatch limits".to_owned());
        }
        Ok(())
    }
}

fn validate_compute_limits(limits: &Limits) -> Result<(), String> {
    let invocations = WORKGROUP_WIDTH.saturating_mul(WORKGROUP_HEIGHT);
    if WORKGROUP_WIDTH > limits.max_compute_workgroup_size_x
        || WORKGROUP_HEIGHT > limits.max_compute_workgroup_size_y
        || invocations > limits.max_compute_invocations_per_workgroup
    {
        return Err("GPU does not support the required 16x16 compute workgroup".to_owned());
    }
    if limits.max_storage_buffers_per_shader_stage < 4 {
        return Err("GPU exposes fewer than four compute storage buffers".to_owned());
    }
    Ok(())
}

fn validate_solver_params(params: SolveParams, rhs: &[f32]) -> Result<(), String> {
    if !params.lambda_squared.is_finite() || params.lambda_squared < 0.0 {
        return Err("lambda_squared must be finite and non-negative".to_owned());
    }
    if !params.weight_x.is_finite() || params.weight_x <= 0.0 {
        return Err("weight_x must be finite and positive".to_owned());
    }
    if !params.weight_y.is_finite() || params.weight_y <= 0.0 {
        return Err("weight_y must be finite and positive".to_owned());
    }
    if !params.omega.is_finite() || !(0.0..=2.0).contains(&params.omega) || params.omega == 0.0 {
        return Err("omega must be finite and in the interval (0, 2]".to_owned());
    }
    if rhs.iter().any(|value| !value.is_finite()) {
        return Err("Poisson right-hand side contains a non-finite value".to_owned());
    }
    Ok(())
}

#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct GpuParams {
    image: [u32; 4],
    coefficients: [f32; 4],
}

struct WgpuResources {
    width: u32,
    height: u32,
    params: Buffer,
    labels: Buffer,
    boundary: Buffer,
    rhs: Buffer,
    heights: Buffer,
    staging: Buffer,
    bind_group: BindGroup,
}

impl WgpuResources {
    fn new(
        device: &Device,
        layout: &BindGroupLayout,
        width: u32,
        height: u32,
        buffer_size: u64,
    ) -> Self {
        let params = create_buffer(
            device,
            "AOD_ImageRelight Poisson params",
            std::mem::size_of::<GpuParams>() as u64,
            BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        );
        let labels = create_buffer(
            device,
            "AOD_ImageRelight Poisson labels",
            buffer_size,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let boundary = create_buffer(
            device,
            "AOD_ImageRelight Poisson boundary",
            buffer_size,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let rhs = create_buffer(
            device,
            "AOD_ImageRelight Poisson RHS",
            buffer_size,
            BufferUsages::STORAGE | BufferUsages::COPY_DST,
        );
        let heights = create_buffer(
            device,
            "AOD_ImageRelight Poisson heights",
            buffer_size,
            BufferUsages::STORAGE | BufferUsages::COPY_DST | BufferUsages::COPY_SRC,
        );
        let staging = create_buffer(
            device,
            "AOD_ImageRelight Poisson staging",
            buffer_size,
            BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        );
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("AOD_ImageRelight Poisson bind group"),
            layout,
            entries: &[
                bind_entry(0, &params),
                bind_entry(1, &labels),
                bind_entry(2, &boundary),
                bind_entry(3, &rhs),
                bind_entry(4, &heights),
            ],
        });
        Self {
            width,
            height,
            params,
            labels,
            boundary,
            rhs,
            heights,
            staging,
            bind_group,
        }
    }
}

fn create_pipelines(device: &Device) -> (BindGroupLayout, ComputePipeline, ComputePipeline) {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("AOD_ImageRelight Poisson shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(include_str!("poisson.wgsl"))),
    });
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("AOD_ImageRelight Poisson bind group layout"),
        entries: &[
            buffer_layout_entry(
                0,
                BufferBindingType::Uniform,
                std::mem::size_of::<GpuParams>(),
            ),
            buffer_layout_entry(1, BufferBindingType::Storage { read_only: true }, 0),
            buffer_layout_entry(2, BufferBindingType::Storage { read_only: true }, 0),
            buffer_layout_entry(3, BufferBindingType::Storage { read_only: true }, 0),
            buffer_layout_entry(4, BufferBindingType::Storage { read_only: false }, 0),
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("AOD_ImageRelight Poisson pipeline layout"),
        bind_group_layouts: &[&layout],
        immediate_size: 0,
    });
    let create = |label, entry_point| {
        device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some(entry_point),
            compilation_options: Default::default(),
            cache: Default::default(),
        })
    };
    let red = create("AOD_ImageRelight Poisson red pipeline", "red");
    let black = create("AOD_ImageRelight Poisson black pipeline", "black");
    (layout, red, black)
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

fn buffer_bytes<T>(count: usize) -> Result<u64, String> {
    let count = u64::try_from(count).map_err(|_| "buffer element count exceeds u64".to_owned())?;
    count
        .checked_mul(std::mem::size_of::<T>() as u64)
        .ok_or_else(|| "GPU buffer byte count overflow".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_params() -> SolveParams {
        SolveParams {
            width: 2,
            height: 2,
            condition: GpuBoundaryCondition::Neumann,
            iterations: 20,
            lambda_squared: 0.0,
            weight_x: 1.0,
            weight_y: 1.0,
            omega: 1.5,
        }
    }

    #[test]
    fn rejects_non_finite_rhs_before_gpu_submission() {
        assert!(validate_solver_params(valid_params(), &[f32::NAN]).is_err());
    }

    #[test]
    fn rejects_unstable_relaxation_weight() {
        let mut params = valid_params();
        params.omega = 2.1;
        assert!(validate_solver_params(params, &[0.0]).is_err());
    }

    #[test]
    fn gpu_dirichlet_solve_keeps_the_boundary_fixed() {
        let Ok(context) = WgpuContext::new() else {
            eprintln!("Skipping ImageRelight GPU test because wgpu is unavailable");
            return;
        };
        let labels = vec![7; 25];
        let boundary = (0..25)
            .map(|index| {
                let x = index % 5;
                let y = index / 5;
                x == 0 || x == 4 || y == 0 || y == 4
            })
            .collect::<Vec<_>>();
        let mut rhs = vec![0.0; 25];
        rhs[12] = 1.0;
        let output = context
            .solve(
                &labels,
                &boundary,
                &rhs,
                SolveParams {
                    width: 5,
                    height: 5,
                    condition: GpuBoundaryCondition::Dirichlet,
                    iterations: 24,
                    lambda_squared: 0.0,
                    weight_x: 1.0,
                    weight_y: 1.0,
                    omega: 1.5,
                },
            )
            .expect("GPU Poisson solve should succeed");
        for (index, value) in output.iter().enumerate() {
            if boundary[index] {
                assert_eq!(*value, 0.0);
            }
        }
        // The source is red parity. Positive black neighbors and red diagonals prove that
        // dependent red/black dispatches are ordered within the single submitted command buffer.
        let center = output[12];
        assert!(center > 0.0);
        for index in [7, 11, 13, 17] {
            assert!(output[index] > 0.0 && output[index] < center);
        }
        for index in [6, 8, 16, 18] {
            assert!(output[index] > 0.0 && output[index] < center);
        }
    }
}
