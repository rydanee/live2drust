use std::{
    sync::{Arc, Mutex},
    thread,
};

use animations::{AnimationFrameData, SharedAnimationState, load_anim, play_anim};
use model_interaction::updateModel;
use winit::event_loop::EventLoop;

use crate::model_interaction::setPartOpacity;

pub mod animations;
pub mod expressions;
pub mod model_interaction;
pub mod physics;
pub mod render;

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);

    let anim: animations::Animation = animations::load_anim(
        "/home/rydanee/live2drust/models/runtime/motions/mtnBody_think2.motion3.json",
    );

    unsafe {
        model_interaction::init_model();

        play_anim(anim);
    }

    let mut app = render::App { state: None };

    event_loop.run_app(&mut app).unwrap();
}
