#!/usr/bin/env python3
"""Prepare split parameter/generator clocks; preserves legacy entry and context clock."""
from pathlib import Path
import argparse,re
p=argparse.ArgumentParser();p.add_argument('--apply',action='store_true');p.add_argument('--domain-dir',type=Path);a=p.parse_args()
root=a.domain_dir or Path(__file__).resolve().parents[1]/'backend/src/domain';path=root/'flat_surface.rs';s=path.read_text()
start=s.index('fn apply_effect_stack_seeded(');end=s.index('\npub fn supported_effect_stack',start)
part=s[start:end];body=part.index('{')
signature=part[:body]
wrapper=signature+'''{
    apply_effect_stack_seeded_at_time(child,effects,local,local.into(),local as f64 / 1_000_000.0,library,context,checkpoint,seed)
}
'''
# Keep the old signature externally, concrete clocks internally to avoid generic propagation.
wrapper=wrapper.replace('mut context:Option','context:Option')
new=signature.replace('fn apply_effect_stack_seeded(', 'fn apply_effect_stack_seeded_at_time(').replace('local:u64,library:', 'local:u64,parameter_time:super::animation_sample_time::AnimationSampleTime,generator_seconds:f64,library:')+part[body:]
edits=[];count=0
for m in re.finditer(r'\b(sample\w*)\s*\(',new):
 if not (m.group(1)=='sample' or m.group(1).startswith('sample_')):continue
 opening=m.end()-1;depth=1;j=opening+1
 while depth:
  depth+=(new[j]=='(')-(new[j]==')');j+=1
 for clock in re.finditer(r'\blocal\b',new[opening+1:j-1]):
  lo=opening+1+clock.start();hi=opening+1+clock.end()
  edits.append((lo,hi,'parameter_time'));count+=1
for lo,hi,value in sorted(set(edits),reverse=True):new=new[:lo]+value+new[hi:]
# Direct generator methods: preserve original seconds path for legacy positive calls.
seconds_count=new.count('local as f64/1000000.')
new=new.replace('local as f64/1000000.','generator_seconds')
for effect in ['Noise','Strobe','WaveWarp']:
 marker='super::composition::VideoEffect::'+('NoiseLegacy' if effect=='Noise' else effect)+'{parameters,animation}=>'
 lo=new.index(marker);hi=new.find('\n',lo)
 line=new[lo:hi]
 assert line.count('p.apply_linear(frame,local,checkpoint)?')==1,effect
 line=line.replace('p.apply_linear(frame,local,checkpoint)?','p.apply_linear_at_seconds(frame,generator_seconds,checkpoint)?')
 new=new[:lo]+line+new[hi:]
assert count>100,count
s=s[:start]+wrapper+new+s[end:]+'\ninclude!("effect_stack_clock_tests.rs");\n'
print(f'{count} parameter call arguments; {seconds_count+3} direct generator seconds paths; apply={a.apply}')
if a.apply:path.write_text(s)
