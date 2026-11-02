#!/usr/bin/env python3
"""Prepare signed generator progress for exact-clock effect evaluation."""
from pathlib import Path
import argparse
p=argparse.ArgumentParser();p.add_argument('--apply',action='store_true');p.add_argument('--domain-dir',type=Path);a=p.parse_args()
root=a.domain_dir or Path(__file__).resolve().parents[1]/'backend/src/domain';path=root/'flat_surface.rs';s=path.read_text()
marker='    fn track(&mut self,index:usize,checkpoint:&mut dyn FnMut()->Result<()>)'
assert s.count(marker)==1
method='''    /// Filmcraft clip progress uses generator seconds, including signed preroll.
    fn clip_progress_seconds(&self, seconds:f64)->Option<f64>{
        if !seconds.is_finite(){return None;}
        self.clip_duration_us().map(|duration|{
            let duration=duration as f64/1_000_000.0;
            if duration<=1e-9{0.0}else{(seconds/duration).clamp(0.0,1.0)}
        })
    }
'''
s=s.replace(marker,method+marker)
start=s.index('fn apply_effect_stack_seeded_at_time(');end=s.index('\npub fn supported_effect_stack',start);part=s[start:end]
old='c.clip_progress(local)';assert part.count(old)==4
new='if matches!(parameter_time,super::animation_sample_time::AnimationSampleTime::SourceTicks(_)|super::animation_sample_time::AnimationSampleTime::FilmcraftSourceTicks(_)){c.clip_progress_seconds(generator_seconds)}else{c.clip_progress(local)}'
part=part.replace(old,new);s=s[:start]+part+s[end:]
print('4 signed progress consumers; legacy progress preserved; apply='+str(a.apply))
if a.apply:path.write_text(s)
