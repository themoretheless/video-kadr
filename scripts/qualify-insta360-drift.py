import hashlib,json,subprocess,os,struct
from pathlib import Path
out=Path('docs/qualification/insta360-2026-10-31/gyro-drift');out.mkdir(exist_ok=True);blob=Path('/Applications/Insta360 Studio.app/Contents/MacOS/studio-main').read_bytes();sl=16384
n=struct.unpack_from('<I',blob,sl+16)[0];pos=sl+32;starts=[]
for _ in range(n):
 cmd,size=struct.unpack_from('<II',blob,pos)
 if cmd==0x26:
  off,length=struct.unpack_from('<II',blob,pos+8);d=blob[sl+off:sl+off+length];p=0;address=0x100000000
  while p<len(d):
   value=0;shift=0
   while 1:
    b=d[p];p+=1;value|=(b&127)<<shift;shift+=7
    if b<128:break
   if value==0:break
   address+=value;starts.append(address)
 pos+=size
rows=[]
for name,a in [('stationary',0x1027cad68),('bias',0x1027cb4bc)]:
 assert a in starts;b=starts[starts.index(a)+1];data=blob[sl+a-0x100000000:sl+b-0x100000000];p=out/f'{name}.arm64.bin';p.write_bytes(data);rows.append({'name':name,'address':hex(a),'endAddress':hex(b),'sourceOffset':sl+a-0x100000000,'sha256':hashlib.sha256(data).hexdigest()})
(out/'manifest.json').write_text(json.dumps({'sourceSha256':hashlib.sha256(blob).hexdigest(),'boundary':'LC_FUNCTION_STARTS arm64 __TEXT source mapping','limitations':'Derived function images omit out-of-range calls/data; generic arm64 ABI only','functions':rows},indent=2))
env=os.environ.copy();env.update(GHIDRA_INSTALL_DIR='/Users/themoretheless/Applications/ghidra_12.1.4_PUBLIC',JAVA_HOME='/opt/homebrew/opt/openjdk/libexec/openjdk.jdk/Contents/Home',REA_GHIDRA_STARTUP_TIMEOUT_MS='120000')
for r in rows:
 with (out/f'{r["name"]}.json').open('w')as o,(out/f'{r["name"]}.stderr.log').open('w')as e:
  code=subprocess.run(['/opt/homebrew/bin/rea','decompile',str((out/f'{r["name"]}.arm64.bin').resolve()),'--target-format','raw-binary','--architecture','arm64','--entry-point',r['address'],'--procedure',r['address'],'--provider','ghidra','--format','json'],env=env,stdout=o,stderr=e,timeout=150).returncode
 print(r['name'],code,flush=True)
 if code==0:(out/f'{r["name"]}.pseudo.c').write_text(json.loads((out/f'{r["name"]}.json').read_text())['normalized_result'])
