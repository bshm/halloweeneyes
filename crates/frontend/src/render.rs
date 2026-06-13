//! wgpu-based eye renderer.
//!
//! Uses WebGL2 via the wgpu WebGL backend (Firefox + Chrome).  The same
//! code runs natively (Vulkan/Metal/DX12) inside a future Tauri shell.
//!
//! # Rendering approach
//!
//! The eye is drawn as two textured quads rendered by a simple fragment
//! shader that composes the iris layer over the background/eyelid mask:
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

use anyhow::Result;
use leptos::html::HtmlElement;
use leptos::html::Canvas as LeptosCanvas;
use shared::EyeState;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use wasm_bindgen::JsCast;

// Global eye state shared between the Leptos signal handler and the render loop.
// Using thread_local because WASM is single-threaded.
thread_local! {
    static CURRENT_STATE: Cell<EyeState> = Cell::new(EyeState::default_open());
    static STATE_DIRTY: AtomicBool = const { AtomicBool::new(false) };
}

/// Called by the Leptos effect whenever the eye state changes.
pub fn update_eye_state(state: EyeState) {
    CURRENT_STATE.with(|s| s.set(state));
    STATE_DIRTY.with(|d| d.store(true, Ordering::Relaxed));
}

/// Initialises the wgpu renderer and starts the animation loop.
pub async fn init_renderer(canvas: HtmlElement<LeptosCanvas>) -> Result<()> {
    let html_canvas = canvas.unchecked_ref::<web_sys::HtmlCanvasElement>();
    let width = html_canvas.width();
    let height = html_canvas.height();

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::GL,
        ..Default::default()
    });

    // SAFETY: the canvas element is kept alive by the DOM for the entire
    // duration of the page.
    let surface = unsafe {
        instance
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::from_canvas(html_canvas)?)
            .map_err(|e| anyhow::anyhow!("create_surface: {e}"))?
    };

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .ok_or_else(|| anyhow::anyhow!("no suitable GPU adapter found"))?;

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default(), None)
        .await
        .map_err(|e| anyhow::anyhow!("request_device: {e}"))?;

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

    // Minimal WGSL shader: renders a full-screen quad and colours it based on
    // eye state uniforms.  Texture sampling is a TODO — images are loaded from
    // the server in a subsequent step.
    let shader_src = include_str!("eye.wgsl");
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("eye_shader"),
        source: wgpu::ShaderSource::Wgsl(shader_src.into()),
    });

    // Uniform buffer: [look_x: f32, look_y: f32, blink: f32, mirror: f32]
    let uniform_data = [0.0f32; 4];
    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("eye_uniforms"),
        size: (uniform_data.len() * std::mem::size_of::<f32>()) as u64,
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
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
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
        multiview: None,
        cache: None,
    });

    // Hand off to the animation loop using requestAnimationFrame
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
    use wasm_bindgen::prelude::*;
    use web_sys::window;

    // Rc-wrapped state for the closure
    let state = std::rc::Rc::new(std::cell::RefCell::new((
        device,
        queue,
        surface,
        pipeline,
        bind_group,
        uniform_buffer,
    )));

    let f = std::rc::Rc::new(std::cell::RefCell::new(None::<Closure<dyn FnMut()>>));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::new(move || {
        let eye = CURRENT_STATE.with(|s| s.get());
        let dirty = STATE_DIRTY.with(|d| d.swap(false, Ordering::Relaxed));

        if dirty {
            let (device, queue, surface, pipeline, bind_group, uniform_buffer) =
                &*state.borrow();
            render_frame(device, queue, surface, pipeline, bind_group, uniform_buffer, &eye);
        }

        // Schedule next frame
        window()
            .unwrap()
            .request_animation_frame(
                f.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
            )
            .expect("requestAnimationFrame failed");
    }));

    window()
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
        1.0f32, // mirror: 1.0 = left eye (no mirror)
    ];
    queue.write_buffer(uniform_buffer, 0, bytemuck::cast_slice(&uniforms));

    let frame = match surface.get_current_texture() {
        Ok(f) => f,
        Err(_) => return,
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
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..4, 0..1); // full-screen triangle strip
    }

    queue.submit([encoder.finish()]);
    frame.present();
}
