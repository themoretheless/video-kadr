#!/usr/bin/env python3
"""Extract unchanged Studio freeze timing and original Rational approximation."""
from pathlib import Path
import hashlib,json,subprocess
root=Path(__file__).resolve().parent.parent
reference=root/'backend/target/concat-reference'
commit='78e4be101c2d3c41a7dc73c112c6b1f11b9ce170'
assert subprocess.check_output(['git','-C',str(reference),'rev-parse','HEAD'],text=True).strip()==commit
paths=['src/crates/concat/src/studio.rs','src/crates/concat-core/src/time.rs','src/crates/concat-host/src/media.rs']
subprocess.run(['git','-C',str(reference),'diff','--quiet','HEAD','--',*paths],check=True)
studio=(reference/paths[0]).read_text()
body=studio[studio.index('    pub fn freeze_at_playhead('):]
body=body[:body.index('\n    pub fn ')]
line='let source_time = clip.source_start + (at - clip.start) * clip.speed;'
assert line in body and 'media::still_at(&media.path, source_time, 1280)' in body and 'jpeg(&frame, 4)' in body and '(source_time * 1000.0).round() as i64' in body
host=(reference/paths[2]).read_text()
still=host[host.index('pub fn still_at('):host.index('/// Flattens an error') ]
assert 'options.starting_at(Rational::approximate(seconds)' in still and 'decoder.seek(Rational::ZERO)' in still and 'match decoder.next_frame()' in still
out=root/'backend/target/concat-freeze-capture-oracle';out.mkdir(exist_ok=True)
program='#[path='+json.dumps(str(reference/paths[1]))+'] mod time;\nuse time::Rational;\nstruct Clip{start:f64,source_start:f64,speed:f64}\nfn original(clip:&Clip,at:f64)->f64{'+line+'source_time}\n'+r'''
fn main(){for start in [2.0f64,2.00000049]{for source_start in [1.5f64,1.0004996,0.0004996]{for speed in [1.0f64,1.75,1.23456789]{for offset in [0.125f64,1.125,2.5]{
 let clip=Clip{start,source_start,speed};let at=start+offset;let source=original(&clip,at);let seek=Rational::approximate(source).unwrap();
 let micros=seek.numerator()as i128*1_000_000/seek.denominator()as i128;
 println!("{:016x} {:016x} {:016x} {:016x} {:016x} {} {}",start.to_bits(),source_start.to_bits(),speed.to_bits(),at.to_bits(),source.to_bits(),micros,(source*1000.).round()as u64);
}}}}}
'''
(out/'main.rs').write_text(program)
subprocess.run(['rustc','+1.89.0','--edition=2021',str(out/'main.rs'),'-o',str(out/'oracle')],check=True)
data=subprocess.check_output([str(out/'oracle')]);(out/'capture.txt').write_bytes(data)
(out/'provenance.json').write_text(json.dumps({'commit':commit,'sources':{p:hashlib.sha256((reference/p).read_bytes()).hexdigest() for p in paths},'driver':hashlib.sha256(program.encode()).hexdigest(),'data':hashlib.sha256(data).hexdigest()},indent=2))
print(f'{len(data.splitlines())} original Studio capture clocks; generated data only in ignored target')
