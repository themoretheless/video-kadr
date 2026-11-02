// Original animated transition scenes for all duration-driven effect families.
#[cfg(test)]
mod original_progress_scene_tests {
    use super::*;
    use crate::domain::{animation_sample_time::AnimationSampleTime,linear_color_frame::LinearColorFrame,transition_spec::TransitionSpec};
    struct Context;
    impl EffectTrackContext for Context {
        fn clip_duration_us(&self)->Option<u64>{Some(10_000_000)}
        fn frame_rate(&self)->Option<f64>{Some(24.)}
        fn track(&mut self,_:usize,_:&mut dyn FnMut()->Result<()>)->Result<Option<LinearColorFrame>>{Ok(None)}
        fn layer_to_output(&self,x:f64,y:f64)->(f64,f64){(x,y)}
    }
    #[test]
    fn original_animated_duration_effect_transition_scenes() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-transition-side-animation.json")).unwrap();
        let mut checked=0;
        for case in fixture["cases"].as_array().unwrap(){
            let effect=case["effect"].as_str().unwrap();
            if !matches!(effect,"grow"|"shrink"|"spin"|"move"){continue;}
            let parameter=if effect=="move"{"motionBlur"}else{case["parameter"].as_str().unwrap()};
            let mut animation=serde_json::Map::new();
            animation.insert(parameter.into(),serde_json::json!({"mode":"keyframes","track":{"timeBase":1000000,"interpolation":"linear","keyframes":[{"tick":18000000,"value":case["low"]},{"tick":32000000,"value":case["high"]}]}}));
            let effect=serde_json::from_value(serde_json::json!({"kind":effect,"parameters":{},"animation":animation})).unwrap();
            let parse=|field:&str|case[field].as_str().unwrap().parse::<i64>().unwrap();
            let mut red=GroupSurface{width:4,height:4,rgba:[255,0,0,255].repeat(16)};
            let mut blue=GroupSurface{width:4,height:4,rgba:[0,0,255,255].repeat(16)};
            let side=if case["mode"].as_u64().unwrap()==0{&mut red}else{&mut blue};
            let rendered=apply_effect_stack_seeded_at_time(side,&[effect],0,AnimationSampleTime::FilmcraftSourceTicks(parse("effectTime")),parse("localTime")as f64/254_016_000_000.,&[],Some(&mut Context),&mut||Ok(()),None).unwrap();
            let (a,b)=if case["mode"]==0{(rendered,LinearColorFrame::from_surface(&blue,&mut||Ok(())).unwrap())}else{(LinearColorFrame::from_surface(&red,&mut||Ok(())).unwrap(),rendered)};
            let progress=(parse("at")-9*254_016_000_000)as f64/(2*254_016_000_000i64)as f64;
            let result=crate::domain::transition_procedural::render(&TransitionSpec{effect:"cross_dissolve".into(),params:Default::default()},&a,&b,progress as f32,case["reverse"].as_bool().unwrap(),&mut||Ok(())).unwrap();
            let expected:Vec<u32>=serde_json::from_value(case["bits"].clone()).unwrap();
            assert_eq!(result.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"duration effect original scene {case}");
            checked+=1;
        }
        assert_eq!(checked,144);
    }
}
