//! GPU-accelerated particle kaleidoscope renderer using wgpu compute shaders.
//!
//! CPU handles particle physics (spawn, update, remove with randomness).
//! GPU handles rendering: each pixel checks all particles with kaleidoscope
//! symmetry and alpha-blends them. Particle data is uploaded each frame via
//! a storage buffer.

use bytemuck::{Pod, Zeroable};
use log::{info, warn};
use wgpu::util::DeviceExt;

use super::particle_system::{Particle, ParticleKind, ParticleSystem, Ring};

/// Uniform parameters for the particle compute shader.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct ParticleParams {
    pub width: u32,
    pub height: u32,
    pub num_particles: u32,
    pub num_rings: u32,
    pub symmetry_folds: u32,
    pub frame_idx: u32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub rms: f32,
    pub low_energy: f32,
    pub mid_energy: f32,
    pub high_energy: f32,
    pub bg_r: f32,
    pub bg_g: f32,
    pub bg_b: f32,
    pub base_hue: f32,
}

/// GPU-side particle data (packed, matches WGSL struct).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GpuParticle {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub life: f32,
    pub hue: f32,
    pub saturation: f32,
    pub brightness: f32,
    pub kind: u32,
}

/// GPU-side ring data (packed, matches WGSL struct).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GpuRing {
    pub radius: f32,
    pub thickness: f32,
    pub life: f32,
    pub hue: f32,
    pub brightness: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}

/// Maximum particles and rings the GPU buffers can hold.
const MAX_PARTICLES: usize = 6000;
const MAX_RINGS: usize = 32;

/// Persistent GPU context for particle rendering.
pub struct GpuParticleRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    width: u32,
    height: u32,
    output_buffer: wgpu::Buffer,
    staging_buffer: wgpu::Buffer,
    particle_buffer: wgpu::Buffer,
    ring_buffer: wgpu::Buffer,
}

impl GpuParticleRenderer {
    /// Initialize the GPU particle renderer.
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
            "[Particles GPU] Using adapter: {:?} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Particles GPU Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            },
        ))
        .ok()?;

        let shader_source = include_str!("particles.wgsl");
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Particles Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(shader_source.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Particles Bind Group Layout"),
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
                // Particle storage (read-only)
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
                // Ring storage (read-only)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
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
            label: Some("Particles Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Particles Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let pixel_count = (width as u64) * (height as u64);
        let buffer_size = pixel_count * 4;

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particles Output Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particles Staging Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let particle_buffer_size = (MAX_PARTICLES * std::mem::size_of::<GpuParticle>()) as u64;
        let particle_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particles Data Buffer"),
            size: particle_buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let ring_buffer_size = (MAX_RINGS * std::mem::size_of::<GpuRing>()) as u64;
        let ring_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Rings Data Buffer"),
            size: ring_buffer_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        info!(
            "[Particles GPU] Initialized ({}x{}, max {} particles, max {} rings)",
            width, height, MAX_PARTICLES, MAX_RINGS
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
            particle_buffer,
            ring_buffer,
        })
    }

    /// Render a single frame. Uploads current particle state to GPU and dispatches.
    pub fn render_frame(&self, params: &ParticleParams, system: &ParticleSystem) -> Vec<u8> {
        // Take the most recent particles (at the end of the vec) — they are the
        // most visible since they have the highest life values. This avoids an
        // expensive O(n log n) sort that was causing slowdown on long videos.
        let particle_count = system.particles.len().min(MAX_PARTICLES);
        let start = system.particles.len().saturating_sub(particle_count);
        let gpu_particles: Vec<GpuParticle> = system.particles[start..]
            .iter()
            .map(|p| particle_to_gpu(p))
            .collect();

        let gpu_rings: Vec<GpuRing> = system
            .rings
            .iter()
            .take(MAX_RINGS)
            .map(|r| ring_to_gpu(r))
            .collect();

        // Upload particle data
        if !gpu_particles.is_empty() {
            self.queue.write_buffer(
                &self.particle_buffer,
                0,
                bytemuck::cast_slice(&gpu_particles),
            );
        }

        // Upload ring data
        if !gpu_rings.is_empty() {
            self.queue
                .write_buffer(&self.ring_buffer, 0, bytemuck::cast_slice(&gpu_rings));
        }

        // Create uniform buffer
        let uniform_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Particles Uniform Buffer"),
                contents: bytemuck::bytes_of(params),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Particles Bind Group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.particle_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.ring_buffer.as_entire_binding(),
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
                label: Some("Particles Encoder"),
            });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Particles Compute Pass"),
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
                warn!("[Particles GPU] Failed to read back frame data");
                vec![0u8; buffer_size as usize]
            }
        }
    }
}

/// Convert a CPU Particle to GPU format.
#[inline]
fn particle_to_gpu(p: &Particle) -> GpuParticle {
    GpuParticle {
        x: p.x,
        y: p.y,
        size: p.current_size(),
        life: p.life,
        hue: p.hue,
        saturation: p.saturation,
        brightness: p.brightness,
        kind: match p.kind {
            ParticleKind::Dot => 0,
            ParticleKind::Glow => 1,
            ParticleKind::Spark => 2,
        },
    }
}

/// Convert a CPU Ring to GPU format.
#[inline]
fn ring_to_gpu(r: &Ring) -> GpuRing {
    GpuRing {
        radius: r.radius,
        thickness: r.thickness,
        life: r.life,
        hue: r.hue,
        brightness: r.brightness,
        _pad0: 0.0,
        _pad1: 0.0,
        _pad2: 0.0,
    }
}
