#!/usr/bin/env python3
"""Isolated real HTTP render job, native color resources and decoded MP4 proof."""
import argparse
import copy
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.request

parser = argparse.ArgumentParser()
parser.add_argument('--ffmpeg', required=True)
parser.add_argument('--canvas-only', action='store_true', help='Render over canvas with no media input')
parser.add_argument('--changing-solid', action='store_true', help='Animate solid green to yellow and verify decoded frames')
parser.add_argument('--solid', action='store_true', help='Exercise an animated solid layer beside text')
args = parser.parse_args()
if args.changing_solid and not args.solid:
    parser.error('--changing-solid requires --solid')
ffmpeg = Path(args.ffmpeg).resolve()
root = Path(__file__).resolve().parents[1]
constant = lambda value: {'mode': 'constant', 'value': value}
transform = dict(x=constant(0), y=constant(0), scaleX=constant(1), scaleY=constant(1), rotationDegrees=constant(0), anchorX=.5, anchorY=.5)
rgba = lambda r, g, b, a: dict(red=r, green=g, blue=b, alpha=a)
keys = [dict(t_us=t, value=dict(r=r, g=0, b=b, a=a), in_=[2/3, 2/3], out=[1/3, 1/3], segment={'kind': 'Linear'}) for t, r, b, a in [(-.5, 255, 0, 128), (1000000.5, 0, 255, 255)]]
color_track = dict(keys=keys, extrapolate=dict(before='Hold', after='Hold'))
with tempfile.TemporaryDirectory(prefix='video-kadr-color-http-') as temporary:
    storage = Path(temporary)
    (storage / 'sources').mkdir()
    media = storage / 'sources' / 'primary.mkv'
    subprocess.run([str(ffmpeg), '-v', 'error', '-f', 'lavfi', '-i', 'color=black:s=64x64:r=30000/1001:d=1', '-c:v', 'ffv1', str(media)], check=True)
    (storage / 'library.json').write_text(json.dumps([dict(id='primary', kind='source', filename=media.name, url='/files/sources/' + media.name, mediaType='video', duration=1.001, width=64, height=64, fps=30000/1001, vcodec='ffv1', createdAt=0)]))
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
        port = probe.getsockname()[1]
    env = dict(os.environ, PATH=str(ffmpeg.parent) + os.pathsep + os.environ['PATH'], STORAGE_DIR=temporary, PORT=str(port), BIND_ADDR='127.0.0.1')
    url = f'http://127.0.0.1:{port}'
    with (storage / 'backend.log').open('w+') as log:
        process = subprocess.Popen([str(root / 'backend/target/debug/video-kadr-backend')], cwd=temporary, env=env, stdout=log, stderr=log)
        def request(path, body=None):
            data = None if body is None else json.dumps(body).encode()
            with urllib.request.urlopen(urllib.request.Request(url + path, data=data, headers={'Content-Type': 'application/json'}), timeout=10) as response:
                return json.load(response)
        try:
            deadline = time.monotonic() + 30
            while True:
                assert process.poll() is None, 'backend exited'
                try:
                    request('/api/health')
                    break
                except OSError:
                    assert time.monotonic() < deadline, 'backend startup timed out'
                    time.sleep(.1)
            composition = dict(schemaVersion=1, timeBase=1000000,
                canvas=dict(width=64, height=64, fpsMilli=29970, fpsRate=dict(numerator=30000, denominator=1001), background=rgba(0, 0, 0, 1)),
                sources={'primary': dict(id='primary', kind='video', durationTicks=1001000, width=64, height=64, hasAudio=False)},
                tracks=[dict(id='titles', kind='text', name='Text', clips=[dict(id='title', timelineStartTick=0, timelineEndTick=1000000, text='M',
                    style=dict(fontFamily='Arial', fontSize=32, color=rgba(1, 0, 0, 1), background=rgba(0, 0, 0, 0), stroke=rgba(0, 0, 0, 0), strokeWidth=0, shadow=rgba(0, 0, 0, 0), shadowX=0, shadowY=0),
                    transform=transform, opacity=constant(1), colorAnimation=color_track)]),
                    dict(id='video', kind='video', name='Video', clips=[dict(id='primary-clip', sourceId='primary', placement=dict(timelineStartTick=0, sourceInTick=0, sourceOutTick=1000000, speed=1), transform=transform, opacity=constant(1), blendMode='normal', sourceAudioEnabled=False)])])
            first = composition['tracks'][0]['clips'][0]
            first['transform'] = dict(transform, x=constant(-16))
            second = copy.deepcopy(first)
            second['id'] = 'title_2'
            second['transform']['x'] = constant(16)
            second['colorAnimation']['keys'] = [dict(t_us=0, value=dict(r=0, g=255, b=0, a=255), in_=[2/3, 2/3], out=[1/3, 1/3], segment={'kind': 'Linear'})]
            composition['tracks'].insert(0, dict(id='green-title', kind='text', name='Independent green', clips=[second]))
            if args.solid:
                composition['tracks'][0] = dict(id='green-solid', kind='color', name='Independent solid', clips=[dict(
                    id='solid', timelineStartTick=0, timelineEndTick=1000000, width=32, height=24,
                    color=dict(r=0, g=255, b=0, a=255), colorAnimation=second['colorAnimation'],
                    transform=second['transform'], opacity=constant(1))])
            if args.changing_solid:
                solid = composition['tracks'][0]['clips'][0]
                solid['colorAnimation']['keys'].append(dict(t_us=1000000, value=dict(r=255, g=255, b=0, a=255), in_=[2/3, 2/3], out=[1/3, 1/3], segment={'kind': 'Linear'}))
            if args.canvas_only:
                composition['sources'] = {}
                composition['tracks'] = [track for track in composition['tracks'] if track['kind'] != 'video']
            try:
                started = request('/api/compositions/render', dict(schemaVersion=1, composition=composition))
            except urllib.error.HTTPError as error:
                raise RuntimeError(error.read().decode()) from error
            deadline = time.monotonic() + 40
            while True:
                job = request('/api/jobs/' + started['jobId'])
                if job['status'] == 'done':
                    break
                assert job['status'] not in ['error', 'cancelled', 'interrupted'], job
                assert time.monotonic() < deadline, job
                time.sleep(.1)
            result = job['result']
            filename = result['filename']
            candidates = list(storage.rglob(filename))
            assert len(candidates) == 1, (result, candidates)
            decoded = subprocess.run([str(ffmpeg), '-v', 'error', '-i', str(candidates[0]), '-map', '0:v:0', '-pix_fmt', 'rgb24', '-f', 'rawvideo', 'pipe:1'], capture_output=True, check=True).stdout
            size = 64 * 64 * 3
            assert len(decoded) == 30 * size, len(decoded)
            interior = next(i for i in range(0, size, 3) if decoded[i] > 100 and decoded[i+1] < 20 and decoded[i+2] < 20)
            last = tuple(decoded[29*size + interior:29*size + interior + 3])
            assert last[2] > 180 and last[0] < 60, last
            green = next(i for i in range(0, size, 3) if (i // 3) % 64 >= 32 and decoded[i+1] > 220 and decoded[i] < 20 and decoded[i+2] < 20)
            if args.changing_solid:
                green = (32 * 64 + 48) * 3
            solid_red = []
            for frame in range(30):
                pixel = tuple(decoded[frame*size + green:frame*size + green + 3])
                assert pixel[1] > 220 and pixel[2] < 20, (frame, pixel)
                if not args.changing_solid:
                    assert pixel[0] < 20, (frame, pixel)
                solid_red.append(pixel[0])
            if args.changing_solid:
                assert solid_red[0] < 20 and solid_red[-1] > 220, solid_red
                assert all(later + 5 >= earlier for earlier, later in zip(solid_red, solid_red[1:])), solid_red
                assert len(set(solid_red)) > 20, solid_red
            assert not list(storage.rglob('composition-color-*.txt')), 'temporary color file leaked'
            print('Actual HTTP animated-color job/MP4/decode/cross-layer isolation/cleanup PASS', job['status'], '30 frames', last)
        except Exception:
            log.seek(0)
            print(log.read()[-8000:])
            raise
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
