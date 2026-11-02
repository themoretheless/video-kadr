#!/usr/bin/env python3
"""Prepare precise metadata context without altering legacy or temporal reads."""
from pathlib import Path
import argparse,re
p=argparse.ArgumentParser();p.add_argument('--apply',action='store_true');p.add_argument('--domain-dir',type=Path);a=p.parse_args()
root=a.domain_dir or Path(__file__).resolve().parents[1]/'backend/src/domain';path=root/'flat_surface.rs';s=path.read_text()
marker="struct CompositionEffectContext<'a> {"
assert s.count(marker)==1
s=s.replace(marker,marker+'\n exact_sample:Option<EffectContextSample>,')
s,count=re.subn(r'(CompositionEffectContext\s*\{)',r'\1exact_sample:None,',s)
assert count>=8,count
old=' fn burnin_values(&self,local_us:u64)->Result<Option<super::metadata_burnin::BurninValues>>{self.burnin_at(self.time_us,local_us)}'
new=''' fn burnin_values(&self,local_us:u64)->Result<Option<super::metadata_burnin::BurninValues>>{
  if let Some(sample)=self.exact_sample {
   let (clip_id,source_id)=match self.temporal_clip.as_ref(){
    Some(TemporalClip::Video(clip))=>(&clip.id,&clip.source_id),
    Some(TemporalClip::Image(clip))=>(&clip.id,&clip.source_id),
    None=>return Ok(None),
   };
   return Ok(Some(super::metadata_burnin::BurninValues::from_composition_at_ticks(self.composition,clip_id,source_id,sample.timeline_tick,sample.clocks.frame,sample.clocks.generator as f64/254_016_000_000.0)?));
  }
  self.burnin_at(self.time_us,local_us)
 }'''
assert s.count(old)==1;s=s.replace(old,new)
s+='\ninclude!("effect_context_sample.rs");\n'
print(f'{count} legacy context initializers preserved; precise metadata branch prepared; apply={a.apply}')
if a.apply:
 path.write_text(s)
 for other in root.glob('*.rs'):
  if other==path:continue
  source=other.read_text()
  source=re.sub(r'(CompositionEffectContext\s*\{)(?!\s*exact_sample:)',r'\1exact_sample:None,',source)
  if source!=other.read_text():other.write_text(source)
