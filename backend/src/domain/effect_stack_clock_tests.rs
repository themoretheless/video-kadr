// Included in flat_surface after split parameter/generator dispatch is registered.
#[cfg(test)]
mod split_clock_tests {
    use super::*;
    use crate::domain::{animation_sample_time::AnimationSampleTime, linear_color_frame::LinearColorFrame};
    #[test]
    fn original_signed_generators_through_shared_effect_stack() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-signed-generators.json")).unwrap();
        let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),560);
        for (index,case) in cases.iter().enumerate() {
            let mut source=GroupSurface{width:case["width"].as_u64().unwrap()as usize,height:case["height"].as_u64().unwrap()as usize,rgba:serde_json::from_value(case["rgba"].clone()).unwrap()};
            let effect=serde_json::from_value(serde_json::json!({"kind":case["effect"],"parameters":case["parameters"]})).unwrap();
            let frame=apply_effect_stack_seeded_at_time(&mut source,&[effect],0,AnimationSampleTime::FilmcraftSourceTicks(20*254_016_000_000),case["localTick"].as_i64().unwrap()as f64/1_000_000.,&[],None,&mut||Ok(()),None).unwrap();
            let expected:Vec<u32>=serde_json::from_value(case["bits"].clone()).unwrap();
            assert_eq!(frame.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected,"shared stack signed generator {index}");
        }
    }
    #[test]
    fn original_parameter_easing_and_negative_generator_clock_are_independent() {
        let fixture:serde_json::Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-exact-parameter-time.json")).unwrap();
        let cases=fixture["cases"].as_array().unwrap();assert_eq!(cases.len(),378);
        for (index,case) in cases.iter().enumerate() {
            let origin=case["origin"].as_str().unwrap().parse::<i64>().unwrap()/254_016;
            let at=case["at"].as_str().unwrap().parse::<i64>().unwrap();
            let effect=serde_json::from_value(serde_json::json!({"kind":"noise","parameters":{"color":false,"clip":false,"grainSize":1.0},"animation":{"amount":{"mode":"keyframes","track":{"timeBase":1000000,"interpolation":case["nativeInterpolation"],"keyframes":[{"tick":origin,"value":5.0},{"tick":origin+1000000,"value":10.0}]}}}})).unwrap();
            let mut source=GroupSurface{width:2,height:1,rgba:vec![64,128,192,128,200,100,50,255]};
            let mut expected=LinearColorFrame::from_surface(&source,&mut||Ok(())).unwrap();
            let mut parameters=crate::domain::noise::Noise::default();
            parameters.amount=f64::from_bits(case["bits"].as_u64().unwrap())as f32;
            parameters.color=false;parameters.clip=false;parameters.grain_size=1.0;
            parameters.apply_linear_at_seconds(&mut expected,-0.500001,&mut||Ok(())).unwrap();
            let actual=apply_effect_stack_seeded_at_time(&mut source,&[effect],0,AnimationSampleTime::FilmcraftSourceTicks(at),-0.500001,&[],None,&mut||Ok(()),None).unwrap();
            assert_eq!(actual.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),expected.pixels().iter().flatten().map(|v|v.to_bits()).collect::<Vec<_>>(),"split clock source parameter {index}");
        }
    }
}
