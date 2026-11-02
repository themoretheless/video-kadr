#!/usr/bin/env python3
"""Run the pinned original linked ripple-trim group, without substituting native conversion code."""
import argparse,json,os,subprocess,tempfile
from pathlib import Path
root=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--reference',type=Path,default=Path('/tmp/video-kadr-filmcraft-reference'));p.add_argument('--check',action='store_true');a=p.parse_args();ref=a.reference.resolve()
assert subprocess.check_output(['git','-C',str(ref),'rev-parse','HEAD'],text=True).strip()=='5231852443363f001c3f6b396dd9b1e6461ae2be'
assert not subprocess.check_output(['git','-C',str(ref),'status','--porcelain','--untracked-files=no'],text=True).strip()
with tempfile.TemporaryDirectory() as tmp:
 d=Path(tmp)
 context=(ref/'crates/engine/src/lib.rs').read_text();context=context[context.index('pub fn media_start('):context.index('/// Subclips of subclips followed')]
 commands=(ref/'crates/engine/src/commands.rs').read_text()
 sequence_tools=(ref/'crates/engine/src/sequence_tools.rs').read_text()
 def function(text,start):
  at=text.index(start);brace=text.index('{',at);depth=1;i=brace+1
  while depth:
   if text[i]=='{':depth+=1
   elif text[i]=='}':depth-=1
   i+=1
  return text[at:i]
 source=(root/'scripts/filmcraft-clipboard-oracle.rs').read_text().replace('__CONTEXT__',context)
 source=source.replace('__WITH_LINKS__',function(commands,'pub fn with_links(')).replace('__COPY_PASTE__',function(commands,'fn copy_selection(')+'\n'+function(commands,'fn paste(')).replace('__RIPPLE__',function(sequence_tools,'pub fn ripple_markers('))
 (d/'main.rs').write_text(source)
 manifest='[package]\nname="video-kadr-wav-audio-oracle"\nversion="0.1.0"\nedition="2024"\n[[bin]]\nname="oracle"\npath="main.rs"\n[dependencies]\nserde_json="1"\nanyhow="1"\n'
 manifest+=f'filmcraft-media={{path={json.dumps(str(ref/"crates/media"))}}}\n'
 manifest+=''.join(f'filmcraft-{n}={{path={json.dumps(str(ref/"crates"/n))}}}\n' for n in ['project','edit','time'])
 (d/'Cargo.toml').write_text(manifest)
 result=subprocess.check_output(['cargo','+1.98.1','run','--quiet','--manifest-path',str(d/'Cargo.toml'),'--',str(root/'backend/tests/fixtures/filmcraft-project-index.json')],env=dict(os.environ,CARGO_TARGET_DIR='/tmp/video-kadr-filmcraft-video-oracle-target'))
 parsed=json.loads(result);assert len(parsed['cases'])==768
 target=root/'backend/tests/fixtures/filmcraft-clipboard.json'
 if a.check: assert parsed==json.loads(target.read_text())
 else: target.write_bytes(result)
print('PASS: original clipboard oracle')
