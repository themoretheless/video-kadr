#!/usr/bin/env python3
"""Compare whole decoded-frame feeding with the actual original pre-AAC mix.

This is a bounded synthetic diagnostic, not a production renderer: each input
is only eight seconds long. The full-file scan of aselect must not be enabled
for arbitrary user media. All media is temporary; only scalar results remain.
"""
from pathlib import Path
import array
import json
import math
import subprocess
import sys
import tempfile
root = Path(__file__).resolve().parent.parent
binary = root / 'backend/target/concat-audio-waveform-build/debug/concat-audio-waveform'
rows = [list(map(float, line.split())) for line in (root / 'backend/target/concat-retime-oracle/retime.txt').read_text().splitlines()]
def run(args):
    p = subprocess.run(args, capture_output=True, timeout=60)
    if p.returncode:
        raise RuntimeError(p.stderr.decode(errors='replace'))
    return p.stdout
def escape(expression):
    return expression.replace(',', r'\,')
bounded_chunks = "--chunks" in sys.argv
results = []
with tempfile.TemporaryDirectory(prefix='concat-audio-frames-') as directory:
    directory = Path(directory)
    source = directory / 'source.wav'
    run(['ffmpeg','-nostdin','-v','error','-f','lavfi','-i','aevalsrc=0.1*sin(2*PI*440*t)|0.2*sin(2*PI*660*t):s=48000:d=8','-c:a','pcm_f32le',str(source)])
    for case in [0, 2, 5]:
        pieces = [dict(path=str(source),start=r[4],duration=r[5],sourceStart=r[6],speed=r[7],fadeIn=r[8],fadeOut=r[9]) for r in rows if r[0] == 2 and r[1] == case and r[2] == 2.019]
        assert len(pieces) == 21
        plan = directory / 'plan.json'
        plan.write_text(json.dumps(dict(duration=2.04,clips=pieces)))
        pcm = directory / 'reference.f32'
        run([str(binary),str(plan),str(directory / 'reference.m4a'),str(pcm)])
        reference = array.array('f', pcm.read_bytes())
        assert len(reference) == 195840
        args = ['ffmpeg','-nostdin','-v','error']
        chains = []
        for index, piece in enumerate(pieces):
            seek = math.floor(piece['sourceStart'] * 1e6) / 1e6
            # Probe the decoder's landed frame, retaining the exact timestamp.
            probe = json.loads(run(['ffprobe','-v','error','-read_intervals',f'{seek:.6f}%+#1','-select_streams','a:0','-show_entries','frame=best_effort_timestamp,pts,nb_samples:stream=sample_rate,time_base','-of','json',str(source)]))
            stream = probe['streams'][0]
            time_num, time_den = map(int, stream['time_base'].split('/'))
            stamp = probe['frames'][0].get('best_effort_timestamp', probe['frames'][0]['pts'])
            landed = stamp * time_num / time_den
            lead = max(piece['sourceStart']-landed,0)
            count = math.floor((piece['duration']*piece['speed']+lead)*int(stream['sample_rate'])+.5)
            if bounded_chunks:
                # This WAV fixture has fixed 4096-sample decoder frames. The
                # production streaming probe supplies the actual frame count.
                size = int(probe['frames'][0]['nb_samples'])
                frames = (count + size - 1) // size
                chunk = directory / f'chunk-{case}-{index}.nut'
                run(['ffmpeg','-nostdin','-v','error','-noaccurate_seek','-ss',f'{seek:.6f}','-i',str(source),'-map','0:a:0','-frames:a',str(frames),'-af','asetpts=N/SR/TB','-c:a','pcm_f32le','-f','nut',str(chunk)])
                captured = json.loads(run(['ffprobe','-v','error','-select_streams','a:0','-show_frames','-show_entries','frame=nb_samples','-of','json',str(chunk)]))['frames']
                assert len(captured) == frames
                assert [int(frame['nb_samples']) for frame in captured] == [size] * frames
                args += ['-i',str(chunk)]
            else:
                args += ['-noaccurate_seek','-ss',f'{seek:.6f}','-i',str(source)]
            # Select the entire last decoded frame, as original MixInput does.
            # Eval registers count samples before each frame independently of PTS.
            select = f'lt(st(0,ld(0)+samples_n)-samples_n,{count})'
            filters = ['aselect='+escape(select),'asetpts=N/SR/TB']
            if lead > 0:
                filters += [f'atrim=start={lead:.6f}','asetpts=PTS-STARTPTS']
            speed = piece['speed']
            if abs(speed-1) >= math.ulp(1.0):
                while speed > 2:
                    filters.append('atempo=2.000000')
                    speed /= 2
                while speed < .5:
                    filters.append('atempo=0.500000')
                    speed /= .5
                filters.append(f'atempo={speed:.6f}')
            if piece['fadeIn'] > 0:
                filters.append(f"afade=t=in:st=0:d={piece['fadeIn']:.4f}")
            if piece['fadeOut'] > 0:
                filters.append(f"afade=t=out:st={max(piece['duration']-piece['fadeOut'],0):.4f}:d={piece['fadeOut']:.4f}")
            filters.append(f"adelay={math.floor(piece['start']*1000+.5)}:all=1")
            chains.append(f'[{index}:a]'+','.join(filters)+f'[a{index}]')
        chains.append(''.join(f'[a{i}]' for i in range(len(pieces)))+f'amix=inputs={len(pieces)}:normalize=0[mix]')
        chains.append('[mix]aresample=48000,apad,atrim=end_sample=97920[out]')
        actual = array.array('f',run(args+['-filter_complex',';'.join(chains),'-map','[out]','-c:a','pcm_f32le','-f','f32le','pipe:1']))
        assert len(actual) == len(reference)
        differences = [abs(a-b) for a,b in zip(actual,reference)]
        result = dict(curve=case,boundedChunks=bounded_chunks,maxAbsError=max(differences),rmsError=math.sqrt(sum(v*v for v in differences)/len(differences)),mismatchingScalars=sum(v>2e-6 for v in differences),totalScalars=len(differences))
        results.append(result)
        print(json.dumps(result), flush=True)
(root / ('backend/target/concat-audio-chunk-diagnostics.json' if bounded_chunks else 'backend/target/concat-audio-frame-diagnostics.json')).write_text(json.dumps(results,indent=2)+'\n')
