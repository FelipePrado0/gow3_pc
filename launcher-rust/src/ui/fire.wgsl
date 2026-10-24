// The fire's embers and flames (spec 4.3 item 5): one quad per particle, added to the scene.
// The particles come from the simulation the canvas fire draws too (`fire.rs`).

struct Scene {
    // The layer's size in logical pixels, the unit of the particles.
    size: vec2<f32>,
    padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> scene: Scene;

struct Particle {
    @location(0) centre: vec2<f32>,
    @location(1) radius: f32,
    // 0 for an ember, 1 for a flame.
    @location(2) kind: f32,
    // Straight colour; the alpha is the one at the centre.
    @location(3) colour: vec4<f32>,
};

struct Fragment {
    @builtin(position) position: vec4<f32>,
    // Where the fragment sits in its particle, in radii from the centre.
    @location(0) offset: vec2<f32>,
    @location(1) @interpolate(flat) kind: f32,
    @location(2) @interpolate(flat) colour: vec4<f32>,
};

// The quad reaches a pixel past the radius, room for the soft edge of an ember.
const MARGIN: f32 = 1.0;

@vertex
fn vs_main(@builtin(vertex_index) index: u32, particle: Particle) -> Fragment {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, 1.0),
    );
    let corner = corners[index];
    let reach = particle.radius + MARGIN;
    let point = particle.centre + corner * reach;
    var out: Fragment;
    out.position = vec4<f32>(
        point.x / scene.size.x * 2.0 - 1.0,
        1.0 - point.y / scene.size.y * 2.0,
        0.0,
        1.0,
    );
    out.offset = corner * reach / particle.radius;
    out.kind = particle.kind;
    out.colour = particle.colour;
    return out;
}

@fragment
fn fs_main(in: Fragment) -> @location(0) vec4<f32> {
    let distance = length(in.offset);
    // Derivatives need uniform control flow: taken before choosing the shape.
    let edge = fwidth(distance);
    // An ember is a solid disc with a one-pixel soft edge.
    let ember = 1.0 - smoothstep(1.0 - edge, 1.0 + edge, distance);
    // A flame fades from its centre to its rim, the radial gradient the canvas fakes with rings.
    let flame = 1.0 - smoothstep(0.0, 1.0, distance);
    let alpha = in.colour.a * select(ember, flame, in.kind > 0.5);
    // Premultiplied for the additive blend (One, One); the alpha written is ignored.
    return vec4<f32>(in.colour.rgb * alpha, 0.0);
}
