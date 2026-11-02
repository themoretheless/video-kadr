//! Actual pinned render/audio_fx.rs configured() parameter-target oracle.
#[path = "__AUDIO_FX_PATH__"]
mod source_fx;
use filmcraft_project::{EffectKind, ParamKind, ParamValue};
use filmcraft_time::Tick;
use serde_json::json;
use std::collections::BTreeMap;
fn main() {
    let mut cases = Vec::new();
    for definition in filmcraft_project::effect_defs().iter().filter(|def| def.kind == EffectKind::Audio && !def.intrinsic) {
        for scenario in ["default", "min", "mid", "max"] {
            let mut instance = definition.instance();
            let mut values = BTreeMap::new();
            if scenario != "default" {
                for parameter in &definition.params {
                    let (value, scalar) = match &parameter.kind {
                        ParamKind::Float { min, max, .. } => {
                            let value = match scenario { "min" => *min, "max" => *max, _ => (*min + *max) / 2.0 };
                            (ParamValue::Float(value), value)
                        }
                        ParamKind::Choice(choices) => {
                            let index = if scenario == "min" { 0 } else if scenario == "max" { choices.len()-1 } else { choices.len()/2 };
                            (ParamValue::Choice(index as u32), index as f64)
                        }
                        ParamKind::Bool => { let value = scenario != "min"; (ParamValue::Bool(value), if value { 1.0 } else { 0.0 }) }
                        _ => panic!("unexpected audio scalar kind"),
                    };
                    instance.param_mut(parameter.id).unwrap().value = value;
                    values.insert(parameter.id, scalar);
                }
            }
            let native = source_fx::configured(&instance, Tick::ZERO, 48000).unwrap();
            let targets: BTreeMap<_, _> = native.params().iter().map(|p| (p.id, native.param(p.id).unwrap())).collect();
            cases.push(json!({"effect":definition.id,"scenario":scenario,"values":values,"dsp":native.id(),"targets":targets,"prerollSeconds":(source_fx::mapping(definition.id).unwrap().preroll)(&instance, Tick::ZERO)}));
        }
    }
    println!("{}", serde_json::to_string_pretty(&json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases})).unwrap());
}
