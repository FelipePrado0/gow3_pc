//! Embers and flames behind the Home page (spec 4.3 item 5); the heat pulse under them is the
//! Home's layer 5 (`home.rs`), the same for both fires. The splash has none (R96): the fire comes
//! in with the Home, as the splash's flight fades its dark background out.

use iced::futures::executor::block_on;
use iced::mouse;
use iced::wgpu;
use iced::widget::canvas::{self, Geometry, Path};
use iced::widget::{canvas as canvas_widget, container, shader as shader_widget, Id};
use iced::{Color, Element, Fill, Point, Rectangle, Renderer, Size, Theme, Vector};

use crate::ui::app::Message;

mod shader;

pub use shader::FireShader;

/// The fire layer: drawn by a wgpu shader, or by a `canvas` when iced renders with tiny-skia.
#[derive(Debug)]
pub enum Fire {
    Canvas(FireCanvas),
    Shader(FireShader),
}

impl Default for Fire {
    fn default() -> Self {
        Fire::new(false)
    }
}

impl Fire {
    /// The shader fire when `use_shader` (see [`probe_gpu`]), the canvas fire otherwise.
    pub fn new(use_shader: bool) -> Self {
        if use_shader {
            Fire::Shader(FireShader::default())
        } else {
            Fire::Canvas(FireCanvas::default())
        }
    }

    pub fn set_intensity(&mut self, intensity: f32) {
        match self {
            Fire::Canvas(fire) => fire.set_intensity(intensity),
            Fire::Shader(fire) => fire.set_intensity(intensity),
        }
    }

    pub fn tick(&mut self, dt: f32, size: Size) {
        match self {
            Fire::Canvas(fire) => fire.tick(dt, size),
            Fire::Shader(fire) => fire.tick(dt, size),
        }
    }

    fn sim(&self) -> &FireSim {
        match self {
            Fire::Canvas(fire) => &fire.sim,
            Fire::Shader(fire) => fire.sim(),
        }
    }

    /// The intensity `set_intensity` last gave.
    pub fn intensity(&self) -> f32 {
        self.sim().intensity
    }

    /// The embers and flames alive.
    pub fn particle_count(&self) -> usize {
        self.sim().particles.len()
    }

    /// The fire filling its layer, a container with `layer_id`.
    pub fn view(&self) -> Element<'_, Message> {
        let fire: Element<'_, Message> = match self {
            Fire::Canvas(fire) => canvas_widget(fire).width(Fill).height(Fill).into(),
            Fire::Shader(fire) => shader_widget(fire).width(Fill).height(Fill).into(),
        };
        container(fire)
            .id(layer_id())
            .width(Fill)
            .height(Fill)
            .into()
    }
}

/// The id of the fire's layer.
pub fn layer_id() -> Id {
    Id::new("fire")
}

/// Whether the fire can be the wgpu shader. Asks the GPU, so call it once, at start-up.
pub fn probe_gpu() -> bool {
    let backend = std::env::var("ICED_BACKEND").ok();
    use_shader(backend.as_deref(), adapter_found)
}

/// Whether iced will render with wgpu, the only renderer that draws a shader. `backend_env` is
/// `ICED_BACKEND`, a comma-separated list iced tries in order, each entry with wgpu then with
/// tiny-skia; `adapter_found` asks the GPU and is called only when wgpu gets its turn.
pub fn use_shader(backend_env: Option<&str>, adapter_found: impl FnOnce() -> bool) -> bool {
    let mut candidates = backend_env
        .unwrap_or_default()
        .split(',')
        .filter(|candidate| !candidate.is_empty())
        .peekable();
    if candidates.peek().is_none() {
        return adapter_found();
    }
    for candidate in candidates {
        match candidate {
            // Without an adapter wgpu fails here, and would again on any later "wgpu".
            "wgpu" => return adapter_found(),
            "tiny-skia" | "tiny_skia" => return false,
            _ => {}
        }
    }
    false
}

/// Whether wgpu finds a graphics adapter, asked the way iced asks for its window
/// (`WGPU_BACKEND`, `WGPU_POWER_PREF`) but without a surface.
fn adapter_found() -> bool {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::from_env().unwrap_or(wgpu::Backends::all()),
        flags: wgpu::InstanceFlags::empty(),
        ..Default::default()
    });
    let options = wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::from_env()
            .unwrap_or(wgpu::PowerPreference::HighPerformance),
        compatible_surface: None,
        force_fallback_adapter: false,
    };
    block_on(instance.request_adapter(&options)).is_ok()
}

/// How much fire there is: none while the splash covers the window (`splash`; R96), 0.8 on a
/// focused Home, 1.0 under the first-run setup, 0.3 unfocused, none when hidden or when effects
/// are off.
pub fn intensity_for(
    splash: bool,
    setup: bool,
    focused: bool,
    visible: bool,
    effects: bool,
) -> f32 {
    if !effects || !visible || splash {
        0.0
    } else if !focused {
        0.3
    } else if setup {
        1.0
    } else {
        0.8
    }
}

/// Seed of the fire's random source.
const SEED: u32 = 0x5eed_f1e5;
/// Particles alive at full intensity.
const FULL_COUNT: f32 = 120.0;
/// Particles born per tick at most, so the fire fills in over a second instead of in one burst.
const MAX_SPAWN_PER_TICK: usize = 6;
/// Share of the spawns that are flames.
const FLAME_SHARE: f32 = 0.22;
/// Height of the spawn line on the right edge, as a share of the window height.
const RISE: f32 = 0.22;
/// Extra life on the right edge.
const EXTRA_LIFE: f32 = 0.7;
/// A flame's rings, outside in: share of the radius and alpha at full life.
const FLAME_RINGS: [(f32, f32); 3] = [(1.0, 0.05), (0.65, 0.07), (0.35, 0.10)];

/// Small deterministic generator; the fire needs variety, not quality.
#[derive(Debug, Clone)]
struct Xorshift32 {
    state: u32,
}

impl Xorshift32 {
    fn new(seed: u32) -> Self {
        Xorshift32 {
            state: if seed == 0 { 0x9e37_79b9 } else { seed },
        }
    }

    /// Uniform in `[0, 1)`.
    fn next_f32(&mut self) -> f32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.next_f32()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticleKind {
    Ember,
    Flame,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Particle {
    pos: Point,
    vel: Vector,
    life: f32,
    max_life: f32,
    kind: ParticleKind,
    radius: f32,
}

impl Particle {
    /// Share of the life left, 1 at birth and 0 at death.
    fn remaining(&self) -> f32 {
        (1.0 - self.life / self.max_life).clamp(0.0, 1.0)
    }

    /// Colour at the centre, fading with age; a flame's alpha scales the alphas of its rings.
    fn colour(&self) -> Color {
        let left = self.remaining();
        match self.kind {
            ParticleKind::Ember => Color::from_rgba(1.0, 0.45 + 0.3 * left, 0.12, 0.9 * left),
            ParticleKind::Flame => Color::from_rgba(0.95, 0.25 + 0.2 * left, 0.05, left),
        }
    }
}

/// The particles both fires draw: spawn rules, motion and their own random source.
#[derive(Debug)]
struct FireSim {
    particles: Vec<Particle>,
    intensity: f32,
    rng: Xorshift32,
    /// Where every particle was born (x, y), for the tests.
    #[cfg(test)]
    spawn_log: Vec<(f32, f32)>,
}

impl FireSim {
    fn seeded(seed: u32) -> Self {
        FireSim {
            particles: Vec::new(),
            intensity: 0.0,
            rng: Xorshift32::new(seed),
            #[cfg(test)]
            spawn_log: Vec::new(),
        }
    }

    fn set_intensity(&mut self, intensity: f32) {
        self.intensity = intensity.clamp(0.0, 1.0);
    }

    fn spawn(&mut self, size: Size) {
        let x = self.rng.next_f32() * size.width;
        let line = size.height - RISE * size.height * (x / size.width);
        let y = line + self.rng.range(-6.0, 6.0);
        let flame = self.rng.next_f32() < FLAME_SHARE;
        let (kind, rise, radius, base_life) = if flame {
            (
                ParticleKind::Flame,
                self.rng.range(30.0, 80.0),
                self.rng.range(14.0, 34.0),
                self.rng.range(1.2, 2.2),
            )
        } else {
            (
                ParticleKind::Ember,
                self.rng.range(60.0, 160.0),
                self.rng.range(1.0, 2.5),
                self.rng.range(2.5, 4.5),
            )
        };
        let drift = self.rng.range(-15.0, 15.0);
        #[cfg(test)]
        self.spawn_log.push((x, y));
        self.particles.push(Particle {
            pos: Point::new(x, y),
            vel: Vector::new(drift, -rise),
            life: 0.0,
            max_life: base_life * (1.0 + EXTRA_LIFE * x / size.width),
            kind,
            radius,
        });
    }

    /// Advances the fire by `dt` seconds in a window of `size`.
    fn tick(&mut self, dt: f32, size: Size) {
        if self.intensity <= 0.0 {
            // Nothing stays frozen on screen when the clock stops.
            self.particles.clear();
            return;
        }
        for particle in &mut self.particles {
            particle.life += dt;
            particle.pos += particle.vel * dt;
        }
        self.particles
            .retain(|p| p.life < p.max_life && p.pos.y > -p.radius);
        if size.width > 0.0 && size.height > 0.0 {
            let target = (FULL_COUNT * self.intensity) as usize;
            let mut born = 0;
            while self.particles.len() < target && born < MAX_SPAWN_PER_TICK {
                self.spawn(size);
                born += 1;
            }
        }
    }

    #[cfg(test)]
    fn take_spawned(&mut self) -> Vec<(f32, f32)> {
        std::mem::take(&mut self.spawn_log)
    }
}

/// The canvas fire: the shared simulation and a geometry cache.
#[derive(Debug)]
pub struct FireCanvas {
    sim: FireSim,
    cache: canvas::Cache,
}

impl Default for FireCanvas {
    fn default() -> Self {
        FireCanvas::seeded(SEED)
    }
}

impl FireCanvas {
    pub fn seeded(seed: u32) -> Self {
        FireCanvas {
            sim: FireSim::seeded(seed),
            cache: canvas::Cache::new(),
        }
    }

    pub fn set_intensity(&mut self, intensity: f32) {
        self.sim.set_intensity(intensity);
    }

    /// Advances the fire by `dt` seconds in a window of `size`.
    pub fn tick(&mut self, dt: f32, size: Size) {
        self.sim.tick(dt, size);
        self.cache.clear();
    }
}

impl canvas::Program<Message> for FireCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            for particle in &self.sim.particles {
                let colour = particle.colour();
                match particle.kind {
                    ParticleKind::Ember => {
                        frame.fill(&Path::circle(particle.pos, particle.radius), colour);
                    }
                    ParticleKind::Flame => {
                        // The canvas has no radial gradient: stacked discs fake the falloff.
                        for (scale, alpha) in FLAME_RINGS {
                            let ring = Color {
                                a: alpha * colour.a,
                                ..colour
                            };
                            frame.fill(&Path::circle(particle.pos, particle.radius * scale), ring);
                        }
                    }
                }
            }
        });
        vec![geometry]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Size;

    #[test]
    fn zero_intensity_leaves_no_particles() {
        let size = Size::new(1000.0, 800.0);
        let mut fire = FireCanvas::seeded(7);
        fire.set_intensity(1.0);
        fire.tick(0.033, size);
        assert!(!fire.sim.particles.is_empty());
        fire.set_intensity(0.0);
        fire.tick(0.033, size);
        assert!(fire.sim.particles.is_empty());
    }

    #[test]
    fn intensity_rules() {
        // R96: no fire while the splash covers the window.
        assert_eq!(intensity_for(true, false, true, true, true), 0.0);
        assert_eq!(intensity_for(true, false, false, true, true), 0.0);
        assert_eq!(intensity_for(false, false, true, true, true), 0.8);
        assert_eq!(intensity_for(false, false, false, true, true), 0.3);
        assert_eq!(intensity_for(true, false, true, false, true), 0.0);
        assert_eq!(intensity_for(true, false, true, true, false), 0.0);
        assert_eq!(intensity_for(false, true, true, true, true), 1.0);
        assert_eq!(intensity_for(false, true, false, true, true), 0.3);
    }

    #[test]
    fn particles_spawn_higher_on_the_right() {
        let size = Size::new(1000.0, 800.0);
        let mut fire = FireCanvas::seeded(12345);
        fire.set_intensity(1.0);
        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut spawned = 0;
        for _ in 0..600 {
            fire.tick(0.033, size);
            for y_x in fire.sim.take_spawned() {
                spawned += 1;
                if y_x.0 < 0.2 * size.width {
                    left.push(y_x.1);
                } else if y_x.0 > 0.8 * size.width {
                    right.push(y_x.1);
                }
            }
            if spawned >= 200 && !left.is_empty() && !right.is_empty() {
                break;
            }
        }
        assert!(spawned >= 200);
        let avg = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        assert!(avg(&right) < avg(&left));
    }

    #[test]
    fn fire_falls_back_to_canvas_without_a_gpu() {
        // iced renders with tiny-skia: no shader, and the GPU is not even asked.
        for backend in ["tiny-skia", "tiny_skia", "tiny-skia,wgpu"] {
            assert!(!use_shader(Some(backend), || unreachable!("{backend}")));
        }
        // wgpu comes first but finds no adapter, so iced falls back to tiny-skia.
        assert!(!use_shader(None, || false));
        assert!(!use_shader(Some("wgpu,tiny-skia"), || false));
        // wgpu with an adapter.
        for backend in [None, Some(""), Some("wgpu"), Some("wgpu,tiny-skia")] {
            assert!(use_shader(backend, || true), "{backend:?}");
        }
        assert!(matches!(Fire::new(false), Fire::Canvas(_)));
        assert!(matches!(Fire::new(true), Fire::Shader(_)));
        assert!(matches!(Fire::default(), Fire::Canvas(_)));
    }
}
