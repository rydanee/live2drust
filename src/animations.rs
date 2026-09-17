use std::{
    ffi::CString,
    fs::File,
    io::BufReader,
    path::Path,
    str::FromStr,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::model_interaction::{self, getParameterId, setParameterValue};

#[allow(unused)]
#[derive(Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "PascalCase")]
pub struct MetaData {
    duration: f32,
    fps: f32,
    fade_in_time: f32,
    fade_out_time: f32,
    #[serde(rename = "Loop")]
    looped: bool,
    are_beziers_restricted: bool, //AreBeziersRestricted
    curve_count: i32,
    total_segment_count: i32,
    total_point_count: i32,
    user_data_count: i32,
    total_user_data_size: i32,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Curve {
    target: String,
    id: String,
    segments: Vec<f32>,
}

#[allow(unused)]
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Animation {
    version: i32,
    meta: MetaData,
    curves: Vec<Curve>,
}

#[derive(Debug, Clone, Copy)]
pub struct Point2D {
    pub time: f32,
    pub value: f32,
}

pub fn load_anim(path: &str) -> Animation {
    let file = File::open(path).unwrap();
    let reader = BufReader::new(file);

    let anim: Animation = serde_json::from_reader(reader).unwrap();

    println!("Loaded animation: {}", path);

    anim
}

fn cubic_bezier_1d(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let u = 1.0 - t;
    let tt = t * t;
    let uu = u * u;
    let uuu = uu * u;
    let ttt = tt * t;

    uuu * p0 + 3.0 * uu * t * p1 + 3.0 * u * tt * p2 + ttt * p3
}

fn find_t_for_time(
    p0_time: f32,
    p1_time: f32,
    p2_time: f32,
    p3_time: f32,
    target_time: f32,
) -> f32 {
    let mut t_lower = 0.0;
    let mut t_upper = 1.0;
    let mut t = 0.5;

    for _ in 0..8 {
        let current_time = cubic_bezier_1d(p0_time, p1_time, p2_time, p3_time, t);
        if current_time < target_time {
            t_lower = t;
        } else {
            t_upper = t;
        }

        t = (t_lower + t_upper) * 0.5;
    }
    t
}

pub struct Segment {
    pub p0: Point2D,
    pub p1: Point2D,
    pub p2: Point2D,
    pub p3: Point2D,
}

impl Segment {
    pub fn evaluate(&self, current_time: f32) -> f32 {
        if current_time <= self.p0.time {
            return self.p0.value;
        }
        if current_time <= self.p3.time {
            return self.p3.value;
        }

        let t = find_t_for_time(
            self.p0.time,
            self.p1.time,
            self.p2.time,
            self.p3.time,
            current_time,
        );

        cubic_bezier_1d(
            self.p0.value,
            self.p1.value,
            self.p2.value,
            self.p3.value,
            t,
        )
    }
}

pub fn find_active_bezier_segment(curve: Curve, total_time: f32) -> Option<Segment> {
    if curve.segments.is_empty() {
        return None;
    }

    let mut current_p0 = Point2D {
        time: curve.segments[1],
        value: curve.segments[2],
    };

    let mut offset: usize = 3;

    while (offset as i32) < (curve.segments.len() as i32) - 7 {
        let segment_type = curve.segments[offset] as i32;

        match segment_type {
            1 => {
                let p1_time = curve.segments[offset + 1];
                let p1_value = curve.segments[offset + 2];
                let p2_time = curve.segments[offset + 3];
                let p2_value = curve.segments[offset + 4];
                let p3_time = curve.segments[offset + 5];
                let p3_value = curve.segments[offset + 6];

                let target_p3 = Point2D {
                    time: p3_time,
                    value: p3_value,
                };

                if total_time >= current_p0.time && total_time <= p3_time {
                    return Some(Segment {
                        p0: current_p0,
                        p1: Point2D {
                            time: p1_time,
                            value: p1_value,
                        },
                        p2: Point2D {
                            time: p2_time,
                            value: p2_value,
                        },
                        p3: target_p3,
                    });
                }

                current_p0 = target_p3;
                offset += 7;
            }

            0 => {
                let target_time = curve.segments[offset + 1];
                let target_value = curve.segments[offset + 2];

                if total_time >= current_p0.time && total_time <= target_time {
                    let target_p3 = Point2D {
                        time: target_time,
                        value: target_value,
                    };
                    return Some(Segment {
                        p0: current_p0,
                        p1: current_p0,
                        p2: target_p3,
                        p3: target_p3,
                    });
                }

                current_p0 = Point2D {
                    time: target_time,
                    value: target_value,
                };
                offset += 3;
            }

            2 => {
                let target_time = curve.segments[offset + 1];
                let target_value = curve.segments[offset + 2];

                if total_time >= current_p0.time && total_time <= target_time {
                    let target_p3 = Point2D {
                        time: target_time,
                        value: target_value,
                    };
                    return Some(Segment {
                        p0: current_p0,
                        p1: current_p0,
                        p2: current_p0,
                        p3: target_p3,
                    });
                }

                current_p0 = Point2D {
                    time: target_time,
                    value: target_value,
                };
                offset += 3;
            }

            _ => {
                break;
            }
        }
    }

    None
}

#[derive(Clone, Default)]
pub struct AnimationFrameData {
    pub parameters: Vec<(i32, f32)>,
}

#[derive(Clone, Default)]
pub struct SharedAnimationState {
    pub current_frame: Arc<Mutex<AnimationFrameData>>,
}

#[allow(unsafe_op_in_unsafe_fn)]
pub unsafe fn play_anim(anim: Animation, shared_state: SharedAnimationState) {
    println!("Tryna play anim.");

    std::thread::Builder::new()
        .name("animation0".to_string())
        .spawn(move || {
            let mut start_time = std::time::Instant::now();
            let fps = anim.meta.fps;
            let dtime = 16; // 1000.0 / fps;

            loop {
                let total_time = start_time.elapsed().as_secs_f32();
                let mut next_frame = AnimationFrameData::default();
                let curves = anim.curves.clone();

                for curve in curves {
                    if let Some(segment) = find_active_bezier_segment(curve.clone(), total_time) {
                        let animated_value = segment.evaluate(total_time);

                        next_frame.parameters.push((
                            getParameterId(CString::from_str(curve.id.as_str()).unwrap().as_ptr()),
                            animated_value,
                        ));
                    }
                }

                {
                    let mut current_frame = shared_state.current_frame.lock().unwrap();
                    *current_frame = next_frame;
                }

                if total_time > anim.meta.duration {
                    if anim.meta.looped {
                        start_time = std::time::Instant::now();
                    } else {
                        break;
                    }
                }

                std::thread::sleep(std::time::Duration::from_millis(dtime as u64));
            }

            println!("Finished anim.");
        });
}
