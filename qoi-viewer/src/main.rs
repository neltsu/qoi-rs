use std::error::Error;
use std::num::NonZeroU32;

use nalgebra::{Matrix2x1, Matrix3, Point2};
use softbuffer::{Buffer, Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle};
use winit::keyboard::{Key, NamedKey};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::{Window, WindowId};

use qoi_rs::{Decoder, Image, Pixel};

struct App {
    window: Option<Window>,
    context: Option<Context<OwnedDisplayHandle>>,
    image: Image<Pixel>,
    transform: Matrix3<f32>,
    saved_transform: Matrix3<f32>,
    cursor: Option<(f64, f64)>,
    saved: Option<(f64, f64)>,
}

impl App {
    fn new(image: Image<Pixel>) -> Self {
        Self {
            window: None,
            context: None,
            image,
            transform: Matrix3::<f32>::identity(),
            saved_transform: Matrix3::<f32>::identity(),
            cursor: None,
            saved: None,
        }
    }

    fn redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.window = event_loop.create_window(Window::default_attributes()).ok();
        self.context = softbuffer::Context::new(event_loop.owned_display_handle()).ok();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        // println!("{event:?}");
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput {
                event: KeyEvent { logical_key, .. },
                ..
            } => match logical_key {
                Key::Named(NamedKey::Escape) => event_loop.exit(),
                Key::Named(NamedKey::Space) => {
                    self.transform = Matrix3::identity();
                    self.redraw();
                }
                _ => (),
            },
            WindowEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(_, scroll_y),
                ..
            } => {
                let current_scaling = f32::min(
                    *self.transform.get(0).unwrap(),
                    *self.transform.get(4).unwrap(),
                );
                let factor = scroll_y * 0.2 + 1.0;
                if current_scaling < 0.02 && factor <= 1.0 {
                    return;
                }

                let (ox, oy) = self.cursor.unwrap_or_else(|| (0f64, 0f64));
                let mut trans = Matrix2x1::new(-ox as f32, -oy as f32);

                self.transform.append_translation_mut(&trans);
                self.transform.append_scaling_mut(factor);
                trans.neg_mut();
                self.transform.append_translation_mut(&trans);

                self.redraw();
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                if state.is_pressed() {
                    self.saved = self.cursor.clone();
                    self.saved_transform = self.transform.clone();
                } else {
                    self.saved = None;
                }
            }
            WindowEvent::CursorMoved {
                position: PhysicalPosition { x, y },
                ..
            } => {
                self.cursor = Some((x, y));

                let Some((prev_x, prev_y)) = self.saved else {
                    return;
                };
                let delta = Point2::new(x - prev_x, y - prev_y);
                let trans = Matrix2x1::new(delta.x as f32, delta.y as f32);

                self.transform = self.saved_transform.append_translation(&trans);
                self.redraw();
            },
            WindowEvent::RedrawRequested => {
                let window = self.window.as_ref().unwrap();
                let context = self.context.as_ref().unwrap();

                let mut surface = Surface::new(context, window).unwrap();

                let size = window.inner_size();
                surface
                    .resize(
                        NonZeroU32::new(size.width).unwrap(),
                        NonZeroU32::new(size.height).unwrap(),
                    )
                    .unwrap();

                let mut buffer = surface.buffer_mut().unwrap();
                draw_image(&self.image, &self.transform, &mut buffer);

                // Notify that you're about to draw.
                window.pre_present_notify();
                buffer.present().unwrap();
            }
            _ => (),
        }
    }
}

fn draw_image<D: HasDisplayHandle, W: HasWindowHandle>(
    image: &Image<Pixel>,
    transform: &Matrix3<f32>,
    buffer: &mut Buffer<'_, D, W>,
) {
    let tl_i = Point2::new(0 as f32, 0 as f32);
    let br_i = Point2::new(image.width as f32, image.height as f32);

    let tl_b = Point2::new(0 as f32, 0 as f32);
    let br_b = Point2::new(buffer.width().get() as f32, buffer.height().get() as f32);

    let tl_t = transform.transform_point(&tl_i);
    let br_t = transform.transform_point(&br_i);

    let tl = Point2::new(tl_t.x.max(tl_b.x), tl_t.y.max(tl_b.y));
    let br = Point2::new(br_t.x.min(br_b.x), br_t.y.min(br_b.y));

    if tl_b.x >= br_b.x || tl_b.y >= br_b.y {
        return;
    }

    let Some(inv) = transform.try_inverse() else {
        println!("transform matrix = {transform:?}");
        return;
    };
    let bwidth = buffer.width().get() as usize;

    for y in tl.y as usize..br.y as usize {
        for x in tl.x as usize..br.x as usize {
            // if x % 4 + y % 4 != 0 { continue; }

            let pt_b = Point2::new(x as f32, y as f32);
            let pt_i = inv.transform_point(&pt_b);

            let Some(output) = buffer.get_mut(y * bwidth + x) else {
                continue;
            };

            let index = pt_i.y as usize * image.width + pt_i.x as usize;
            let Some(&Pixel { r, g, b, .. }) = image.pixels.get(index) else {
                continue;
            };
            *output = u32::from_be_bytes([0, r, g, b]);
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

    let mut app = App::new(image);

    // For alternative loop run options see `pump_events` and `run_on_demand` examples.
    event_loop.run_app(&mut app).map_err(|e| e.into())
}
