//! wgpu-based eye renderer.
//!
//! Uses WebGL2 via the wgpu WebGL backend (Firefox + Chrome).  The same
//! code runs natively (Vulkan/Metal/DX12) inside a future Tauri shell.
//!
//! # Rendering approach
//!
//! The eye is drawn as a full-screen quad rendered by a WGSL fragment
//! shader that procedurally composites the iris, pupil, sclera, and eyelid
//! layers driven by uniforms (look_x, look_y, blink, mirror).
//!
//! ```text
//! ┌─────────────────────────────┐
//! │  background (BG)            │  — static eyelid mask image
//! │  ┌──────────────────────┐   │
//! │  │  iris (IRIS)         │   │  — moved by (look_x, look_y)
//! │  └──────────────────────┘   │
//! │  upper-lid BG overlay       │  — shifted by blink_level
//! │  lower-lid BG overlay       │  — shifted by blink_level
//! └─────────────────────────────┘
//! ```
//!
//! The shader takes three uniforms: `look` (vec2), `blink` (float), and
//! `mirror` (float, +1 or -1 for left/right eye).

use anyhow::{anyhow, Result};
use shared::EyeState;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::prelude::*;

// Global eye state shared between the Leptos signal handler and the render loop.
// WASM is single-threaded so Cell / RefCell are safe here.
thread_local! {
    static CURRENT_STATE: RefCell<EyeState> = RefCell::new(EyeState::default_open());
    static STATE_DIRTY: Cell<bool> = Cell::new(false);
}

/// Called by the Leptos effect whenever the eye state changes.
pub fn update_eye_state(state: EyeState) {
    CURRENT_STATE.with(|s| *s.borrow_mut() = state);
    STATE_DIRTY.with(|d| d.set(true));
}

/// Initialises the wgpu renderer and starts the rAF animation loop.
pub async fn init_renderer(canvas: web_sys::HtmlCanvasElement) -> Result<()> {
    let width = canvas.width();
    let height = canvas.height();

    // Default instance selects WebGL2 on WASM (with the `webgl` feature).
    let instance = wgpu::Instance::default();

    // Safe canvas surface creation — requires `webgl` feature in wgpu.
    let surface: wgpu::Surface<'static> = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
        .map_err(|e| anyhow!("create_surface: {e}"))?;

    // wgpu 29: request_adapter returns Result<Adapter, RequestAdapterError>
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .map_err(|e| anyhow!("request_adapter: {e}"))?;

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .map_err(|e| anyhow!("request_device: {e}"))?;

    let surface_format = wgpu::TextureFormat::Rgba8Unorm;
    surface.configure(
        &device,
        &wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        },
    );

    let shader_src = include_str!("eye.wgsl");
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("eye_shader"),
        source: wgpu::ShaderSource::Wgsl(shader_src.into()),
    });

    // Uniform buffer: [look_x: f32, look_y: f32, blink: f32, mirror: f32]
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("eye_uniforms"),
        size: (4 * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("eye_bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("eye_bg"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("eye_pipeline_layout"),
        // wgpu 29: bind_group_layouts is &[Option<&BindGroupLayout>]
        bind_group_layouts: &[Some(&bind_group_layout)],
        // wgpu 29: push_constant_ranges replaced by immediate_size
        immediate_size: 0,
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("eye_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        // wgpu 29: renamed from `multiview`
        multiview_mask: None,
        cache: None,
    });

    start_render_loop(device, queue, surface, pipeline, bind_group, uniform_buffer);

    Ok(())
}

fn start_render_loop(
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
) {
    let state = Rc::new(RefCell::new((
        device,
        queue,
        surface,
        pipeline,
        bind_group,
        uniform_buffer,
    )));

    // Classic Rust/WASM requestAnimationFrame loop pattern.
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::new(move || {
        let dirty = STATE_DIRTY.with(|d| d.replace(false));
        if dirty {
            let eye = CURRENT_STATE.with(|s| *s.borrow());
            let mut guard = state.borrow_mut();
            let (device, queue, surface, pipeline, bind_group, uniform_buffer) = &mut *guard;
            render_frame(device, queue, surface, pipeline, bind_group, uniform_buffer, &eye);
        }

        web_sys::window()
            .unwrap()
            .request_animation_frame(f.borrow().as_ref().unwrap().as_ref().unchecked_ref())
            .expect("requestAnimationFrame failed");
    }));

    web_sys::window()
        .unwrap()
        .request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref())
        .expect("requestAnimationFrame failed");
}

fn render_frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surface: &wgpu::Surface,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    uniform_buffer: &wgpu::Buffer,
    eye: &EyeState,
) {
    let uniforms = [
        eye.look_x as f32,
        eye.look_y as f32,
        eye.blink_level as f32,
        1.0f32, // mirror: 1.0 = left eye
    ];
    queue.write_buffer(uniform_buffer, 0, bytemuck::cast_slice(&uniforms));

    // wgpu 29: CurrentSurfaceTexture is an enum with Success/Suboptimal/error variants
    let frame = match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(t)
        | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
        _ => return,
    };
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("eye_encoder"),
    });

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("eye_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                // wgpu 29: new required field
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            // wgpu 29: renamed from `multiview`
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..4, 0..1); // full-screen triangle strip
    }

    queue.submit([encoder.finish()]);
    frame.present();
}
