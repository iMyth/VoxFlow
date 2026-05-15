//! GPU-accelerated fractal renderer using wgpu compute shaders.
//!
//! Initializes a wgpu device once and reuses it across all frames for minimal
//! overhead. Falls back to CPU rendering if GPU initialization fails.

use bytemuck::{Pod, Zeroable};
use log::{info, warn};
use wgpu::util::DeviceExt;

/// Uniform parameters passed to the compute shader (must match WGSL struct layout).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct FractalParams {
    pub width: u32,
    pub height: u32,
    pub max_iter: u32,
    pub _pad0: u32,
    pub center_x: f32,
    pub center_y: f32,
    pub scale: f32,
    pub aspect: f32,
    pub fg_r: f32,
    pub fg_g: f32,
    pub fg_b: f32,
    pub hue_offset: f32,
    pub glow_intensity: f32,
    pub brightness_boost: f32,
    pub bg_r: f32,
    pub bg_g: f32,
    pub bg_b: f32,
    pub _pad1: f32,
    pub _pad2: f32,
    pub _pad3: f32,
}

/// Persistent GPU context reused across frames.
pub struct GpuFractalRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    width: u32,
    height: u32,
    output_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
}

impl GpuFractalRenderer {
    /// Try to initialize the GPU renderer. Returns None if GPU is unavailable.
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok()?;

        info!(
            "[Fractal GPU] Using adapter: {:?} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Fractal GPU Device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .ok()?;

        let shader_source = include_str!("fractal.wgsl");
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fractal Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Fractal Bind Group Layout"),
            entries: &[
                // Uniform params
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
                // Output storage buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
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
            label: Some("Fractal Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Fractal Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let pixel_count = (width as u64) * (height as u64);
        let buffer_size = pixel_count * 4; // 4 bytes per pixel (u32 packed RGBA)

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fractal Output Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fractal Staging Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        info!(
            "[Fractal GPU] Initialized successfully ({}x{}, {} pixels)",
            width, height, pixel_count
        );

        Some(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
            width,
            height,
            output_buffer,
            staging_buffer,
        })
    }

    /// Render a single frame on the GPU. Returns RGBA pixel data.
    pub fn render_frame(&self, params: &FractalParams) -> Vec<u8> {
        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Fractal Uniform Buffer"),
                contents: bytemuck::bytes_of(params),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Fractal Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.output_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Fractal Encoder"),
            });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Fractal Compute Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);

            // Dispatch workgroups: ceil(width/16) x ceil(height/16)
            let wg_x = self.width.div_ceil(16);
            let wg_y = self.height.div_ceil(16);
            pass.dispatch_workgroups(wg_x, wg_y, 1);
        }

        let buffer_size = (self.width as u64) * (self.height as u64) * 4;
        encoder.copy_buffer_to_buffer(&self.output_buffer, 0, &self.staging_buffer, 0, buffer_size);

        self.queue.submit(std::iter::once(encoder.finish()));

        // Map the staging buffer and read back
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
                warn!("[Fractal GPU] Failed to read back frame data");
                vec![0u8; buffer_size as usize]
            }
        }
    }
}
