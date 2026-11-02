#!/usr/bin/env python3
"""Run pinned Concat Rational/FrameRate and unchanged export quantise.

Writes only scalar records and the standalone program under ignored target/.
"""
from pathlib import Path
import subprocess
root = Path(__file__).resolve().parent.parent
reference = root / 'backend/target/concat-reference/src/crates'
source = (reference / 'concat-export/src/resolve.rs').read_text()
start = source.index('pub(crate) fn quantise(')
end = source.index('\n}', start) + 2
quantise = source[start:end]
out = root / 'backend/target/concat-frame-clock-oracle'
out.mkdir(exist_ok=True)
program = '#[path = ' + repr(str(reference / 'concat-core/src/time.rs')).replace("'", '"') + '] mod time;\nuse time::{FrameRate,Rational};\n' + quantise + r'''
fn main(){for (num,den) in [(30000,1001),(24000,1001),(60000,1001)]{
 let rate=FrameRate::checked(num,den).unwrap();
 for value in [0.325324,0.325325,0.325326,0.650649,0.65065,0.650651,1.151149,1.15115,1.151151,1.451449,1.45145,1.451451,2.452449,2.45245,2.452451,2.752749,2.75275,2.752751]{
  let stamp=quantise(value,rate);let ticks=stamp.numerator()as i128*254016000000i128/stamp.denominator()as i128;
  let end=quantise(value+value,rate);let end_ticks=end.numerator()as i128*254016000000i128/end.denominator()as i128;
  let duration=quantise(2.,rate);let duration_ticks=duration.numerator()as i128*254016000000i128/duration.denominator()as i128;
  println!("{} {} {} {} {} {}",num,den,value,ticks,end_ticks,duration_ticks);
 }
}}
'''
path = out / 'main.rs'
path.write_text(program)
subprocess.run(['rustc', '+1.89.0', '--edition=2021', str(path), '-o', str(out / 'oracle')], check=True)
result = subprocess.run([str(out / 'oracle')], check=True, capture_output=True, text=True)
(out / 'frames.txt').write_text(result.stdout)
print(result.stdout, end='')
