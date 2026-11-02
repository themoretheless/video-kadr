//! Actual source audio_fx::process, including cache/seek/preroll/look-ahead/control grid.
#[path = "__AUDIO_FX_PATH__"]
mod source_fx;
use filmcraft_project::{ClipId, EffectKind, ItemId, Label, TrackItem, ParamKind, ParamValue};
use filmcraft_time::Tick;
use serde_json::json;
fn main() {
    let mut cases = Vec::new();
    let nondefault = std::env::args().any(|arg| arg == "--nondefault");
    for (index, definition) in filmcraft_project::effect_defs().iter().filter(|def| def.kind == EffectKind::Audio && !def.intrinsic).enumerate() {
        for (scenario_index, scenario) in (if nondefault { vec!["min", "mid", "max"] } else { vec!["default"] }).into_iter().enumerate() {
        let mut instance = definition.instance();
        let mut values = std::collections::BTreeMap::new();
        if nondefault {
            for parameter in &definition.params {
                let (value, scalar) = match &parameter.kind {
                    ParamKind::Float { min, max, .. } => {
                        let value = match scenario { "min" => *min, "max" => *max, _ => (*min + *max)/2.0 };
                        (ParamValue::Float(value), value)
                    }
                    ParamKind::Choice(choices) => {
                        let index = match scenario { "min" => 0, "max" => choices.len()-1, _ => choices.len()/2 };
                        (ParamValue::Choice(index as u32), index as f64)
                    }
                    ParamKind::Bool => { let value = scenario != "min"; (ParamValue::Bool(value), if value {1.0} else {0.0}) }
                    _ => panic!("unexpected audio scalar kind"),
                };
                instance.param_mut(parameter.id).unwrap().value = value;
                values.insert(parameter.id, scalar);
            }
        }
        let item = TrackItem {
            id: ClipId((index * 3 + scenario_index) as u64 + 1), item: ItemId(1), name: "oracle".into(), label: Label::Iris,
            start: Tick::ZERO, duration: Tick::from_seconds_f64(1.0), source_in: Tick::ZERO,
            speed: 1.0, reverse: false, enabled: true, link: None, group: None,
            effects: vec![instance], markers: vec![], gain_db: 0.0,
            frame_hold: None, scale_to_frame: false, essential: None, multicam: None,
            time_interpolation: Default::default(), hold_filters: false, field_options: None,
            source_channels: vec![], graphic: None,
        };
        let read = |start: i64, count: usize| -> Vec<Vec<f32>> {
            let left: Vec<f32> = (0..count).map(|i| {
                let frame = start + i as i64;
                if !(0..48000).contains(&frame) { return 0.0 }
                ((frame * 7919 % 1009) as f32 / 1009.0 - 0.5) * 0.1
            }).collect();
            let right = left.iter().map(|v| -*v * 0.75).collect();
            vec![left, right]
        };
        for (start, frames) in [(13000, 257), (13257, 255), (5000, 512)] {
            let pcm = source_fx::process(&item, start, frames, 48000, &read);
            let bits: Vec<Vec<u32>> = pcm.iter().map(|channel| channel.iter().map(|v| v.to_bits()).collect()).collect();
            let mut case = json!({"effect":definition.id,"start":start,"frames":frames,"bits":bits});
            if nondefault { case["scenario"] = json!(scenario); case["values"] = json!(values); }
            cases.push(case);
        }
    }
        }
    println!("{}", serde_json::to_string(&json!({"sourceCommit":"5231852443363f001c3f6b396dd9b1e6461ae2be","cases":cases})).unwrap());
}
