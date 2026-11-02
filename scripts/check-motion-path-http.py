#!/usr/bin/env python3
"""Exercise the built backend's actual motion-path HTTP route in isolated storage."""
import copy
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='video-kadr-path-http-') as storage:
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
        port = probe.getsockname()[1]
    env = dict(os.environ, PORT=str(port), BIND_ADDR='127.0.0.1', STORAGE_DIR=storage)
    with open(Path(storage) / 'backend.log', 'w+') as log:
        process = subprocess.Popen([str(root / 'backend/target/debug/video-kadr-backend')], cwd=storage, env=env, stdout=log, stderr=log)
        url = f'http://127.0.0.1:{port}/api/compositions/motion-path'
        def request(payload, endpoint='motion-path'):
            data = json.dumps(payload).encode()
            try:
                target = f'http://127.0.0.1:{port}/api/compositions/{endpoint}'
                with urllib.request.urlopen(urllib.request.Request(target, data=data, headers={'Content-Type': 'application/json'}), timeout=5) as response:
                    return response.status, json.load(response)
            except urllib.error.HTTPError as error:
                return error.code, error.read().decode()
        def node(identifier, x, y):
            return dict(id=identifier, point=dict(x=x,y=y), inHandle=dict(x=0,y=0), outHandle=dict(x=0,y=0), segment='line', tangentMode='corner')
        path = dict(nodes=[node('a',0,0),node('b',30,0),node('c',30,70)])
        try:
            deadline = time.monotonic()+20
            while True:
                if process.poll() is not None:
                    log.seek(0)
                    raise RuntimeError(log.read())
                try:
                    status, measured = request(dict(operation='measure',path=path))
                    break
                except urllib.error.URLError:
                    if time.monotonic() >= deadline:
                        raise
                    time.sleep(0.05)
            assert status == 200, measured
            assert measured['length'] == 100
            assert measured['nodeProgresses'] == [0,0.3,1]
            status, sample = request(dict(operation='sample',path=path,progresses=[0.3,0.5,1.1]))
            assert status == 200 and sample['points'][1] == dict(x=30,y=20), sample
            status, hit = request(dict(operation='nearest',path=path,point=dict(x=15,y=4)))
            assert status == 200 and hit['nearest']['segment'] == 0, hit
            assert abs(hit['nearest']['t']-0.5) < 1e-6 and abs(hit['nearest']['distance']-4) < 1e-8, hit
            status, translated = request(dict(operation='translate',path=path,delta=dict(x=13,y=-9)))
            assert status == 200 and translated['length'] == measured['length'], translated
            assert translated['path']['nodes'][1]['point'] == dict(x=43,y=-9), translated
            status, moved_sample = request(dict(operation='sample',path=translated['path'],progresses=[0.5]))
            assert status == 200 and moved_sample['points'][0] == dict(x=43,y=11), moved_sample
            status, curved = request(dict(operation='toggle_segment',path=path,index=0))
            assert status == 200 and curved['path']['nodes'][0]['segment'] == 'cubic', curved
            assert curved['nodeProgresses'][0] == 0 and curved['nodeProgresses'][-1] == 1
            assert 0 < curved['nodeProgresses'][1] < 1
            status, curve_hit = request(dict(operation='nearest',path=curved['path'],point=dict(x=15,y=-30)))
            assert status == 200 and curve_hit['nearest']['segment'] == 0, curve_hit
            assert 0 < curve_hit['nearest']['t'] < 1, curve_hit
            status, automatic = request(dict(operation='mode',path=curved['path'],index=1,mode='auto'))
            assert status == 200 and automatic['path']['nodes'][1]['tangentMode'] == 'auto', automatic
            status, manual = request(dict(operation='edit',path=automatic['path'],index=1,part='out_handle',value=dict(x=10,y=20)))
            assert status == 200 and manual['path']['nodes'][1]['tangentMode'] == 'smooth', manual
            coincident = dict(nodes=[node('zero-a',5,7),node('zero-b',5,7)])
            status, zero = request(dict(operation='sample',path=coincident,progresses=[-1,0.5,2]))
            assert status == 200 and zero['length'] == 0 and zero['nodeProgresses'] == [0,0], zero
            assert zero['points'] == [dict(x=5,y=7)]*3, zero
            status, inserted = request(dict(operation='insert',path=curved['path'],segment=0,t=0.37,id='inserted'))
            assert status == 200 and len(inserted['path']['nodes']) == 4, inserted
            status, removed = request(dict(operation='remove',path=inserted['path'],index=1))
            assert status == 200 and len(removed['path']['nodes']) == 3, removed
            status, error = request(dict(operation='remove',path=path,index=999))
            assert status == 400, (status,error)
            status, error = request(dict(operation='measure',path=path,unexpected=True))
            assert 400 <= status < 500, (status,error)
            def conversion_key(t, value):
                return dict(t_us=t,value=value,in_=[2/3,2/3],out=[1/3,1/3],segment=dict(kind='Linear'))
            def conversion_track(keys):
                return dict(keys=keys,extrapolate=dict(before='Hold',after='Hold'))
            options = dict(fpsNum=30,fpsDen=1,startFrame=0,endFrame=30,tolerancePx=0.5,everyFrames=1,xyMode='editable')
            source = dict(nodes=None,tracks=[conversion_track([conversion_key(0,0),conversion_key(1000000,100)]),conversion_track([conversion_key(0,20)])],options=options)
            status, fitted = request(source,'position-conversion')
            assert status == 200 and fitted['withinTolerance'], fitted
            assert fitted['maxErrorPx'] <= 0.5 and len(fitted['tracks']) == 1, fitted
            assert 1 <= len(fitted['nodes']) <= 128, fitted
            # Convert the actual native result back; do not substitute a hand-authored path.
            reverse = dict(nodes=fitted['nodes'],tracks=fitted['tracks'],options=options)
            status, restored = request(reverse,'position-conversion')
            assert status == 200 and restored['withinTolerance'] and restored['nodes'] is None, restored
            assert len(restored['tracks']) == 2 and restored['maxErrorPx'] <= 0.5, restored
            assert abs(restored['tracks'][0]['keys'][0]['value']) < 1e-7, restored
            assert abs(restored['tracks'][0]['keys'][-1]['value']-100) < 1e-7, restored
            assert abs(restored['tracks'][1]['keys'][0]['value']-20) < 1e-7, restored
            reverse['options'] = dict(options,xyMode='bake',everyFrames=5)
            status, baked = request(reverse,'position-conversion')
            assert status == 200 and baked['withinTolerance'], baked
            assert len(baked['tracks'][0]['keys']) == 7, baked
            source['options'] = dict(options,tolerancePx=0)
            status, error = request(source,'position-conversion')
            assert status == 400, (status,error)
            print('PASS: real HTTP XY→Path→XY and frame baking, tolerance refusal')
            fixture = json.loads((root / 'backend/tests/fixtures/weftcut-animated-color.json').read_text())
            color_samples = 0
            for case in fixture['cases']:
                authored = case['track']
                if authored['mode'] == 'Static':
                    keys = [dict(conversion_key(0,authored['value']))]
                else:
                    keys = [dict(t_us=key['t_us'],value=key['value'],in_=[key['in']['x'],key['in']['y']],out=[key['out']['x'],key['out']['y']],segment=key['segment']) for key in authored['value']]
                color_request = dict(track=dict(keys=keys,extrapolate=authored.get('extrapolate',dict(before='Hold',after='Hold'))),default=fixture['default'],times_us=[sample['t_us'] for sample in case['samples']])
                status, sampled = request(color_request,'animated-color')
                assert status == 200 and len(sampled['colors']) == len(case['samples']), (case['name'],sampled)
                for actual, sample in zip(sampled['colors'],case['samples']):
                    assert all(abs(actual[channel]-sample['expect'][channel]) <= 1 for channel in ['r','g','b','a']), (case['name'],sample,actual)
                    color_samples += 1
            invalid_color = dict(track=dict(keys=[],extrapolate=dict(before='Hold',after='Hold')),default=dict(r=256,g=0,b=0,a=255),times_us=[0])
            status, error = request(invalid_color,'animated-color')
            assert 400 <= status < 500, (status,error)
            print(f'PASS: real HTTP OkLab color goldens, {len(fixture["cases"])} cases/{color_samples} samples and invalid channel refusal')
            legacy_start = conversion_key(0.5,dict(r=255,g=0,b=0,a=255))
            legacy_start.update(segment=dict(kind='Spline'),out=[1/3,0],legacy_curve='ease_in_out')
            legacy_end = conversion_key(1000000.5,dict(r=0,g=0,b=0,a=0))
            legacy_end['in_'] = [2/3,1]
            legacy_color = dict(track=conversion_track([legacy_start,legacy_end]),default=fixture['default'],times_us=[250000,750000])
            status, sampled = request(legacy_color,'animated-color')
            assert status == 200 and sampled['colors'] == [dict(r=255,g=0,b=0,a=239),dict(r=255,g=0,b=0,a=16)], sampled
            constant = lambda value: dict(mode='constant',value=value)
            transform = dict(x=constant(0),y=constant(0),scaleX=constant(1),scaleY=constant(1),rotationDegrees=constant(0),anchorX=.5,anchorY=.5)
            content = dict(schemaVersion=1,timeBase=1000000,canvas=dict(width=64,height=64,fpsMilli=30000,background=dict(red=0,green=0,blue=0,alpha=1),backgroundMode='color',backgroundBlur=24),sources={},tracks=[])
            def nested_instance(identifier,child,start,source_in,source_out):
                return dict(id=identifier,timing=dict(composition=child,timelineStartUs=start,sourceInUs=source_in,sourceOutUs=source_out),transform=copy.deepcopy(transform),opacity=constant(.5),blendMode='normal')
            project = dict(schemaVersion=1,root='root',compositions={
                'root': dict(content=copy.deepcopy(content),instances=[nested_instance('outer','child',500000,100000,1100000)]),
                'child': dict(content=copy.deepcopy(content),instances=[nested_instance('inner','leaf',200000,700000,1700000)]),
                'leaf': dict(content=copy.deepcopy(content),instances=[])})
            status, frame = request(dict(project=project,timeUs=800000),'nested-frame')
            assert status == 200, frame
            assert [entry['sourceTimeUs'] for entry in frame['entries']] == [800000,400000,900000], frame
            assert frame['entries'][2]['instancePath'] == ['outer','inner'], frame
            assert len(frame['entries'][2]['ancestors']) == 2, frame
            status, frame = request(dict(project=project,timeUs=1500000),'nested-frame')
            assert status == 200 and len(frame['entries']) == 1, frame
            status, grouped = request(dict(project=project,parent='root',child='grouped',instanceId='new-group'),'nested-group')
            assert status == 200, grouped
            assert grouped['compositions']['grouped'] == project['compositions']['root'], grouped
            status, frame = request(dict(project=grouped,timeUs=800000),'nested-frame')
            assert status == 200 and [entry['sourceTimeUs'] for entry in frame['entries']] == [800000,800000,400000,900000], frame
            assert frame['entries'][-1]['instancePath'] == ['new-group','outer','inner'], frame
            status, error = request(dict(project=project,parent='root',child='child',instanceId='new-group'),'nested-group')
            assert status == 400, (status,error)
            project['compositions']['leaf']['instances'] = [nested_instance('cycle','child',0,0,1000000)]
            status, error = request(dict(project=project,timeUs=0),'nested-frame')
            assert status == 400 and 'cycle' in error, (status,error)
            print('PASS: real HTTP nested frame offsets, group paths, timing boundaries and cycle rejection')
            print('PASS: real HTTP fractional color key times and exact legacy cubic fade')
            print('PASS: real HTTP measure/sample/nearest/translate/modes/handles/curve/insert/remove, zero length and invalid requests')
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
