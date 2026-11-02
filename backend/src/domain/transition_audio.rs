//! Exact independent transition audio block replacement, Filmcraft audio.rs (MIT).
use super::composition::{ClipTransition,CompositionClipId};
use anyhow::{Result,ensure,Context};

/// Provider returns clip-bounded, already processed planar PCM, irrespective of enabled flag.
/// Missing sides are silence; callers retain ordinary track mixing outside each interval.
pub fn replace_transition_blocks(
 output:&mut [Vec<f32>],start_sample:i64,sample_rate:u32,transitions:&[ClipTransition],
 provider:&mut dyn FnMut(&CompositionClipId,i64,usize)->Result<Vec<Vec<f32>>>,
 checkpoint:&mut dyn FnMut()->Result<()>,
)->Result<()>{
 ensure!((1..=192000).contains(&sample_rate)&&!output.is_empty()&&output.len()<=16,"invalid transition audio layout");
 let frames=output[0].len();let channels=output.len();
 ensure!(frames<=65536&&output.iter().all(|channel|channel.len()==frames&&channel.iter().all(|v|v.is_finite())),"invalid transition audio block");
 let block_end=start_sample.checked_add(i64::try_from(frames)?).context("audio sample overflow")?;
 let clock=|ticks:i128|->Result<i64>{Ok(i64::try_from((ticks*i128::from(sample_rate)).div_euclid(254016000000))?)};
 let mut plans=Vec::new();
 for transition in transitions{
  let timeline=transition.timeline.context("legacy transition needs separate audio plan")?;
  let(start,duration)=timeline.source_span().map_err(|_|anyhow::anyhow!("invalid audio transition interval"))?;
  let begin=clock(start as i128)?;let end=clock(start as i128+duration as i128)?;
  let s0=begin.max(start_sample);let s1=end.min(block_end);
  if s1>s0{plans.push((transition,begin,s0,s1,clock(duration as i128)?.max(1)));}
 }
 let mut mixed=output.to_vec();
 for(transition,begin,s0,s1,duration)in plans{
  checkpoint()?;
  let n=usize::try_from(s1-s0)?;let offset=usize::try_from(s0-start_sample)?;
  let mut side=|id:Option<&CompositionClipId>|->Result<Vec<Vec<f32>>>{
   let pcm=if let Some(id)=id{provider(id,s0,n)?}else{vec![vec![0.;n];channels]};
   ensure!(pcm.len()==channels&&pcm.iter().all(|channel|channel.len()==n&&channel.iter().all(|v|v.is_finite())),"invalid transition side PCM");Ok(pcm)
  };
  let a=side(transition.from_clip_id.as_ref())?;let b=side(transition.to_clip_id.as_ref())?;
  let base=(s0-begin)as f32;let duration=duration as f32;
  for i in 0..n{
   if i%1024==0{checkpoint()?;}
   let p=(base+i as f32)/duration;
   let(ga,gb)=super::audio_transition::AudioTransitionCurve::ConstantPower.gains(p)?;
   for channel in 0..channels{
    let value=a[channel][i]*ga+b[channel][i]*gb;
    ensure!(value.is_finite(),"transition audio mix overflow");mixed[channel][offset+i]=value;
   }
  }
 }
 for(out,mixed)in output.iter_mut().zip(mixed){*out=mixed;}
 Ok(())
}

#[cfg(test)]mod tests{
 use super::*;
 fn run_fixture(text:&str,count:usize){
  let oracle:serde_json::Value=serde_json::from_str(text).unwrap();
  assert_eq!(oracle["commit"],"5231852443363f001c3f6b396dd9b1e6461ae2be");let cases=oracle["cases"].as_array().unwrap();assert_eq!(cases.len(),count);
  for case in cases{
   let mode=case["mode"].as_u64().unwrap();let start=case["startSample"].as_i64().unwrap();
   let decode=|key:&str|->Vec<Vec<f32>>{serde_json::from_value::<Vec<Vec<u32>>>(case[key].clone()).unwrap().into_iter().map(|channel|channel.into_iter().map(f32::from_bits).collect()).collect()};
   let(a,b)=(decode("a"),decode("b"));
   let mut actual:Vec<Vec<f32>>=a.iter().zip(&b).map(|(a,b)|a.iter().zip(b).map(|(a,b)|(if mode==3{0.}else{*a})+(if mode==4{0.}else{*b})).collect()).collect();
   let mut transitions=vec![serde_json::json!({"id":"first","fromClipId":if mode==1{None}else{Some("a")},"toClipId":if mode==2{None}else{Some("b")},"timeline":{"startTick":case.get("startTick").cloned().unwrap_or(serde_json::json!("2286144000000")),"durationTick":"508032000000"},"kind":"dissolve","reverse":case["reverse"]})];
   if mode==5{let mut next=transitions[0].clone();next["id"]=serde_json::json!("second");next["fromClipId"]=serde_json::Value::Null;transitions.push(next);}
   let transitions:Vec<ClipTransition>=serde_json::from_value(serde_json::Value::Array(transitions)).unwrap();
   let mut provider=|id:&CompositionClipId,first:i64,n:usize|{let pcm=if id.as_str()=="a"{&a}else{&b};let offset=usize::try_from(first-start).unwrap();Ok(pcm.iter().map(|channel|channel[offset..offset+n].to_vec()).collect())};
   replace_transition_blocks(&mut actual,start,48000,&transitions,&mut provider,&mut||Ok(())).unwrap();
   let bits:Vec<Vec<u32>>=actual.iter().map(|channel|channel.iter().map(|v|v.to_bits()).collect()).collect();
   assert_eq!(bits,serde_json::from_value::<Vec<Vec<u32>>>(case["bits"].clone()).unwrap(),"{case}");
  }
 }
 #[test]fn original_transition_audio_84_blocks(){run_fixture(include_str!("../../tests/fixtures/filmcraft-transition-audio.json"),84)}
 #[test]fn original_transition_audio_420_fractional_sample_blocks(){run_fixture(include_str!("../../tests/fixtures/filmcraft-transition-audio-sample-clock.json"),420)}
 #[test]fn transition_audio_failure_is_atomic(){
  let transition:ClipTransition=serde_json::from_value(serde_json::json!({"id":"fade","fromClipId":"a","toClipId":null,"timeline":{"startTick":"0","durationTick":"254016000000"},"kind":"dissolve"})).unwrap();
  let original=vec![vec![0.25;16],vec![-0.25;16]];
  let mut actual=original.clone();
  assert!(replace_transition_blocks(&mut actual,0,48000,&[transition.clone()],&mut|_,_,_|Ok(vec![vec![0.;15]]),&mut||Ok(())).is_err());
  assert_eq!(actual,original);
  assert!(replace_transition_blocks(&mut actual,0,48000,&[transition],&mut|_,_,n|Ok(vec![vec![0.;n];2]),&mut||anyhow::bail!("cancelled")).is_err());
  assert_eq!(actual,original);
 }

}
