//! wgpu-based eye renderer — uses BG.png + IRIS.png from the original Qt project.
//!
//! Rendering matches QtEye.cpp:
//!   1. IRIS at irisLookTranslation
//!   2. BG at upperLidTranslation
//!   3. BG at lowerLidTranslation
//!   4. BG at bgLookTranslation

use anyhow::{anyhow, Result};
use shared::EyeState;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

thread_local! {
    static CURRENT_STATE: RefCell<EyeState> = RefCell::new(EyeState::default_open());
    static STATE_DIRTY: Cell<bool> = Cell::new(false);
}

pub fn update_eye_state(state: EyeState) {
    CURRENT_STATE.with(|s| *s.borrow_mut() = state);
    STATE_DIRTY.with(|d| d.set(true));
}

macro_rules! log {
    ($($arg:tt)*) => {
        crate::diagnostics::log_remote(&format!("[eye-renderer] {}", format!($($arg)*)))
    };
}

async fn fetch_bytes(url: &str) -> Result<Vec<u8>> {
    let window = web_sys::window().ok_or_else(|| anyhow!("no window"))?;
    let promise = window.fetch_with_str(url);
    let resp = JsFuture::from(promise)
        .await
        .map_err(|e| anyhow!("fetch {url}: {:?}", e))?;
    let resp: web_sys::Response = resp.dyn_into().map_err(|e| anyhow!("cast: {:?}", e))?;
    let buf_promise = resp.array_buffer().map_err(|e| anyhow!("array_buffer: {:?}", e))?;
    let buf = JsFuture::from(buf_promise)
        .await
        .map_err(|e| anyhow!("await buf: {:?}", e))?;
    let array = js_sys::Uint8Array::new(&buf);
    Ok(array.to_vec())
}

fn decode_rgba(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map_err(|e| anyhow!("decode png: {e}"))?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Ok((w, h, img.into_raw()))
}

fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    width: u32,
    height: u32,
    data: &[u8],
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    texture
}

pub async fn init_renderer(canvas: web_sys::HtmlCanvasElement, mirror: f32) -> Result<()> {
    let width = canvas.width();
    let height = canvas.height();
    log!("init_renderer start: canvas {}x{}", width, height);

    log!("fetching BG.png ...");
    let bg_bytes = fetch_bytes("/public/BG.png").await?;
    let (bg_w, bg_h, bg_data) = decode_rgba(&bg_bytes)?;
    log!("BG.png decoded: {}x{}", bg_w, bg_h);

    log!("fetching IRIS.png ...");
    let iris_bytes = fetch_bytes("/public/IRIS.png").await?;
    let (iris_w, iris_h, iris_data) = decode_rgba(&iris_bytes)?;
    log!("IRIS.png decoded: {}x{}", iris_w, iris_h);

    struct WebDisplay(wgpu::rwh::DisplayHandle<'static>);
    unsafe impl Send for WebDisplay {}
    unsafe impl Sync for WebDisplay {}
    impl std::fmt::Debug for WebDisplay {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "WebDisplay") }
    }
    impl wgpu::rwh::HasDisplayHandle for WebDisplay {
        fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
            Ok(self.0)
        }
    }

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::GL,
        display: Some(Box::new(WebDisplay(wgpu::rwh::DisplayHandle::web()))),
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    log!("instance created");

    let surface: wgpu::Surface<'static> = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
        .map_err(|e| anyhow!("create_surface: {e}"))?;

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .map_err(|e| anyhow!("request_adapter: {e}"))?;
    log!("adapter: {:?}", adapter.get_info());

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
            ..Default::default()
        })
        .await
        .map_err(|e| anyhow!("request_device: {e}"))?;

    let caps = surface.get_capabilities(&adapter);
    let surface_format = caps.formats.first().copied()
        .ok_or_else(|| anyhow!("no surface format"))?;
    log!("using surface format: {:?}", surface_format);

    let alpha_mode = if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
        wgpu::CompositeAlphaMode::Opaque
    } else {
        caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto)
    };

    surface.configure(&device, &wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width,
        height,
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    });

    let bg_tex = upload_texture(&device, &queue, "bg", bg_w, bg_h, &bg_data);
    let iris_tex = upload_texture(&device, &queue, "iris", iris_w, iris_h, &iris_data);
    let bg_view = bg_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let iris_view = iris_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("eye_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    log!("textures uploaded");

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("eye_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("eye.wgsl").into()),
    });

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("eye_uniforms"),
        size: (4 * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("eye_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("eye_bg"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&bg_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&iris_view) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&sampler) },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("eye_layout"),
        bind_group_layouts: &[Some(&bgl)],
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
        multiview_mask: None,
        cache: None,
    });

    log!("pipeline created");
    start_render_loop(device, queue, surface, pipeline, bind_group, uniform_buffer, mirror);
    log!("render loop started");
    Ok(())
}

fn start_render_loop(
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    mirror: f32,
) {
    let state = Rc::new(RefCell::new((device, queue, surface, pipeline, bind_group, uniform_buffer, mirror)));
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();
    let frame_count = Rc::new(Cell::new(0u64));

    *g.borrow_mut() = Some(Closure::new(move || {
        STATE_DIRTY.with(|d| d.set(false)); // consume dirty flag
        let eye = CURRENT_STATE.with(|s| *s.borrow());
        let n = frame_count.get();
        frame_count.set(n + 1);
        if n % 60 == 0 {
            log!("frame {} — look=({:.2},{:.2}) blink={:.2}", n, eye.look_x, eye.look_y, eye.blink_level);
        }
        {
            let mut guard = state.borrow_mut();
            let (device, queue, surface, pipeline, bind_group, uniform_buffer, mirror) = &mut *guard;
            render_frame(device, queue, surface, pipeline, bind_group, uniform_buffer, &eye, *mirror);
        }
        web_sys::window()
            .unwrap()
            .request_animation_frame(f.borrow().as_ref().unwrap().as_ref().unchecked_ref())
            .expect("rAF failed");
    }));

    web_sys::window()
        .unwrap()
        .request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref())
        .expect("rAF failed");
}

fn render_frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surface: &wgpu::Surface,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    uniform_buffer: &wgpu::Buffer,
    eye: &EyeState,
    mirror: f32,
) {
    let uniforms = [eye.look_x as f32, eye.look_y as f32, eye.blink_level as f32, mirror];
    queue.write_buffer(uniform_buffer, 0, bytemuck::cast_slice(&uniforms));

    let frame = match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(t) => t,
        wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
        err => { log!("surface error: {:?}", err); return; }
    };
    let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("eye_enc") });
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
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
    queue.submit([encoder.finish()]);
    frame.present();
}
