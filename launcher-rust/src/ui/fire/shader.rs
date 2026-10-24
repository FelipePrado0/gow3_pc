//! The wgpu fire: the shared simulation drawn by `fire.wgsl`, one additive quad per particle.

use iced::mouse;
use iced::wgpu;
use iced::widget::shader::{self, Viewport};
use iced::{Color, Point, Rectangle, Size};

use super::{FireSim, Particle, ParticleKind, FLAME_RINGS, SEED};
use crate::ui::app::Message;

const FIRE_WGSL: &str = include_str!("../fire.wgsl");
/// A flame's alpha at the centre: its rings added up, as the additive blend adds them.
const FLAME_PEAK: f32 = FLAME_RINGS[0].1 + FLAME_RINGS[1].1 + FLAME_RINGS[2].1;
/// Floats per instance: centre (2), radius, kind, colour (4).
const INSTANCE_FLOATS: usize = 8;
const INSTANCE_BYTES: u64 = (INSTANCE_FLOATS * size_of::<f32>()) as u64;
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32x4];
/// Instances the first buffer holds: the particles alive at full intensity, rounded up.
const INITIAL_CAPACITY: usize = 128;
/// The `Scene` uniform: size (2) and padding (2).
const SCENE_BYTES: u64 = 16;

/// The shader fire: the shared simulation, drawn on the GPU.
#[derive(Debug)]
pub struct FireShader {
    sim: FireSim,
}

impl Default for FireShader {
    fn default() -> Self {
        FireShader::seeded(SEED)
    }
}

impl FireShader {
    pub fn seeded(seed: u32) -> Self {
        FireShader {
            sim: FireSim::seeded(seed),
        }
    }

    pub fn set_intensity(&mut self, intensity: f32) {
        self.sim.set_intensity(intensity);
    }

    /// Advances the fire by `dt` seconds in a window of `size`.
    pub fn tick(&mut self, dt: f32, size: Size) {
        self.sim.tick(dt, size);
    }

    pub(super) fn sim(&self) -> &FireSim {
        &self.sim
    }
}

impl shader::Program<Message> for FireShader {
    type State = ();
    type Primitive = FirePrimitive;

    fn draw(&self, _state: &(), _cursor: mouse::Cursor, _bounds: Rectangle) -> FirePrimitive {
        FirePrimitive {
            glows: self.sim.particles.iter().map(Glow::of).collect(),
        }
    }
}

/// A particle as the shader draws it.
#[derive(Debug)]
struct Glow {
    centre: Point,
    radius: f32,
    kind: ParticleKind,
    /// Straight sRGB colour; the alpha is the one at the centre.
    colour: Color,
}

impl Glow {
    fn of(particle: &Particle) -> Self {
        let colour = particle.colour();
        let peak = match particle.kind {
            ParticleKind::Ember => colour.a,
            ParticleKind::Flame => colour.a * FLAME_PEAK,
        };
        Glow {
            centre: particle.pos,
            radius: particle.radius,
            kind: particle.kind,
            colour: Color { a: peak, ..colour },
        }
    }

    /// The vertex buffer's instance; on an sRGB target the colour goes in linear, like iced's own.
    fn instance(&self, linear: bool) -> [f32; INSTANCE_FLOATS] {
        let Color { r, g, b, a } = self.colour;
        let [r, g, b, a] = if linear {
            self.colour.into_linear()
        } else {
            [r, g, b, a]
        };
        let kind = match self.kind {
            ParticleKind::Ember => 0.0,
            ParticleKind::Flame => 1.0,
        };
        [self.centre.x, self.centre.y, self.radius, kind, r, g, b, a]
    }
}

/// The fire of one frame.
#[derive(Debug)]
pub struct FirePrimitive {
    glows: Vec<Glow>,
}

fn bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

impl shader::Primitive for FirePrimitive {
    type Pipeline = FirePipeline;

    fn prepare(
        &self,
        pipeline: &mut FirePipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        if self.glows.is_empty() {
            return;
        }
        let scene = [bounds.width, bounds.height, 0.0, 0.0];
        queue.write_buffer(&pipeline.scene, 0, &bytes(&scene));
        let instances: Vec<f32> = self
            .glows
            .iter()
            .flat_map(|glow| glow.instance(pipeline.linear))
            .collect();
        pipeline.reserve(device, self.glows.len());
        queue.write_buffer(&pipeline.instances, 0, &bytes(&instances));
    }

    fn draw(&self, pipeline: &FirePipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        if !self.glows.is_empty() {
            render_pass.set_pipeline(&pipeline.pipeline);
            render_pass.set_bind_group(0, &pipeline.bind_group, &[]);
            render_pass.set_vertex_buffer(0, pipeline.instances.slice(..));
            render_pass.draw(0..6, 0..self.glows.len() as u32);
        }
        true
    }
}

/// The GPU side every fire primitive shares. One fire is drawn per frame (the splash's or the
/// Home's, never both), so one set of buffers is enough.
pub struct FirePipeline {
    pipeline: wgpu::RenderPipeline,
    scene: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    /// Instances `instances` holds.
    capacity: usize,
    /// Whether the target is sRGB, so the colours go in linear.
    linear: bool,
}

impl FirePipeline {
    fn reserve(&mut self, device: &wgpu::Device, count: usize) {
        if count > self.capacity {
            self.capacity = count.next_power_of_two();
            self.instances = instance_buffer(device, self.capacity);
        }
    }
}

fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fire instances"),
        size: capacity as u64 * INSTANCE_BYTES,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl shader::Pipeline for FirePipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fire.wgsl"),
            source: wgpu::ShaderSource::Wgsl(FIRE_WGSL.into()),
        });
        let scene = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fire scene"),
            size: SCENE_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fire scene"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fire scene"),
            layout: &scene_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: scene.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fire"),
            bind_group_layouts: &[&scene_layout],
            push_constant_ranges: &[],
        });
        // Additive (spec 4.3 item 5): the fire's light adds to the scene; the target's alpha stays.
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fire"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: INSTANCE_BYTES,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &INSTANCE_ATTRIBUTES,
                }],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(additive),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        FirePipeline {
            pipeline,
            scene,
            bind_group,
            instances: instance_buffer(device, INITIAL_CAPACITY),
            capacity: INITIAL_CAPACITY,
            linear: format.is_srgb(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::ui::fire::{Fire, FireCanvas};
    use iced::futures::executor::block_on;
    use iced::widget::shader::{Pipeline, Program};
    use iced::{mouse, Rectangle, Size};

    #[test]
    fn both_variants_share_the_spawn_rules() {
        let size = Size::new(1000.0, 800.0);
        let mut canvas = Fire::Canvas(FireCanvas::seeded(42));
        let mut shader = Fire::Shader(FireShader::seeded(42));
        for tick in 0..150 {
            let intensity = if tick < 100 { 1.0 } else { 0.3 };
            for fire in [&mut canvas, &mut shader] {
                fire.set_intensity(intensity);
                fire.tick(0.033, size);
            }
        }
        let (Fire::Canvas(canvas), Fire::Shader(shader)) = (&canvas, &shader) else {
            unreachable!()
        };
        // Same particles: positions, velocities, lives, kinds and radii.
        assert!(!canvas.sim.particles.is_empty());
        assert_eq!(canvas.sim.particles, shader.sim.particles);
        // And the shader draws each of them where the canvas does.
        let primitive = Program::<Message>::draw(
            shader,
            &(),
            mouse::Cursor::Unavailable,
            Rectangle::with_size(size),
        );
        let drawn: Vec<_> = primitive
            .glows
            .iter()
            .map(|glow| (glow.centre, glow.radius, glow.kind))
            .collect();
        let particles: Vec<_> = canvas
            .sim
            .particles
            .iter()
            .map(|particle| (particle.pos, particle.radius, particle.kind))
            .collect();
        assert_eq!(drawn, particles);
    }

    /// Compiles `fire.wgsl` and builds its pipeline on any adapter, a software one included (WARP,
    /// llvmpipe), so a WGSL mistake fails here and not on a player's screen.
    #[test]
    fn the_fire_shader_compiles() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = [false, true].into_iter().find_map(|fallback| {
            block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                force_fallback_adapter: fallback,
                ..Default::default()
            }))
            .ok()
        });
        let Some(adapter) = adapter else {
            println!("no graphics adapter, not even a software one: fire.wgsl not compiled");
            return;
        };
        let info = adapter.get_info();
        let request = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_limits: adapter.limits(),
            ..Default::default()
        }));
        let Ok((device, queue)) = request else {
            println!("no device on {info:?}: fire.wgsl not compiled");
            return;
        };
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fire.wgsl"),
            source: wgpu::ShaderSource::Wgsl(FIRE_WGSL.into()),
        });
        for format in [
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let _pipeline = <FirePipeline as Pipeline>::new(&device, &queue, format);
        }
        let error = block_on(device.pop_error_scope());
        assert!(error.is_none(), "fire.wgsl on {info:?}: {error:?}");
        println!("fire.wgsl compiled on {} ({:?})", info.name, info.backend);
    }
}
