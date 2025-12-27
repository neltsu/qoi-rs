use std::error::Error;
use std::num::NonZeroU32;

use softbuffer::{Buffer, Context, Surface};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::{Window, WindowId};

use qoi_rs::{Decoder, Image, Pixel};

struct App {
    window: Option<Window>,
    ctx: Context<OwnedDisplayHandle>,
    image: Image<Pixel>,
}

impl App {
    fn new(ctx: Context<OwnedDisplayHandle>, image: Image<Pixel>) -> Self {
        Self {
            window: None,
            ctx,
            image,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.window = Some(
            event_loop
                .create_window(Window::default_attributes())
                .unwrap(),
        );
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        // println!("{event:?}");
        match event {
            WindowEvent::CloseRequested => {
                println!("Close was requested; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                // Redraw the application.
                //
                // It's preferable for applications that do not render continuously to render in
                // this event rather than in AboutToWait, since rendering in here allows
                // the program to gracefully handle redraws requested by the OS.

                let window = self
                    .window
                    .as_ref()
                    .expect("redraw request without a window");

                // Notify that you're about to draw.
                window.pre_present_notify();

                // Draw.
                // TODO: cache surface somehow (fight the compiler)
                let mut surface = Surface::new(&self.ctx, window).unwrap();

                let size = window.inner_size();
                surface
                    .resize(
                        NonZeroU32::new(size.width).unwrap(),
                        NonZeroU32::new(size.height).unwrap(),
                    )
                    .unwrap();

                let mut buffer = surface.buffer_mut().unwrap();
                draw_image(&self.image, &mut buffer);

                buffer.present().unwrap();

                // For contiguous redraw loop you can request a redraw from here.
                // window.request_redraw();
            }
            _ => (),
        }
    }
}

fn draw_image<D: HasDisplayHandle, W: HasWindowHandle>(image: &Image<Pixel>, buffer: &mut Buffer<'_, D, W>) {
    let buffer_width = buffer.width().get() as usize;
    for i in 0..image.height {
        for j in 0..image.width {
            if let Some(output) = buffer.get_mut(i * buffer_width + j) {
                let Pixel { r, g, b, .. } = image.pixels[i * image.width + j];
                *output = u32::from_be_bytes([0, r, g, b]);
            }
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().into_iter();
    let _program = args.next().expect("program name");
    let filename = args.next().expect("filename");

    let file = std::fs::read(filename).expect("file exists and is readable");
    let mut decoder = Decoder::new();
    let image = decoder.decode(&file).expect("file is valid QOI image");

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let context = softbuffer::Context::new(event_loop.owned_display_handle())?;

    let mut app = App::new(context, image);

    // For alternative loop run options see `pump_events` and `run_on_demand` examples.
    event_loop.run_app(&mut app).map_err(|e| e.into())
}
