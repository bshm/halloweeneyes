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
use std::collections::VecDeque;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

macro_rules! log {
    ($($arg:tt)*) => {
        crate::diagnostics::log_remote(&format!("[eye-renderer] {}", format!($($arg)*)))
    };
}

struct RenderState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    mirror: f32,
    target: EyeState,
    current: EyeState,
    last_time: f64,
}

thread_local! {
    static RENDERER: RefCell<Option<RenderState>> = RefCell::new(None);
    /// (server t_ms, state) samples waiting to be played back.
    static TIMELINE: RefCell<VecDeque<(f64, EyeState)>> = RefCell::new(VecDeque::new());
    /// server_ms - local_ms, measured over the websocket/HTTP path.
    static CLOCK_OFFSET_MS: Cell<f64> = const { Cell::new(0.0) };
    static RAF_HANDLE: RefCell<Option<wasm_bindgen::closure::Closure<dyn FnMut(f64)>>> =
        RefCell::new(None);
}

/// Time constant for smoothing toward the newest target (seconds).
const SMOOTH_TAU: f64 = 0.0; // easing happens detector-side; clients snap to latest

/// Playback delay: render the state that the server produced this long ago, so
/// network jitter never shifts the phase. Both boards use the same value, which
/// is what keeps left and right eyes synchronised.
const PLAYBACK_DELAY_MS: f64 = 100.0;

/// Keep at most this much history.
const TIMELINE_KEEP_MS: f64 = 2000.0;

/// Start the display-rate render loop. Eye state arrives at network cadence
/// (tens of Hz, with jitter); drawing at refresh rate and easing toward the
/// newest target removes the visible stepping.
fn start_render_loop() {
    let cb = wasm_bindgen::closure::Closure::wrap(Box::new(move |_ts: f64| {
        RENDERER.with(|r| {
            let mut guard = r.borrow_mut();
            let Some(rs) = guard.as_mut() else { return; };
            let now = web_sys::window()
                .and_then(|w| w.performance())
                .map(|p| p.now())
                .unwrap_or(rs.last_time);
            let dt = ((now - rs.last_time) / 1000.0).max(0.0);
            rs.last_time = now;

            let offset = CLOCK_OFFSET_MS.with(|c| c.get());
            let display_ms = now + offset - PLAYBACK_DELAY_MS;
            if let Some(st) = sample_at(display_ms) {
                rs.target = st;
            }

            let alpha = if SMOOTH_TAU <= 0.0 { 1.0 } else { 1.0 - (-dt / SMOOTH_TAU).exp() };
            rs.current.look_x += (rs.target.look_x - rs.current.look_x) * alpha;
            rs.current.look_y += (rs.target.look_y - rs.current.look_y) * alpha;
            rs.current.blink_level +=
                (rs.target.blink_level - rs.current.blink_level) * alpha;
            rs.current.motion_detected = rs.target.motion_detected;
            rs.current.t_ms = rs.target.t_ms;

            render_frame(
                &rs.device, &rs.queue, &rs.surface,
                &rs.pipeline, &rs.bind_group, &rs.uniform_buffer,
                &rs.current, rs.mirror,
            );

            // Publish the state being displayed so sync can be measured without
            // screenshotting: window.eyeDebug = { t_ms, look_x, look_y, blink }.
            if let Some(win) = web_sys::window() {
                let obj = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&obj, &"t_ms".into(), &rs.current.t_ms.into());
                let _ = js_sys::Reflect::set(&obj, &"look_x".into(), &rs.current.look_x.into());
                let _ = js_sys::Reflect::set(&obj, &"look_y".into(), &rs.current.look_y.into());
                let _ = js_sys::Reflect::set(&obj, &"blink".into(), &rs.current.blink_level.into());
                let _ = js_sys::Reflect::set(&obj, &"mirror".into(), &rs.mirror.into());
                let _ = js_sys::Reflect::set(&obj, &"offset".into(), &offset.into());
                let _ = js_sys::Reflect::set(&obj, &"queued".into(), &(TIMELINE.with(|t| t.borrow().len()) as f64).into());
                let _ = js_sys::Reflect::set(&win, &"eyeDebug".into(), &obj);
            }
        });

        // reschedule on the next display refresh
        RAF_HANDLE.with(|h| {
            if let Some(cb) = h.borrow().as_ref() {
                if let Some(w) = web_sys::window() {
                    let _ = w.request_animation_frame(cb.as_ref().unchecked_ref());
                }
            }
        });
    }) as Box<dyn FnMut(f64)>);

    if let Some(w) = web_sys::window() {
        let _ = w.request_animation_frame(cb.as_ref().unchecked_ref());
    }
    RAF_HANDLE.with(|h| *h.borrow_mut() = Some(cb));
}

/// Queue a timestamped state from the detector.
pub fn push_state(state: EyeState) {
    TIMELINE.with(|t| {
        let mut q = t.borrow_mut();

        // The detector restarts its monotonic clock at zero (service restart,
        // redeploy). Old samples would then sit "in the future" forever, the
        // queue would never prune and the playhead would pin to the oldest
        // sample - a frozen eye. Detect the jump and resync.
        if let Some(&(last, _)) = q.back() {
            if state.t_ms + 1000.0 < last {
                q.clear();
            }
        }

        q.push_back((state.t_ms, state));

        // Hard cap: 240 samples ~ 4 s at 60 Hz, independent of timestamps.
        while q.len() > 240 {
            q.pop_front();
        }
        // Age-based prune on top, for sparse senders.
        while q.len() > 1 && state.t_ms - q.front().map(|s| s.0).unwrap_or(state.t_ms) > TIMELINE_KEEP_MS {
            q.pop_front();
        }
    });
}

/// Offset between the detector clock and this page's `performance.now()`.
pub fn set_clock_offset_ms(offset: f64) {
    CLOCK_OFFSET_MS.with(|c| c.set(offset));
}

/// Compatibility entry point: state without a usable timestamp is played now.
pub fn update_eye_state(state: EyeState) {
    RENDERER.with(|r| {
        if let Some(rs) = r.borrow_mut().as_mut() {
            rs.target = state;
        }
    });
    push_state(state);
}

/// State to display at server time `now_ms`, linearly interpolated.
fn sample_at(now_ms: f64) -> Option<EyeState> {
    TIMELINE.with(|t| {
        let q = t.borrow();
        if q.is_empty() {
            return None;
        }
        let mut prev: Option<(f64, EyeState)> = None;
        for (ts, st) in q.iter() {
            if *ts >= now_ms {
                if let Some((pts, pst)) = prev {
                    let span = (*ts - pts).max(0.0001);
                    let a = ((now_ms - pts) / span).clamp(0.0, 1.0);
                    return Some(EyeState {
                        look_x: pst.look_x + (st.look_x - pst.look_x) * a,
                        look_y: pst.look_y + (st.look_y - pst.look_y) * a,
                        blink_level: pst.blink_level + (st.blink_level - pst.blink_level) * a,
                        motion_detected: st.motion_detected,
                        t_ms: now_ms,
                    });
                }
                return Some(*st);
            }
            prev = Some((*ts, *st));
        }
        prev.map(|(_, st)| st)
    })
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

    log!("fetching BG2.png ...");
    let bg_bytes = fetch_bytes("/public/BG2.png").await?;
    let (bg_w, bg_h, bg_data) = decode_rgba(&bg_bytes)?;
    log!("BG2.png decoded: {}x{}", bg_w, bg_h);

    log!("fetching IRIS2.png ...");
    let iris_bytes = fetch_bytes("/public/IRIS2.png").await?;
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

    // Render the initial frame immediately so the eye is visible before any WebSocket update.
    let initial_state = EyeState::default_open();
    render_frame(&device, &queue, &surface, &pipeline, &bind_group, &uniform_buffer, &initial_state, mirror);

    let start_time = web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0);

    RENDERER.with(|r| {
        *r.borrow_mut() = Some(RenderState {
            device, queue, surface, pipeline, bind_group, uniform_buffer, mirror,
            target: initial_state,
            current: initial_state,
            last_time: start_time,
        });
    });
    start_render_loop();
    log!("renderer ready (display-rate loop, tau {:.0} ms)", SMOOTH_TAU * 1000.0);
    Ok(())
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
