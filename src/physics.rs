use serde::Deserialize;
use std::ffi::CString;
use std::fs::File;
use std::io::Read;

#[derive(Deserialize, Debug, Clone)]
pub struct PhysicsVec2 {
    #[serde(rename = "X")]
    pub x: f32,
    #[serde(rename = "Y")]
    pub y: f32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PhysicsMeta {
    #[serde(rename = "Fps")]
    pub fps: Option<f32>,
    #[serde(rename = "EffectiveForces")]
    pub effective_forces: EffectiveForces,
}

#[derive(Deserialize, Debug, Clone)]
pub struct EffectiveForces {
    #[serde(rename = "Gravity")]
    pub gravity: PhysicsVec2,
    #[serde(rename = "Wind")]
    pub wind: PhysicsVec2,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PhysicsTarget {
    #[serde(rename = "Target")]
    pub target_type: String,
    #[serde(rename = "Id")]
    pub id: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PhysicsInput {
    #[serde(rename = "Source")]
    pub source: PhysicsTarget,
    #[serde(rename = "Weight")]
    pub weight: f32,
    #[serde(rename = "Type")]
    pub input_type: String,
    #[serde(rename = "Reflect")]
    pub reflect: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PhysicsOutput {
    #[serde(rename = "Destination")]
    pub destination: PhysicsTarget,
    #[serde(rename = "VertexIndex")]
    pub vertex_index: usize,
    #[serde(rename = "Scale")]
    pub scale: f32,
    #[serde(rename = "Weight")]
    pub weight: f32,
    #[serde(rename = "Type")]
    pub output_type: String,
    #[serde(rename = "Reflect")]
    pub reflect: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PhysicsVertex {
    #[serde(rename = "Position")]
    pub position: PhysicsVec2,
    #[serde(rename = "Mobility")]
    pub mobility: f32,
    #[serde(rename = "Delay")]
    pub delay: f32,
    #[serde(rename = "Acceleration")]
    pub acceleration: f32,
    #[serde(rename = "Radius")]
    pub radius: f32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct NormalizationRange {
    #[serde(rename = "Minimum")]
    pub minimum: f32,
    #[serde(rename = "Default")]
    pub default: f32,
    #[serde(rename = "Maximum")]
    pub maximum: f32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PhysicsNormalization {
    #[serde(rename = "Position")]
    pub position: NormalizationRange,
    #[serde(rename = "Angle")]
    pub angle: NormalizationRange,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PhysicsSetting {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Input")]
    pub inputs: Vec<PhysicsInput>,
    #[serde(rename = "Output")]
    pub outputs: Vec<PhysicsOutput>,
    #[serde(rename = "Vertices")]
    pub vertices: Vec<PhysicsVertex>,
    #[serde(rename = "Normalization")]
    pub normalization: PhysicsNormalization,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Physics3Json {
    #[serde(rename = "Meta")]
    pub meta: PhysicsMeta,
    #[serde(rename = "PhysicsSettings")]
    pub physics_settings: Vec<PhysicsSetting>,
}

pub fn load_physics(path: &str) -> Physics3Json {
    let mut file = File::open(path).expect("Failed to open .physics3.json");
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    serde_json::from_str(&contents).expect("Failed to parse physics JSON")
}

use std::sync::{Arc, LazyLock, Mutex};

#[derive(Clone, Debug)]
pub struct RuntimePhysicsInput {
    pub source_id: i32, // Числовой ID параметра вместо String
    pub weight: f32,
    pub input_type: String,
    pub reflect: bool,
}

pub struct RuntimePhysicsOutput {
    pub destination_id: i32, // Числовой ID параметра вместо String
    pub vertex_index: usize,
    pub scale: f32,
    pub weight: f32,
    pub reflect: bool,
}

#[derive(Clone, Debug)]
pub struct RuntimeParticle {
    pub position: [f32; 2],
    pub velocity: [f32; 2],
}

pub struct RuntimePhysicsGroup {
    pub inputs: Vec<RuntimePhysicsInput>,
    pub outputs: Vec<RuntimePhysicsOutput>,
    pub vertices_config: Vec<PhysicsVertex>,
    pub normalization: PhysicsNormalization,
    pub particles: Vec<RuntimeParticle>,
}

pub struct PhysicsRuntimeSystem {
    pub gravity: [f32; 2],
    pub groups: Vec<RuntimePhysicsGroup>,
}

fn normalize_value(val: f32, min: f32, max: f32, def: f32) -> f32 {
    let clamped = val.clamp(min, max);
    if clamped > def {
        (clamped - def) / (max - def).max(0.0001)
    } else {
        (clamped - def) / (def - min).max(0.0001)
    }
}

impl PhysicsRuntimeSystem {
    pub fn new(json: Physics3Json) -> Self {
        let gravity = [
            json.meta.effective_forces.gravity.x,
            json.meta.effective_forces.gravity.y,
        ];
        let mut groups = Vec::new();

        for setting in json.physics_settings {
            let inputs = setting
                .inputs
                .iter()
                .map(|inp| {
                    let c_str = CString::new(inp.source.id.as_str()).unwrap();
                    let id = unsafe { crate::model_interaction::getParameterId(c_str.as_ptr()) };
                    RuntimePhysicsInput {
                        source_id: id,
                        weight: inp.weight,
                        input_type: inp.input_type.clone(),
                        reflect: inp.reflect,
                    }
                })
                .collect();

            let outputs = setting
                .outputs
                .iter()
                .map(|out| {
                    let c_str = CString::new(out.destination.id.as_str()).unwrap();
                    let id = unsafe { crate::model_interaction::getParameterId(c_str.as_ptr()) };
                    RuntimePhysicsOutput {
                        destination_id: id,
                        vertex_index: out.vertex_index,
                        scale: out.scale,
                        weight: out.weight,
                        reflect: out.reflect,
                    }
                })
                .collect();

            let mut particles = Vec::new();
            for v in &setting.vertices {
                particles.push(RuntimeParticle {
                    position: [v.position.x, v.position.y],
                    velocity: [0.0, 0.0],
                });
            }

            groups.push(RuntimePhysicsGroup {
                inputs,
                outputs,
                vertices_config: setting.vertices,
                normalization: setting.normalization,
                particles,
            });
        }

        Self { gravity, groups }
    }

    pub fn update(&mut self, current_params: &[(i32, f32)], delta_time: f32) -> Vec<(i32, f32)> {
        let dt = 1.0 / 60.0;
        let mut output_params = Vec::with_capacity(self.groups.len() * 2);

        for group in &mut self.groups {
            let mut total_input_x = 0.0f32;
            let mut total_input_angle = 0.0f32;

            for input in &group.inputs {
                if let Some((_, raw_val)) =
                    current_params.iter().find(|(id, _)| *id == input.source_id)
                {
                    let mut val = *raw_val;
                    if input.reflect {
                        val = -val;
                    }

                    let norm = &group.normalization;
                    match input.input_type.as_str() {
                        "X" | "Y" => {
                            let n = normalize_value(
                                val,
                                norm.position.minimum,
                                norm.position.maximum,
                                norm.position.default,
                            );
                            total_input_x += n * (input.weight / 100.0);
                        }
                        "Angle" => {
                            let n = normalize_value(
                                val,
                                norm.angle.minimum,
                                norm.angle.maximum,
                                norm.angle.default,
                            );
                            total_input_angle += n * (input.weight / 100.0);
                        }
                        _ => {}
                    }
                }
            }

            if group.particles.is_empty() {
                continue;
            }

            group.particles[0].position = [
                group.vertices_config[0].position.x + total_input_x + total_input_angle,
                group.vertices_config[0].position.y,
            ];

            for i in 1..group.particles.len() {
                let v_config = &group.vertices_config[i];
                let parent_pos = group.particles[i - 1].position;

                let force_x = self.gravity[0] * v_config.acceleration;
                let force_y = self.gravity[1] * v_config.acceleration;

                let p = &mut group.particles[i];
                let damping = v_config.delay.clamp(0.01, 0.95);
                let mobility = v_config.mobility.clamp(0.0, 1.0);

                p.velocity[0] = (p.velocity[0] + force_x * dt) * damping;
                p.velocity[1] = (p.velocity[1] + force_y * dt) * damping;

                p.velocity[0] = p.velocity[0].clamp(-50.0, 50.0);
                p.velocity[1] = p.velocity[1].clamp(-50.0, 50.0);

                p.position[0] += p.velocity[0] * mobility;
                p.position[1] += p.velocity[1] * mobility;

                let mut dx = p.position[0] - parent_pos[0];
                let mut dy = p.position[1] - parent_pos[1];
                let len = (dx * dx + dy * dy).sqrt();

                if len > 0.0001 {
                    let factor = v_config.radius / len;
                    p.position[0] = parent_pos[0] + dx * factor;
                    p.position[1] = parent_pos[1] + dy * factor;
                }
            }

            for out in &group.outputs {
                let idx = out.vertex_index;
                if idx == 0 || idx >= group.particles.len() {
                    continue;
                }

                let p_curr = group.particles[idx].position;
                let p_prev = group.particles[idx - 1].position;

                let dx = p_curr[0] - p_prev[0];
                let dy = p_curr[1] - p_prev[1];
                let angle = dx.atan2(dy);

                let mut result = angle * out.scale * (out.weight / 100.0);
                if out.reflect {
                    result = -result;
                }

                output_params.push((out.destination_id, result));
            }
        }

        output_params
    }
}

pub static runtime: LazyLock<Mutex<PhysicsRuntimeSystem>> = LazyLock::new(|| {
    let physics_data = load_physics("models/runtime/zundamon.physics3.json");
    Mutex::new(PhysicsRuntimeSystem::new(physics_data))
});
