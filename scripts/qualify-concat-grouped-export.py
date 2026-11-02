#!/usr/bin/env python3
"""Read/write an actual grouped native export through pinned original Concat."""
from pathlib import Path
import hashlib,json,subprocess
root=Path(__file__).resolve().parent.parent
reference=root/'backend/target/concat-reference/src'
commit='78e4be101c2d3c41a7dc73c112c6b1f11b9ce170'
assert subprocess.check_output(['git','-C',str(reference),'rev-parse','HEAD'],text=True).strip()==commit
subprocess.run(['git','-C',str(reference),'diff','--quiet','HEAD','--',':(top)src/crates/concat-project/src',':(top)src/crates/concat-core/src'],check=True)
source=root/'backend/target/concat-grouped-export.json'
assert source.is_file(),'run concat_freeze_native with CONCAT_GROUPED_EXPORTED_OUTPUT first'
out=root/'backend/target/concat-grouped-reader';out.mkdir(exist_ok=True)
(out/'Cargo.toml').write_text('''[package]
name="video-kadr-concat-grouped-reader"
version="0.1.0"
edition="2024"
[workspace]
[[bin]]
name="concat-grouped-reader"
path="../../../scripts/reference/concat_project_reader.rs"
[dependencies]
concat-project={path="../concat-reference/src/crates/concat-project"}
serde_json="1"
''')
output=out/'rewritten.json'
build=root/'backend/target/concat-retime-command-build'
subprocess.run(['cargo','+1.97.1','run','--manifest-path',str(out/'Cargo.toml'),'--target-dir',str(build),'--',str(source),str(output)],check=True)
files=[*sorted((reference/'crates/concat-project/src').rglob('*.rs')),*sorted((reference/'crates/concat-core/src').rglob('*.rs')),root/'scripts/reference/concat_project_reader.rs',Path(__file__).resolve(),source,output,out/'Cargo.toml',out/'Cargo.lock',build/'debug/concat-grouped-reader']
(out/'provenance.json').write_text(json.dumps({'commit':commit,'sha256':{str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in files}},indent=2))
print(output)
