#!/usr/bin/env python3
"""Qualify project-wide RemoveMedia against the pinned original Concat engine."""
import importlib.util
from pathlib import Path
import sys
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('slot_oracle',ROOT/'scripts/generate-concat-media-slots-oracle.py')
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
def inputs():
    cases=[]
    for kind in ['image','video','audio']:
        for placeholder in [False,True]:
            for target in ['used','unused','missing']:
                media=[dict(id=ident,name=ident,path='/inert/'+ident,kind=kind,duration=None if kind=='image' else 4.,width=16,height=16,hasAudio=kind=='audio',placeholder=placeholder) for ident in ['used','keep','unused']]
                timelines=[]
                for ident in ['active','shelved','empty']:
                    clips=[] if ident=='empty' else [dict(id='c-'+source,trackId='lane',mediaId=source,kind=kind,name='Clip '+source,start=start,duration=1.) for source,start in [('used',0.),('keep',2.)]]
                    timelines.append(dict(id=ident,name=ident,video=dict(width=16,height=16,rateNum=24,rateDen=1),tracks=[dict(id='lane',name='Lane')],clips=clips))
                cases.append(dict(document=dict(version=1,activeTimelineId='active',media=media,timelines=timelines),command=dict(op='removeMedia',mediaId=target)))
    return cases
module.inputs=inputs
if '--output' not in sys.argv:
    sys.argv.extend(['--output',str(ROOT/'backend/tests/fixtures/concat-remove-media-oracle.json')])
module.main()
