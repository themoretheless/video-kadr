import struct,json,pathlib,hashlib
root=pathlib.Path.cwd();folder=root/'docs/qualification/insta360-2026-10-31/gyro-bridge';folder.mkdir(exist_ok=True)
source=pathlib.Path('/Applications/Insta360 Studio.app/Contents/MacOS/studio-main');b=source.read_bytes();off=16384;n=struct.unpack_from('<I',b,off+16)[0];p=off+32;segs=[];sections=[]
for _ in range(n):
 cmd,z=struct.unpack_from('<II',b,p)
 if cmd==25:
  va,vs,fo,fs=struct.unpack_from('<QQQQ',b,p+24);segs.append((va,va+vs,fo))
  for i in range(struct.unpack_from('<I',b,p+64)[0]):
   q=p+72+i*80;name=b[q:q+16].split(b'\0')[0].decode();addr,size,f=struct.unpack_from('<QQI',b,q+32);sections.append((name,addr,size,f))
 if cmd==0x26:fs_off,fs_size=struct.unpack_from('<II',b,p+8)
 p+=z
def rd(a,z):
 for va,end,fo in segs:
  if va<=a<end:return b[off+fo+a-va:off+fo+a-va+z]
p=off+fs_off;end=p+fs_size;addr=0x100000000;starts=[]
while p<end:
 v=0;s=0
 while True:
  c=b[p];p+=1;v|=(c&127)<<s;s+=7
  if c<128:break
 if not v:break
 addr+=v;starts.append(addr)
rows=[]
for a in [0x1018b9728,0x101036b60,0x1018b928c]:
 i=starts.index(a);end=starts[i+1];f=next(fo+a-va for va,e,fo in segs if va<=a<e);part=b[off+f:off+f+end-a];path=folder/f'{a:x}.arm64.bin';path.write_bytes(part);rows.append(dict(address=hex(a),end_address=hex(end),source_offset=off+f,bytes=len(part),sha256=hashlib.sha256(part).hexdigest(),path=str(path)))
registered=[]
for name,a,z,f in sections:
 if name=='__init_offsets':
  for i in range(z//4):
   raw=struct.unpack_from('<I',b,off+f+i*4)[0];ptr=0x100000000+raw
   if ptr==0x1018b9728:registered.append(dict(section=name,index=i,source_offset=off+f+i*4,raw_pointer=hex(raw),decoded_target=hex(ptr)))
 if name=='__mod_init_func':
  for i in range(z//8):
   raw=struct.unpack_from('<Q',b,off+f+i*8)[0];ptr=0x100000000+(raw&0xfffffffff)
   if ptr==0x1018b9728:registered.append(dict(section=name,index=i,source_offset=off+f+i*8,raw_pointer=hex(raw),decoded_target=hex(ptr)))
constants=rd(0x103b1d980,40);floats=list(struct.unpack('<10f',constants))+[struct.unpack('<f',struct.pack('<I',0xb33bbd2e))[0],0,0,0,0,1]
supp=dict(method='Source-bound portable Mach-O and original LLVM assembly reads; not REA Evidence',source_sha256=hashlib.sha256(b).hexdigest(),initializer_registration=registered,literal_va='0x103b1d980',literal_bytes_hex=constants.hex(),assembled_mat4_column_major=floats,global_xyzw_quaternion_va='0x104a63db0',inverse_va='0x104a63dc0',limiting_ideal_quaternion_xyzw=[0.5,-0.5,-0.5,0.5])
(folder/'source-layout.json').write_text(json.dumps(supp,indent=2));(folder/'manifest.json').write_text(json.dumps(dict(source_path=str(source),source_sha256=hashlib.sha256(b).hexdigest(),boundary_source='Original LC_FUNCTION_STARTS and original arm64 segment mapping',limitations=['Function-only subranges omit outside data, calls and Apple ABI','Static observation, not runtime global read'],capsules=rows),indent=2));print(json.dumps(supp,indent=2))
