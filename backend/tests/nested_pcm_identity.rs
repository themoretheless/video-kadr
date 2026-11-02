use std::sync::atomic::AtomicBool;
use video_kadr_backend::domain::{nested_audio::NestedAudioLayer,nested_pcm::{mix_block,mix_block_with_video_transitions}};
fn layer(gain:f64)->NestedAudioLayer{
    let clip=serde_json::from_value(serde_json::json!({"id":"clip","sourceId":"pcm","placement":{"timelineStartTick":0,"sourceInTick":0,"sourceOutTick":100000,"speed":1.0},"gain":{"mode":"constant","value":gain},"pan":{"mode":"constant","value":0.0},"fadeInTicks":0,"fadeOutTicks":0})).unwrap();
    NestedAudioLayer{concat_retime_owner:None,concat_retime_piece:None,gain_track:None,export_fade_clock:false,video_transitions:None,composition:"root".into(),instance_path:vec![],root_range_us:0..100000,clip_local_start_us:0,clip,crossfade:Default::default()}
}
#[test]
fn identity_pcm_routing_preserves_each_channel_zero_sign_and_subnormals(){
    let cancel=AtomicBool::new(false);
    for input in [[-0.,-0.],[-0.,0.25],[0.25,-0.],[0.,-0.],[f64::from_bits(1),-f64::from_bits(1)]]{
        for bridge in [false,true]{
            let actual=if bridge{mix_block_with_video_transitions(&[layer(1.)],0,1,48000,&cancel,|_,_|Ok((2,input)))}else{mix_block(&[layer(1.)],0,1,48000,&cancel,|_,_|Ok((2,input)))}.unwrap();
            assert_eq!(actual[0].map(f64::to_bits),input.map(f64::to_bits));
        }
    }
}
#[test]
fn zero_contributions_follow_ieee_addition_while_muted_layers_stay_silent(){
    let cancel=AtomicBool::new(false);
    let actual=mix_block(&[layer(0.5),layer(0.5)],0,1,48000,&cancel,|_,_|Ok((2,[-0.,-0.]))).unwrap();assert_eq!(actual[0].map(f64::to_bits),[-0f64,-0.].map(f64::to_bits));
    let mut index=0;let actual=mix_block(&[layer(1.),layer(1.)],0,1,48000,&cancel,|_,_|{index+=1;Ok((2,if index==1{[-0.,-0.]}else{[0.,0.]}))}).unwrap();assert_eq!(actual[0].map(f64::to_bits),[0f64,0.].map(f64::to_bits));
    let actual=mix_block(&[layer(0.)],0,1,48000,&cancel,|_,_|panic!("muted input must not be read")).unwrap();assert_eq!(actual[0].map(f64::to_bits),[0f64,0.].map(f64::to_bits));
}
#[test]
fn identity_routing_keeps_ordered_nonzero_sum_unclipped(){
    let cancel=AtomicBool::new(false);let mut index=0;
    let inputs=[[1.25,-0.75],[-0.5,1.5],[0.125,-0.25]];
    let actual=mix_block(&[layer(1.),layer(1.),layer(1.)],0,1,48000,&cancel,|_,_|{let v=inputs[index];index+=1;Ok((2,v))}).unwrap();assert_eq!(actual,[[0.875,0.5]]);
}
