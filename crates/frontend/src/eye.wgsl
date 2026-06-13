// eye.wgsl — Halloween Eyes shader
//
// Replicates QtEye.cpp rendering exactly using BG.png and IRIS.png.
//
// Qt constants (pixels):
//   canvas: 800 x 600
//   texture: 2000 x 2000
//   irisCenter: (998, 998)
//   center: (400, 300)
//   imageTranslation = center - irisCenter = (-598, -698)
//   irisMoveFactor: (250, 150)
//   bgMoveFactor: (75, 50)
//   upperLidMovement: (0, 375)   blink pushes lid down
//   lowerLidMovement: (0, -125)  blink pushes lid up
//
// Qt draw order (painter's algorithm, each layer alpha-composites over previous):
//   1. IRIS at irisLookTranslation
//   2. BG  at upperLidTranslation
//   3. BG  at lowerLidTranslation
//   4. BG  at bgLookTranslation

struct Uniforms {
    look_x: f32,
    look_y: f32,
    blink:  f32,
    mirror: f32,  // +1.0 = left eye, -1.0 = right eye
}

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var t_bg:   texture_2d<f32>;
@group(0) @binding(2) var t_iris: texture_2d<f32>;
@group(0) @binding(3) var s_eye:  sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
}

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
    // UV (0,0) = bottom-left, (1,1) = top-right
    out.uv = (pos + 1.0) * 0.5;
    return out;
}

// Alpha-composite src over dst
fn blend(dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    let a = src.a;
    return vec4<f32>(src.rgb * a + dst.rgb * (1.0 - a), dst.a + a * (1.0 - dst.a));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Canvas pixel coords (Qt convention: y=0 at top)
    let cx = in.uv.x * 800.0;
    let cy = (1.0 - in.uv.y) * 600.0;

    // Qt constants
    let img_tx = -598.0;
    let img_ty = -698.0;

    let iris_look_x = u.look_x * 250.0;
    let iris_look_y = u.look_y * 150.0;
    let bg_look_x   = u.look_x * 75.0;
    let bg_look_y   = u.look_y * 50.0;

    let iris_tx = img_tx + iris_look_x;
    let iris_ty = img_ty + iris_look_y;
    let bg_tx   = img_tx + bg_look_x;
    let bg_ty   = img_ty + bg_look_y;
    let upper_tx = bg_tx;
    let upper_ty = bg_ty + u.blink * 375.0;
    let lower_tx = bg_tx;
    let lower_ty = bg_ty + u.blink * (-125.0);

    let tex_size = 2000.0;

    // Raw U coordinates before mirroring
    let iris_u_raw  = (cx - iris_tx)  / tex_size;
    let bg_u_raw    = (cx - bg_tx)    / tex_size;
    let upper_u_raw = (cx - upper_tx) / tex_size;
    let lower_u_raw = (cx - lower_tx) / tex_size;

    // Qt mirrors the textures around irisCenter for the right eye.
    // Replicate by flipping U when mirror = -1.
    let flip = u.mirror < 0.0;
    let iris_uv  = vec2<f32>(select(iris_u_raw,  1.0 - iris_u_raw,  flip), (cy - iris_ty)  / tex_size);
    let bg_uv    = vec2<f32>(select(bg_u_raw,    1.0 - bg_u_raw,    flip), (cy - bg_ty)    / tex_size);
    let upper_uv = vec2<f32>(select(upper_u_raw, 1.0 - upper_u_raw, flip), (cy - upper_ty) / tex_size);
    let lower_uv = vec2<f32>(select(lower_u_raw, 1.0 - lower_u_raw, flip), (cy - lower_ty) / tex_size);

    let c_iris  = textureSample(t_iris, s_eye, iris_uv);
    let c_bg    = textureSample(t_bg,   s_eye, bg_uv);
    let c_upper = textureSample(t_bg,   s_eye, upper_uv);
    let c_lower = textureSample(t_bg,   s_eye, lower_uv);

    var out = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    out = blend(out, c_iris);
    out = blend(out, c_upper);
    out = blend(out, c_lower);
    out = blend(out, c_bg);

    return vec4<f32>(out.rgb, 1.0);
}
