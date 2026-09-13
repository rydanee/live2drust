use winit::event_loop::EventLoop;

pub mod model_interaction;
pub mod render;

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = render::App::default();

    unsafe {
        model_interaction::init_model();
    }

    event_loop.run_app(&mut app).unwrap();
}
