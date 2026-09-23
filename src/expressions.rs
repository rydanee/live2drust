use std::{fs::File, io::BufReader};

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone)]
pub struct ExpressionMeta {
    Type: String,
    #[serde(rename = "FadeInTime")]
    fadein: f32,
    #[serde(rename = "FadeOutTime")]
    fadeout: f32,
    #[serde(rename = "Parameters")]
    params: Vec<ExpressParam>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ExpressParam {
    id: String,
    value: f32,
    blend: String,
}

pub fn load_expression(path: &str) -> ExpressionMeta {
    let file = File::open(path).unwrap();
    let reader = BufReader::new(file);

    let expression: ExpressionMeta = serde_json::from_reader(reader).unwrap();

    println!("Loaded expression {}!", path);

    expression
}
