//! GPU-accelerated starfield renderer using wgpu compute shaders.

use bytemuck::{Pod, Zeroable};
use log::{info, warn};
use wgpu::util::DeviceExt;

/// Uniform parameters for the starfield compute shader.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct StarfieldParams {
    pub width: u32,
    pub height: u32,
    pub num_stars: u32,
    pub _pad0: u32,
    pub rms: f32,
    pub low_energy: f32,
    pub mid_energy: f32,
    pub high_energy: f32,
    pub fg_r: f32,
    pub fg_g: f32,
    pub fg_b: f32,
    pub bg_r: f32,
    pub bg_g: f32,
    pub bg_b: f32,
    pub frame_time: f32,
    pub warp_intensity: f32,
}

/// GPU-side star data (matches WGSL struct).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GpuStar {
    pub sx: f32,
    pub sy: f32,
    pub prev_sx: f32,
    pub prev_sy: f32,
    pub depth: f32,
    pub hue: f32,
    pub brightness: f32,
    pub has_trail: f32,
}

const MAX_STARS: usize = 2000;

/// Persistent GPU context for starfield rendering.
pub struct GpuStarfieldRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    width: u32,
    height: u32,
    output_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
    star_buffer: wgpu::Buffer,
}

impl GpuStarfieldRenderer {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })).ok()?;

        info!(
            "[Starfield GPU] Using adapter: {:?} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Starfield GPU Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            },
        ))
        .ok()?;

        let shader_source = include_str!("starfield.wgsl");
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Starfield Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Starfield Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Starfield Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Starfield Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let pixel_count = (width as u64) * (height as u64);
        let buffer_size = pixel_count * 4;

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Starfield Output Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Starfield Staging Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let star_buffer_size = (MAX_STARS * std::mem::size_of::<GpuStar>()) as u64;
        let star_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Starfield Star Buffer"),
            size: star_buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        info!("[Starfield GPU] Initialized ({}x{})", width, height);

        Some(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
            width,
            height,
            output_buffer,
            staging_buffer,
            star_buffer,
        })
    }

    pub fn render_frame(&self, params: &StarfieldParams, gpu_stars: &[GpuStar]) -> Vec<u8> {
        if !gpu_stars.is_empty() {
            self.queue.write_buffer(
                &self.star_buffer,
                0,
                bytemuck::cast_slice(gpu_stars),
            );
        }

        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Starfield Uniform Buffer"),
                contents: bytemuck::bytes_of(params),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Starfield Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.star_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.output_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Starfield Encoder"),
            });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Starfield Compute Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups((self.width + 15) / 16, (self.height + 15) / 16, 1);
        }

        let buffer_size = (self.width as u64) * (self.height as u64) * 4;
        encoder.copy_buffer_to_buffer(&self.output_buffer, 0, &self.staging_buffer, 0, buffer_size);
        self.queue.submit(std::iter::once(encoder.finish()));

        let buffer_slice = self.staging_buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let _ = self.device.poll(wgpu::PollType::Wait);

        match receiver.recv() {
            Ok(Ok(())) => {
                let data = buffer_slice.get_mapped_range();
                let result = data.to_vec();
                drop(data);
                self.staging_buffer.unmap();
                result
            }
            _ => {
                warn!("[Starfield GPU] Failed to read back frame data");
                vec![0u8; buffer_size as usize]
            }
        }
    }
}
