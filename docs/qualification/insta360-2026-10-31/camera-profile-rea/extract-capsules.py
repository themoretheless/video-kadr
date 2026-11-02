import struct,hashlib,json,pathlib
src=pathlib.Path('/Applications/Insta360 Studio.app/Contents/MacOS/studio-main');b=src.read_bytes();off=16384;n=struct.unpack_from('<I',b,off+16)[0];p=off+32;segs=[]
for _ in range(n):
 cmd,z=struct.unpack_from('<II',b,p)
 if cmd==25:
  va,vs,fo,fs=struct.unpack_from('<QQQQ',b,p+24);segs.append((va,va+vs,fo))
 if cmd==0x26:fs_off,fs_size=struct.unpack_from('<II',b,p+8)
 p+=z
p=off+fs_off;end=p+fs_size;addr=0x100000000;starts=[]
while p<end:
 v=0;s=0
 while True:
  c=b[p];p+=1;v|=(c&127)<<s;s+=7
  if c<128:break
 if not v:break
 addr+=v;starts.append(addr)
folder=pathlib.Path('docs/qualification/insta360-2026-10-31/camera-profile-rea');folder.mkdir(exist_ok=True);rows=[]
for a in [0x1027ae5cc,0x1027ae708,0x1027ae748,0x1027ae8d8,0x1027ae8e0,0x1027af66c,0x1027aecd0,0x1027ae9d8,0x101b566f4]:
 i=starts.index(a);end=starts[i+1];f=next(fo+a-va for va,e,fo in segs if va<=a<e);part=b[off+f:off+f+end-a];path=folder/f'{a:x}.bin';path.write_bytes(part);rows.append(dict(address=hex(a),end_address=hex(end),source_offset=off+f,bytes=len(part),sha256=hashlib.sha256(part).hexdigest(),path=str(path.resolve())))
(folder/'manifest.json').write_text(json.dumps(dict(source_path=str(src),source_sha256=hashlib.sha256(b).hexdigest(),boundary_source='LC_FUNCTION_STARTS original arm64 slice',limitations=['Function-only subranges; outside memory/calls/ABI not supplied','Selected original VA preserved; capsule Evidence binds subrange, not original app'],capsules=rows),indent=2))
print(json.dumps(rows,indent=2))
