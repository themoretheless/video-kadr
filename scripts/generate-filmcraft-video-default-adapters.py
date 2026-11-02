#!/usr/bin/env python3
"""Build candidate native defaults for the127original scene effects (not a parameter importer)."""
from pathlib import Path
import json,argparse
p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');a=p.parse_args()
root=Path(__file__).resolve().parents[1]
source=json.loads((root/'backend/tests/fixtures/filmcraft-transition-side-effects.json').read_text())
assert source['commit']=='5231852443363f001c3f6b396dd9b1e6461ae2be'
names=sorted({v['effect'] for v in source['cases']});assert len(names)==127
aliases={'extract':'color_extract','invert':'color_invert','levels':'color_levels','tint':'color_tint','gradient_wipe_legacy':'gradient_wipe','linear_wipe_legacy':'linear_wipe','magnify_legacy':'magnify'}
entries=[]
for name in names:
 kind=aliases.get(name,name);wire={'kind':kind}
 if kind not in ['horizontal_flip','vertical_flip','black_white']:wire['parameters']={}
 entries.append({'source':name,'native':wire})
result={'commit':source['commit'],'scope':'Native default candidates; parameter translation and pixel parity are separate checks','effects':entries}
target=root/'backend/tests/fixtures/filmcraft-video-default-adapters.json'
if a.check:assert json.loads(target.read_text())==result
else:target.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
print('127native default candidate mappings: PASS')
