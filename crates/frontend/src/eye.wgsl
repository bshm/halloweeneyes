// eye.wgsl — Halloween Eyes shader
//
// Vertex shader generates a full-screen triangle strip (no vertex buffer).
// Fragment shader composites iris + eyelid layers using uniforms.

struct Uniforms {
    look_x:  f32,
    look_y:  f32,
    blink:   f32,
    mirror:  f32,
}

@group(0) @binding(0)
var<uniform> u: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
}

// Full-screen triangle strip: positions for 4 vertices in clip space
var<private> POSITIONS: array<vec2<f32>, 4> = array<vec2<f32>, 4>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 1.0, -1.0),
    vec2<f32>(-1.0,  1.0),
    vec2<f32>( 1.0,  1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    var out: VertexOutput;
    let pos = POSITIONS[idx];
    out.position = vec4<f32>(pos, 0.0, 1.0);
    // UV: (0,0) = bottom-left, (1,1) = top-right
    out.uv = (pos + 1.0) * 0.5;
    return out;
}

// ---------------------------------------------------------------------------
// Fragment shader
//
// Renders a procedural eye for now (texture sampling TODO).
// The iris is a dark circle that moves with look_x / look_y.
// The eyelid mask is a pair of horizontal bands driven by blink level.
// ---------------------------------------------------------------------------

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Normalised UV centred at (0,0)
    var uv = in.uv * 2.0 - 1.0;
    uv.x = uv.x * u.mirror; // mirror for right eye

    // --- Sclera (white of eye) ---
    let eye_r = 0.85;
    let dist_eye = length(uv);
    if dist_eye > eye_r {
        // Outside the eyeball — black
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    var colour = vec3<f32>(0.92, 0.88, 0.82); // sclera

    // --- Iris ---
    let iris_center = vec2<f32>(u.look_x * 0.35, u.look_y * 0.25);
    let iris_r = 0.38;
    let dist_iris = length(uv - iris_center);
    if dist_iris < iris_r {
        // Iris colour: greenish-brown
        colour = vec3<f32>(0.35, 0.28, 0.12);
    }

    // --- Pupil ---
    let pupil_r = 0.18;
    let dist_pupil = length(uv - iris_center);
    if dist_pupil < pupil_r {
        colour = vec3<f32>(0.04, 0.04, 0.04);
    }

    // --- Iris highlight ---
    let highlight_center = iris_center + vec2<f32>(-0.08, -0.12);
    if length(uv - highlight_center) < 0.06 {
        colour = vec3<f32>(1.0, 1.0, 1.0);
    }

    // --- Eyelid occlusion (blink) ---
    // blink = 0 → eye open, blink = 1 → eye fully closed
    let upper_lid_y =  0.85 - u.blink * 1.7;  // descends from top
    let lower_lid_y = -0.85 + u.blink * 0.57;  // rises from bottom

    if uv.y > upper_lid_y || uv.y < lower_lid_y {
        // Lid colour: skin tone
        colour = vec3<f32>(0.15, 0.08, 0.05);
    }

    return vec4<f32>(colour, 1.0);
}
