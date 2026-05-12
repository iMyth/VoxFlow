//! GPU-accelerated vinyl disc renderer using wgpu compute shaders.
//!
//! Renders all visual layers (background, bokeh, rings, EQ bars, disc texture,
//! iridescent edge, spindle hole, specular highlight) in a single GPU dispatch.
//! The disc texture is uploaded once and sampled per-frame with rotation.

use bytemuck::{Pod, Zeroable};
use log::{info, warn};
use wgpu::util::DeviceExt;

/// Uniform parameters for the vinyl compute shader.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct VinylParams {
    pub width: f32,
    pub height: f32,
    pub disc_radius: f32,
    pub angle: f32,
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
    pub pulse_scale: f32,
    pub eq_rotation: f32,
    /// Angular velocity (radians per frame) — used for motion blur
    pub angular_velocity: f32,
    pub _pad2: f32,
    pub _pad3: f32,
}

/// Persistent GPU context for vinyl rendering.
pub struct GpuVinylRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    width: u32,
    height: u32,
    output_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
    #[allow(dead_code)]
    disc_texture: wgpu::Texture,
    disc_texture_view: wgpu::TextureView,
    disc_sampler: wgpu::Sampler,
}

impl GpuVinylRenderer {
    /// Initialize the GPU vinyl renderer with a disc texture.
    /// `disc_rgba` is the RGBA pixel data for the disc texture (square, `tex_size x tex_size`).
    pub fn new(width: u32, height: u32, disc_rgba: &[u8], tex_size: u32) -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))?;

        info!(
            "[Vinyl GPU] Using adapter: {:?} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Vinyl GPU Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .ok()?;

        // Create disc texture
        let texture_extent = wgpu::Extent3d {
            width: tex_size,
            height: tex_size,
            depth_or_array_layers: 1,
        };

        let disc_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Vinyl Disc Texture"),
            size: texture_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &disc_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            disc_rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * tex_size),
                rows_per_image: Some(tex_size),
            },
            texture_extent,
        );

        let disc_texture_view = disc_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let disc_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Vinyl Disc Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Shader
        let shader_source = include_str!("vinyl.wgsl");
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Vinyl Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Vinyl Bind Group Layout"),
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
                // Disc texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Output storage buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
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
            label: Some("Vinyl Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Vinyl Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let pixel_count = (width as u64) * (height as u64);
        let buffer_size = pixel_count * 4;

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vinyl Output Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vinyl Staging Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        info!(
            "[Vinyl GPU] Initialized ({}x{}, disc texture {}x{})",
            width, height, tex_size, tex_size
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
            disc_texture,
            disc_texture_view,
            disc_sampler,
        })
    }

    /// Render a single frame on the GPU. Returns RGBA pixel data.
    pub fn render_frame(&self, params: &VinylParams) -> Vec<u8> {
        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vinyl Uniform Buffer"),
                contents: bytemuck::bytes_of(params),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Vinyl Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.disc_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.disc_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.output_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Vinyl Encoder"),
            });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Vinyl Compute Pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);

            let wg_x = (self.width + 15) / 16;
            let wg_y = (self.height + 15) / 16;
            pass.dispatch_workgroups(wg_x, wg_y, 1);
        }

        let buffer_size = (self.width as u64) * (self.height as u64) * 4;
        encoder.copy_buffer_to_buffer(&self.output_buffer, 0, &self.staging_buffer, 0, buffer_size);

        self.queue.submit(std::iter::once(encoder.finish()));

        let buffer_slice = self.staging_buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });

        self.device.poll(wgpu::Maintain::Wait);

        match receiver.recv() {
            Ok(Ok(())) => {
                let data = buffer_slice.get_mapped_range();
                let result = data.to_vec();
                drop(data);
                self.staging_buffer.unmap();
                result
            }
            _ => {
                warn!("[Vinyl GPU] Failed to read back frame data");
                vec![0u8; buffer_size as usize]
            }
        }
    }
}
