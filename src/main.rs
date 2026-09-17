use std::{
    sync::{Arc, Mutex},
    thread,
};

use animations::{AnimationFrameData, SharedAnimationState, load_anim, play_anim};
use model_interaction::updateModel;
use winit::event_loop::EventLoop;

pub mod animations;
pub mod model_interaction;
pub mod render;

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let shared_frame = SharedAnimationState {
        current_frame: Arc::new(Mutex::new(AnimationFrameData::default())),
    };

    let shared_frame_thread = shared_frame.clone();

    let anim: animations::Animation = animations::load_anim(
        "/home/rydanee/live2drust/models/runtime/motions/mtnBody_laugh.motion3.json",
    );

    unsafe {
        model_interaction::init_model();
        //anims
        play_anim(anim, shared_frame_thread);
    }

    let mut app = render::App {
        state: None,
        shared_anim_state: shared_frame,
    };

    event_loop.run_app(&mut app).unwrap();
}
