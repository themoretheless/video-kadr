#!/usr/bin/env python3
"""Explicit exact-time adapters for composite, VR, keyer and text scalar sampling."""
from pathlib import Path
import argparse,re
p=argparse.ArgumentParser();p.add_argument('--apply',action='store_true');p.add_argument('--domain-dir',type=Path);a=p.parse_args()
root=a.domain_dir or Path(__file__).resolve().parents[1]/'backend/src/domain'
names='creative_grade lumetri chroma_surface vr_glow vr_digital_glitch vr_denoise vr_chromatic vr_fractal_noise vr_plane_to_sphere vr_color_gradients metadata_burnin simple_text'.split()
changes={}
for name in names:
 path=root/(name+'.rs');s=path.read_text()
 match=re.search(r'pub fn sample\s*\([^)]*?\b(local_us|local)\s*:\s*u64',s,re.S)
 assert match,name
 clock=match.group(1);start=match.end()-3;body=s.index('{',match.end())
 s=s[:body+1]+'\n        let '+clock+': super::animation_sample_time::AnimationSampleTime = '+clock+'.into();\n'+s[body+1:]
 s=s[:start]+'impl Into<super::animation_sample_time::AnimationSampleTime>'+s[start+3:]
 changes[path]=s
 print(name+':sample:'+clock)
path=root/'graphic_source_text_animation.rs'
s=path.read_text()
old="    pub fn sample(&self, local_us: u64) -> Result<&str> {\n        self.sample_signed(i128::from(local_us))\n    }"
new="""    pub fn sample(&self, time: impl Into<super::animation_sample_time::AnimationSampleTime>) -> Result<&str> {
        use super::animation_sample_time::AnimationSampleTime;
        match time.into() {
            AnimationSampleTime::Microseconds(us) => self.sample_signed(i128::from(us)),
            AnimationSampleTime::SignedMicroseconds(us) => self.sample_signed(i128::from(us)),
            AnimationSampleTime::SourceTicks(ticks) | AnimationSampleTime::FilmcraftSourceTicks(ticks) => self.sample_original_ticks(i128::from(ticks)),
        }
    }"""
assert s.count(old)==1
changes[path]=s.replace(old,new)+'\ninclude!("source_text_exact_time_tests.rs");\n'
assert len(changes)==13
if a.apply:
 for path,s in changes.items():path.write_text(s)
print('13 additional sampler adapters; apply='+str(a.apply))
