#!/usr/bin/env python3
"""Source-bound selected optical-flow observations; no vendor implementation copied into product."""
import hashlib,json,os,struct,subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[1]
out=root/'docs/qualification/insta360-2026-10-31/optical-flow-rea';out.mkdir(exist_ok=True)
source=Path('/Applications/Insta360 Studio.app/Contents/MacOS/studio-main');blob=source.read_bytes();sl=16384
n=struct.unpack_from('<I',blob,sl+16)[0];pos=sl+32;starts=[]
for _ in range(n):
 cmd,size=struct.unpack_from('<II',blob,pos)
 if cmd==0x26:
  off,length=struct.unpack_from('<II',blob,pos+8);data=blob[sl+off:sl+off+length];i=0;address=0x100000000
  while i<len(data):
   value=0;shift=0
   while True:
    b=data[i];i+=1;value|=(b&127)<<shift;shift+=7
    if b<128:break
   if value==0:break
   address+=value;starts.append(address)
 pos+=size
rows=[]
for name,address in [('init',0x10274d0a8),('calc',0x10274dcb0),('temporal',0x102751654),('lowtexture',0x1027516a4)]:
 assert address in starts;end=starts[starts.index(address)+1];offset=sl+address-0x100000000;payload=blob[offset:sl+end-0x100000000];(out/f'{name}.arm64.bin').write_bytes(payload)
 rows.append(dict(name=name,address=hex(address),endAddress=hex(end),sourceOffset=offset,sha256=hashlib.sha256(payload).hexdigest()))
(out/'manifest.json').write_text(json.dumps(dict(sourceSha256=hashlib.sha256(blob).hexdigest(),boundary='LC_FUNCTION_STARTS arm64 __TEXT mapping',limitations='Derived function capsules omit outside data/calls and use generic arm64 ABI; selected branches do not establish actual clip execution.',functions=rows),indent=2))
env=os.environ.copy();env.update(GHIDRA_INSTALL_DIR='/Users/themoretheless/Applications/ghidra_12.1.4_PUBLIC',JAVA_HOME='/opt/homebrew/opt/openjdk/libexec/openjdk.jdk/Contents/Home',REA_GHIDRA_STARTUP_TIMEOUT_MS='120000',REA_GHIDRA_DISABLED_ANALYZERS='Decompiler Switch Analysis,Basic Constant Reference Analyzer,Objective-C Message Analyzer')
results=[]
for row in rows:
 name=row['name']
 with (out/f'{name}.json').open('w')as stdout,(out/f'{name}.stderr.log').open('w')as stderr:
  code=subprocess.run(['/opt/homebrew/bin/rea','decompile',str(out/f'{name}.arm64.bin'),'--target-format','raw-binary','--architecture','arm64','--entry-point',row['address'],'--procedure',row['address'],'--provider','ghidra','--format','json'],env=env,stdout=stdout,stderr=stderr,timeout=150).returncode
 result=json.loads((out/f'{name}.json').read_text());success=code==0 and bool(result.get('normalized_result'))
 if success:(out/f'{name}.pseudo.c').write_text(result['normalized_result'])
 results.append(dict(name=name,exitCode=code,success=success,evidenceId=result.get('evidence_id')));(out/'results.json').write_text(json.dumps(results,indent=2));print(results[-1],flush=True)
