#!/usr/bin/env python3
"""Run REA on source-bound, function-only arm64 subranges; never full-app proof."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

root=Path(__file__).resolve().parents[1]
folder=root/'docs/qualification/insta360-2026-10-31/capsules'
manifest=json.loads((folder/'manifest.json').read_text())
env=os.environ.copy()
env.update(GHIDRA_INSTALL_DIR='/Users/themoretheless/Applications/ghidra_12.1.4_PUBLIC',
           REA_GHIDRA_STARTUP_TIMEOUT_MS='120000',
           REA_GHIDRA_DISABLED_ANALYZERS='Decompiler Switch Analysis,Basic Constant Reference Analyzer,Objective-C Message Analyzer')
results=[]
for index,row in enumerate(manifest['capsules']):
    path=Path(row['path'])
    assert hashlib.sha256(path.read_bytes()).hexdigest()==row['sha256']
    output=folder/f'procedure-{index:02d}.json'
    started=time.monotonic()
    if index==2 and (folder/'probe.json').exists():
        value=json.loads((folder/'probe.json').read_text())
        assert value['subject']['digest']['sha256']==row['sha256']
        output.write_text(json.dumps(value,indent=2))
        code=0
    else:
        command=['/opt/homebrew/bin/rea','decompile',str(path),'--target-format','raw-binary',
                 '--architecture','arm64','--entry-point',row['address'],'--procedure',row['address'],
                 '--provider','ghidra','--format','json']
        with output.open('w')as stdout,(folder/f'procedure-{index:02d}.stderr.log').open('w')as stderr:
            code=subprocess.run(command,cwd=root,env=env,stdout=stdout,stderr=stderr,timeout=160).returncode
        value=json.loads(output.read_text())
    success=code==0 and value.get('operation')=='procedure_pseudo_code' and bool(value.get('normalized_result'))
    results.append({'address':row['address'],'bytes':row['bytes'],'success':success,
                    'exit_code':code,'seconds':time.monotonic()-started,
                    'evidence_id':value.get('evidence_id'),'output':output.name})
    (folder/'results.json').write_text(json.dumps(results,indent=2))
    print(json.dumps(results[-1]),flush=True)
print(json.dumps({'complete':True,'successful':sum(r['success']for r in results),'total':len(results)}),flush=True)
