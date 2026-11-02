#!/usr/bin/env python3
"""Original pinned Concat chain compiler plus real FFmpeg floating PCM, qualification only."""
import argparse, gzip, hashlib, json, os, shutil, struct, subprocess, tempfile
from pathlib import Path
PIN='78e4be101c2d3c41a7dc73c112c6b1f11b9ce170'
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--family',choices=['echo','reflections','stadium','distorted','tones','voices','crunch','robot','enhance','denoise','pitch'],default='echo');p.add_argument('--reference',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--target',type=Path,required=True);a=p.parse_args()
    repo=Path(__file__).resolve().parent.parent;ref=a.reference.resolve();out=a.output.resolve();out.mkdir(parents=True,exist_ok=True)
    if repo.joinpath('backend/tests').resolve() in out.parents:raise SystemExit('generate outside registered tests')
    assert subprocess.check_output(['git','-C',str(ref),'rev-parse','HEAD'],text=True).strip()==PIN
    family=a.family
    tracked=['src/Cargo.lock','src/crates/concat-effects/src/catalogue.rs','src/crates/concat-effects/packages/concat.echo/effect.toml','src/crates/concat-effects/packages/concat.echo/fixtures.toml']
    assert not subprocess.check_output(['git','-C',str(ref),'status','--porcelain','--','src/crates/concat-effects','src/crates/concat-project','src/crates/concat-core','src/Cargo.toml','src/Cargo.lock'])
    if family=='reflections':
        tracked += [f'src/crates/concat-effects/packages/concat.{name}/{file}' for name in ['room','hall','plate','cathedral'] for file in ['effect.toml','fixtures.toml']]
    if family=='stadium':
        tracked += [f'src/crates/concat-effects/packages/concat.{name}/{file}' for name in ['stadium','room','cathedral'] for file in ['effect.toml','fixtures.toml']]
    if family=='distorted':
        tracked += [f'src/crates/concat-effects/packages/concat.distorted/{file}' for file in ['effect.toml','fixtures.toml']]
    if family=='tones':
        tracked += [f'src/crates/concat-effects/packages/concat.{name}/{file}' for name in ['bass','treble','bright','distorted'] for file in ['effect.toml','fixtures.toml']]
    if family=='voices':
        tracked += [f'src/crates/concat-effects/packages/concat.{name}/{file}' for name in ['underwater','telephone','megaphone','vinyl','whisper','bass','distorted'] for file in ['effect.toml','fixtures.toml']]
    if family=='crunch':
        tracked += [f'src/crates/concat-effects/packages/concat.{name}/{file}' for name in ['radio','lofi','distorted','telephone'] for file in ['effect.toml','fixtures.toml']]
    if family=='robot':
        tracked += [f'src/crates/concat-effects/packages/concat.{name}/{file}' for name in ['robot','bass'] for file in ['effect.toml','fixtures.toml']]
    if family=='enhance':
        tracked += [f'src/crates/concat-effects/packages/concat.{name}/{file}' for name in ['enhance-voice','bass','distorted'] for file in ['effect.toml','fixtures.toml']]
    if family=='denoise':
        tracked += [f'src/crates/concat-effects/packages/concat.{name}/{file}' for name in ['denoise','enhance-voice','bass','distorted'] for file in ['effect.toml','fixtures.toml']]
    if family=='pitch':
        tracked += [f'src/crates/concat-effects/packages/concat.pitch/{file}' for file in ['effect.toml','fixtures.toml']]
    hashes={f:hashlib.sha256((ref/f).read_bytes()).hexdigest()for f in tracked};adapter=repo/f'scripts/reference/concat_audio_{family}.rs'
    with tempfile.TemporaryDirectory(prefix='concat-audio-echo-')as scratch:
        root=Path(scratch);(root/'src').mkdir();shutil.copyfile(adapter,root/'src/main.rs');shutil.copyfile(ref/'src/Cargo.lock',root/'Cargo.lock')
        deps='\n'.join(f'{name}={{path={json.dumps(str(ref/"src/crates"/name))}}}'for name in ['concat-effects','concat-project'])
        (root/'Cargo.toml').write_text('[package]\nname="video-kadr-concat-audio-oracle"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[dependencies]\n'+deps+'\nserde_json="1"\n')
        env=os.environ.copy();env.update(CARGO_TARGET_DIR=str(a.target.resolve()),CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0')
        plans=out/'plans.json';subprocess.run(['cargo','+1.98.1','run','--manifest-path',str(root/'Cargo.toml'),'--',str(plans)],env=env,check=True)
        data=json.loads(plans.read_text());cases=[];offset=0
        ffmpeg=shutil.which('ffmpeg');version=subprocess.check_output([ffmpeg,'-version'],text=True).splitlines()[0]
        with (out/f'{family}-pcm.f32le.gz').open('wb')as raw, gzip.GzipFile(filename='',mode='wb',fileobj=raw,mtime=0)as pcm:
            for rate in ([8000,13000,13001,13100,16000,44100,48000,192000] if family=='stadium' else [8000,12000,12001,18000,18001,44100,48000,192000] if family=='tones' else [8000,10000,10001,15000,15001,44100,48000,192000] if family=='voices' else [8000,44100,48000,192000] if family in ['crunch','robot','enhance','denoise','pitch'] else [8000,44100,48000]):
                frames=rate+rate//8+17
                for channels in [1,2]:
                    for plan_index,plan in enumerate(data['cases']):
                        if family=='stadium' and rate in [13001,192000] and plan_index not in [0,5]:continue
                        if family=='tones':
                            settings=plan['settings'];single=len(settings)==1;id=settings[0]['id'];gain=settings[0]['params'].get('gain')
                            if rate in [12000,12001] and not(single and id=='concat.bright'):continue
                            if rate in [18000,18001] and not(single and id=='concat.treble'):continue
                            if rate==192000 and not(single and id in ['concat.bass','concat.treble','concat.bright'] and gain in [4 if id=='concat.treble' else 5,12]):continue
                        if family=='voices':
                            settings=plan['settings'];single=len(settings)==1;id=settings[0]['id'];value=next(iter(settings[0]['params'].values()))
                            if rate in [10000,10001] and not(single and id=='concat.vinyl'):continue
                            if rate in [15000,15001] and not(single and id=='concat.whisper'):continue
                            if rate==192000 and not(single and id in ['concat.underwater','concat.telephone','concat.megaphone','concat.vinyl','concat.whisper'] and value in [3 if id=='concat.telephone' else 5,10]):continue
                        if family=='crunch' and rate==192000:
                            settings=plan['settings'];single=len(settings)==1;id=settings[0]['id'];value=next(iter(settings[0]['params'].values()))
                            if not(single and ((id=='concat.radio' and value in [0,30,100]) or (id=='concat.lofi' and value in [4,8,12]))):continue
                        signal=plan.get('signal','noise');values=[]
                        for i in range(frames):
                            for ch in range(channels):
                                if signal=='steps':
                                    v=(2.0 if ch==0 else -1.5)*(1 if (i*8//rate)%4<2 else -1)
                                else:
                                    h=((i%257+ch*1061)*1664525+1013904223)&0xffffffff
                                    v=((h>>12)%2049-1024)/4096
                                    if i in [0,rate//7,frames-1]:v=[1.25,-0.75][ch]
                                values.append(v)
                        source=struct.pack('<'+'f'*len(values),*values);source_path=root/'input.f32le';source_path.write_bytes(source)
                        command=[ffmpeg,'-v','error','-nostdin','-f','f32le','-ar',str(rate),'-ac',str(channels),'-i',str(source_path),'-af',plan['chain'],'-c:a','pcm_f32le','-f','f32le','pipe:1']
                        result=subprocess.run(command,check=True,stdout=subprocess.PIPE).stdout
                        assert len(result)%(channels*4)==0
                        cases.append({**plan,**({'inputPacketFrames':1<<((rate//10).bit_length()-1),'inputPacketProfile':'ffmpeg-pcm-10fps-pow2'} if family=='pitch' else {}),'rate':rate,'outputRate':48000 if family=='pitch' else rate,'channels':channels,'inputFrames':frames,'inputSha256':hashlib.sha256(source).hexdigest(),'outputFrames':len(result)//(channels*4),'offsetBytes':offset,'byteLength':len(result),'outputSha256':hashlib.sha256(result).hexdigest()});pcm.write(result);offset+=len(result)
    assert not subprocess.check_output(['git','-C',str(ref),'status','--porcelain','--','src/crates/concat-effects','src/crates/concat-project','src/crates/concat-core','src/Cargo.toml','src/Cargo.lock'])
    assert hashes=={f:hashlib.sha256((ref/f).read_bytes()).hexdigest()for f in tracked}
    manifest={'schemaVersion':1,'package':f'concat.{family}','reference':{'repository':'https://github.com/jub0t/concat','commit':PIN,'files':hashes,'adapterSha256':hashlib.sha256(adapter.read_bytes()).hexdigest()},'ffmpeg':{'version':version,'binarySha256':hashlib.sha256(Path(ffmpeg).read_bytes()).hexdigest()},'aliasVerified':data['aliasVerified'],'pcmBytes':offset,'cases':cases}
    (out/f'concat-{family}-pcm.json').write_text(json.dumps(manifest,indent=2,allow_nan=False)+'\n')
    print(f'Original {family}: {len(cases)} cases, {offset} PCM bytes; {out}')
if __name__=='__main__':main()
