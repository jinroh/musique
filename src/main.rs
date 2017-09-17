extern crate gl;
extern crate glutin;

use std::error;
use std::io::{self, Write};
use glutin::GlContext;

fn window_event_loop() -> Result<(), Box<error::Error>> {
    let mut events_loop = glutin::EventsLoop::new();

    let window = glutin::WindowBuilder::new()
        .with_title("Musique")
        .with_multitouch();

    let context = glutin::ContextBuilder::new()
        .with_vsync(true)
        .with_gl(glutin::GlRequest::GlThenGles {
                     opengl_version: (3, 2),
                     opengles_version: (3, 0),
                 });

    let gl_window = glutin::GlWindow::new(window, context, &events_loop)?;

    let gl = match gl::GlType::default() {
        gl::GlType::Gl => unsafe {
            gl::GlFns::load_with(|symbol| window.get_proc_address(symbol) as *const _)
        },
        gl::GlType::Gles => unsafe {
            gl::GlesFns::load_with(|symbol| window.get_proc_address(symbol) as *const _)
        },
    };

    loop {
        events_loop.poll_events(|event| {
                                    println!("{:?}", event);
                                });

        unsafe {
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        gl_window.swap_buffers()?;
    }
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
