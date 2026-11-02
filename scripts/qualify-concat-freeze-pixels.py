#!/usr/bin/env python3
"""Build unchanged host still_at + original linked decoder/JPEG as a pixel oracle."""
from pathlib import Path
import hashlib,json,subprocess
root=Path(__file__).resolve().parent.parent
ref=root/'backend/target/concat-reference'
commit='78e4be101c2d3c41a7dc73c112c6b1f11b9ce170'
assert subprocess.check_output(['git','-C',str(ref),'rev-parse','HEAD'],text=True).strip()==commit
paths=['src/crates/concat-host/src/media.rs','src/crates/concat-media/src','src/crates/concat-core/src']
subprocess.run(['git','-C',str(ref),'diff','--quiet','HEAD','--',*paths],check=True)
host=(ref/paths[0]).read_text()
def item(needle):
 start=host.index(needle);brace=host.index('{',start);level=1;end=brace+1
 while level:
  if host[end]=='{':level+=1
  if host[end]=='}':level-=1
  end+=1
 return host[start:end]
out=root/'backend/target/concat-freeze-pixel-oracle';out.mkdir(exist_ok=True)
program='use concat_core::{frame::Frame,time::Rational};\nuse concat_media::{Decoder,DecodeOptions,FrameSource,SeekableSource};\n'+item('pub fn still_at(')+'\n'+item('pub fn describe(')+r'''
fn main(){
 concat_media::set_hardware_decode(false);
 let args:Vec<String>=std::env::args().collect();let source=&args[1];let us:u64=args[2].parse().unwrap();
 let frame=still_at(source,us as f64/1_000_000.,1280).unwrap();
 std::fs::write(&args[3],concat_media::jpeg(&frame,4).unwrap()).unwrap();
 std::fs::write(&args[4],frame.pixels()).unwrap();
}
'''
(out/'main.rs').write_text(program)
(out/'Cargo.toml').write_text('''[package]
name="video-kadr-concat-freeze-pixels"
version="0.1.0"
edition="2024"
[workspace]
[[bin]]
name="concat-freeze-pixels"
path="main.rs"
[dependencies]
concat-core={path="../concat-reference/src/crates/concat-core"}
concat-media={path="../concat-reference/src/crates/concat-media"}
''')
files=[ref/paths[0],*sorted((ref/paths[1]).rglob('*.rs')),*sorted((ref/paths[2]).rglob('*.rs'))]
(out/'provenance.json').write_text(json.dumps({'commit':commit,'sources':{str(p.relative_to(ref)):hashlib.sha256(p.read_bytes()).hexdigest() for p in files},'driver':hashlib.sha256(program.encode()).hexdigest()},indent=2))
subprocess.run(['cargo','+1.97.1','build','--manifest-path',str(out/'Cargo.toml'),'--target-dir',str(root/'backend/target/concat-audio-waveform-build')],check=True)
binary=root/'backend/target/concat-audio-waveform-build/debug/concat-freeze-pixels'
proof=json.loads((out/'provenance.json').read_text())
proof['cargoLock']=hashlib.sha256((out/'Cargo.lock').read_bytes()).hexdigest()
proof['binary']=hashlib.sha256(binary.read_bytes()).hexdigest()
proof['ffmpegVersion']=subprocess.check_output(['/opt/homebrew/bin/ffmpeg','-version'],text=True).splitlines()[0]
(out/'provenance.json').write_text(json.dumps(proof,indent=2))
print(binary)
