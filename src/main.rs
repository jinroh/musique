extern crate gleam;
extern crate glutin;
extern crate webrender;

use std::error;
use std::io::{self, Write};

use gleam::gl;
use webrender::api::*;

fn window_event_loop() -> Result<(), Box<error::Error>> {
    let window = glutin::WindowBuilder::new()
        .with_title("WebRender Sample App")
        .with_multitouch()
        .with_vsync()
        .with_gl(glutin::GlRequest::GlThenGles {
                     opengl_version: (3, 2),
                     opengles_version: (3, 0),
                 })
        .build()
        .unwrap();

    unsafe { window.make_current().ok() };

    let gl = match gl::GlType::default() {
        gl::GlType::Gl => unsafe {
            gl::GlFns::load_with(|symbol| window.get_proc_address(symbol) as *const _)
        },
        gl::GlType::Gles => unsafe {
            gl::GlesFns::load_with(|symbol| window.get_proc_address(symbol) as *const _)
        },
    };

    let (width, height) = window.get_inner_size_pixels().unwrap();
    let mut cursor_position = WorldPoint::zero();
    let mut window_size = DeviceUintSize::new(width, height);
    let mut dpi_factor = window.hidpi_factor();
    let mut layout_size = LayoutSize::new((width as f32) / dpi_factor,
                                          (height as f32) / dpi_factor);

    let opts = webrender::RendererOptions {
        debug: true,
        precache_shaders: false,
        // enable_subpixel_aa: true, // TODO decide
        // enable_aa: true,
        device_pixel_ratio: dpi_factor,
        ..webrender::RendererOptions::default()
    };

    let (mut renderer, sender) = webrender::Renderer::new(gl, opts).unwrap();
    let notifier = Box::new(Notifier::new(window.create_window_proxy()));
    renderer.set_render_notifier(notifier);

    let api = sender.create_api();
    let root_document_id = api.add_document(window_size);
    let root_pipeline_id = PipelineId(0, 0);
    api.set_root_pipeline(root_document_id, root_pipeline_id);

    let epoch = Epoch(0);
    let root_background_color = Some(ColorF::new(0.7, 0.6, 0.1, 1.0));

    {

        let builder = DisplayListBuilder::new(root_pipeline_id, layout_size);
        let resources = ResourceUpdates::new();

        api.set_display_list(root_document_id,
                             epoch,
                             root_background_color,
                             layout_size,
                             builder.finalize(),
                             true,
                             resources);
        api.generate_frame(root_document_id, None);

        renderer.update();
        renderer.render(window_size).unwrap();
        window.swap_buffers().unwrap();

        window.show();
    }

    let mut scroll_offset = LayoutPoint::zero();
    let mut touch_start = (LayoutPoint::zero(), LayoutPoint::zero());
    let root_clip = ClipId::new(1, root_pipeline_id);


    let mut events_queue = vec![];

    'outer: for event in window.wait_events() {
        use std::mem;

        let mut events = mem::replace(&mut events_queue, vec![]);

        events.push(event);

        for event in window.poll_events() {
            events.push(event);
        }

        let mut set_window_parameters = false;
        let mut set_root_scroll = false;

        use glutin::Event::*;
        use glutin::ElementState::*;
        use glutin::MouseScrollDelta::*;

        for event in events {
            match event {
                Closed |
                KeyboardInput(_, _, Some(glutin::VirtualKeyCode::Escape)) |
                KeyboardInput(_, _, Some(glutin::VirtualKeyCode::Q)) => break 'outer,
                KeyboardInput(Pressed, _, Some(glutin::VirtualKeyCode::D)) => {
                    let mut flags = renderer.get_debug_flags();
                    flags.toggle(webrender::PROFILER_DBG);
                    renderer.set_debug_flags(flags);
                }
                Moved(_, _) => {
                    dpi_factor = window.hidpi_factor();
                    set_window_parameters = true;
                }
                Resized(w, h) => {
                    window_size = DeviceUintSize::new(w, h);
                    layout_size = LayoutSize::new((w as f32) / dpi_factor, (h as f32) / dpi_factor);
                    set_window_parameters = true;
                }
                MouseMoved(x, y) => {
                    cursor_position = WorldPoint::new((x as f32) / dpi_factor,
                                                      (y as f32) / dpi_factor);
                }
                MouseWheel(delta, _, _) => {
                    const LINE_HEIGHT: f32 = 24.0;
                    let dy = match delta {
                        PixelDelta(_, dy) => dy,
                        LineDelta(_, dy) => (dy * LINE_HEIGHT),
                    };

                    scroll_offset += LayoutVector2D::new(0.0, -(dy as f32));
                    scroll_offset.y = scroll_offset.y.round();
                    set_root_scroll = true;
                }
                Touch(glutin::Touch { location, phase, .. }) => {
                    let touch_position = LayoutPoint::new(location.0 as f32, location.1 as f32);

                    if phase == glutin::TouchPhase::Ended ||
                       phase == glutin::TouchPhase::Cancelled {
                        touch_start = (LayoutPoint::zero(), LayoutPoint::zero());
                        break;
                    }

                    if phase == glutin::TouchPhase::Started {
                        touch_start = (scroll_offset, touch_position);
                    }

                    scroll_offset.y = touch_start.0.y + (touch_start.1.y - touch_position.y);
                    scroll_offset.y = scroll_offset.y.round();
                    set_root_scroll = true;
                }
                _ => {}
            }
        }

        if set_window_parameters {
            api.set_window_parameters(root_document_id,
                                      window_size,
                                      DeviceUintRect::new(DeviceUintPoint::zero(), window_size),
                                      dpi_factor);
            // window.set_inner_size(window_size.width, window_size.height);
        }

        if set_root_scroll {
            if scroll_offset.x < 0.0 {
                scroll_offset.x = 0.0
            }
            if scroll_offset.y < 0.0 {
                scroll_offset.y = 0.0
            }
            api.scroll_node_with_id(root_document_id,
                                    scroll_offset,
                                    root_clip,
                                    ScrollClamping::NoClamping);
        }

        let mut root_builder = DisplayListBuilder::new(root_pipeline_id, layout_size);
        let resources = ResourceUpdates::new();

        let scroll_width = layout_size.width;
        let scroll_height = 6.0 * 512.0;

        let bounds = LayoutRect::new(LayoutPoint::zero(), layout_size);
        root_builder.push_stacking_context(&PrimitiveInfo::new(bounds),
                                           ScrollPolicy::Scrollable,
                                           None,
                                           TransformStyle::Flat,
                                           None,
                                           MixBlendMode::Normal,
                                           vec![]);
        let content_rect = LayoutRect::new(LayoutPoint::zero(),
                                           LayoutSize::new(scroll_width, scroll_height));
        root_builder.define_scroll_frame(Some(root_clip),
                                         content_rect,
                                         bounds,
                                         vec![],
                                         None,
                                         ScrollSensitivity::ScriptAndInputEvents);
        root_builder.push_clip_id(root_clip);

        // ----------

        for i in 0..256 {
            let f = 1.0 - (i as f32) / 256.0;
            let mut m = WorldPoint::new(cursor_position.x, cursor_position.y);
            let c = (layout_size.width / 2.0, layout_size.height / 2.0);
            if m.x > c.0 {
                m.x = layout_size.width - m.x;
            }
            if m.y > c.1 {
                m.y = layout_size.height - m.y;
            }
            let r = ((c.0 - (c.0 - m.x) * f) as i32, (c.1 - (c.1 - m.y) * f) as i32)
                .to((c.0 + (c.0 - m.x) * f) as i32,
                    (c.1 + (c.1 - m.y) * f) as i32);

            let info = LayoutPrimitiveInfo::new(r);
            root_builder.push_rect(&info,
                                   ColorF::new(0.5 * (1.0 - f),
                                               0.5 * (1.0 - f),
                                               0.5 * (1.0 - f),
                                               1.0));
        }

        for i in 0..256 {
            let h = 35;
            let p = 10;

            let f = (i as f32) / 256.0;
            let rect = (5, i * h + p).to(layout_size.width as i32 - 5, (i + 1) * h);
            let info = LayoutPrimitiveInfo::new(rect);

            let color = if i % 2 == 0 {
                ColorF::new(f, f, f, 1.0)
            } else {
                ColorF::new((1.0 - f), (1.0 - f), (1.0 - f), 1.0)
            };

            root_builder.push_rect(&info, color);
        }

        // ----------

        root_builder.pop_clip_id();
        root_builder.pop_stacking_context();

        api.set_display_list(root_document_id,
                             epoch,
                             root_background_color,
                             layout_size,
                             root_builder.finalize(),
                             true,
                             resources);
        api.generate_frame(root_document_id, None);

        renderer.update();
        renderer.render(window_size).unwrap();
        window.swap_buffers().unwrap();
    }

    renderer.deinit();

    Ok(())
}

fn main() {
    ::std::process::exit(match window_event_loop() {
                             Ok(_) => 0,
                             Err(err) => {
                                 writeln!(io::stderr(), "{}", err.description()).unwrap();
                                 1
                             }
                         })
}

struct Notifier {
    window_proxy: glutin::WindowProxy,
}

impl Notifier {
    fn new(window_proxy: glutin::WindowProxy) -> Notifier {
        Notifier { window_proxy }
    }
}

pub trait HandyDandyRectBuilder {
    fn to(&self, x2: i32, y2: i32) -> LayoutRect;
    fn by(&self, w: i32, h: i32) -> LayoutRect;
}

impl HandyDandyRectBuilder for (i32, i32) {
    fn to(&self, x2: i32, y2: i32) -> LayoutRect {
        LayoutRect::new(LayoutPoint::new(self.0 as f32, self.1 as f32),
                        LayoutSize::new((x2 - self.0) as f32, (y2 - self.1) as f32))
    }

    fn by(&self, w: i32, h: i32) -> LayoutRect {
        LayoutRect::new(LayoutPoint::new(self.0 as f32, self.1 as f32),
                        LayoutSize::new(w as f32, h as f32))
    }
}

impl HandyDandyRectBuilder for (f32, f32) {
    fn to(&self, x2: i32, y2: i32) -> LayoutRect {
        LayoutRect::new(LayoutPoint::new(self.0, self.1),
                        LayoutSize::new(((x2 as f32) - self.0), ((y2 as f32) - self.1)))
    }

    fn by(&self, w: i32, h: i32) -> LayoutRect {
        LayoutRect::new(LayoutPoint::new(self.0, self.1),
                        LayoutSize::new(w as f32, h as f32))
    }
}

impl RenderNotifier for Notifier {
    fn new_frame_ready(&mut self) {
        // #[cfg(not(target_os = "android"))]
        // self.window_proxy.wakeup_event_loop();
    }

    fn new_scroll_frame_ready(&mut self, _composite_needed: bool) {
        // #[cfg(not(target_os = "android"))]
        // self.window_proxy.wakeup_event_loop();
    }
}
