#!/usr/bin/env python3
"""Compare current native WB helper stages with instrumented original GPU readback.

Diagnostics only: this does not qualify package parity or change tolerances.
The readback must come from scripts/reference/concat_white_balance_diagnostic.rs.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument("readback", type=Path)
p.add_argument("--toolchain", default="1.89.0")
p.add_argument("--target-dir", type=Path, default=root / "backend/target/wb-stage-diagnostic")
a = p.parse_args()
readback = json.loads(a.readback.read_text())
assert readback.get("instrumented") is True, "expected instrumented diagnostic, not a package fixture"
assert readback.get("schemaVersion") == 1
source = (root / "backend/src/domain/concat_white_balance.rs").read_text()
helpers = source[source.index("type Matrix"):source.index("#[derive")]
main = 'fn main() {\nlet source:serde_json::Value=serde_json::from_reader(std::fs::File::open(std::env::args().nth(1).unwrap()).unwrap()).unwrap();\nlet mut count=0;let mut max=0u32;\nfor c in source["cases"].as_array().unwrap(){let t=c["temperature"].as_f64().unwrap() as f32; let tint=c["tint"].as_f64().unwrap() as f32;let e=c["expression"].as_str().unwrap();let axis=if e.ends_with(".x"){0}else if e.ends_with(".y"){1}else{2};\nlet v=if e.starts_with("planckian(params.temperature *") {locus(t*1.01)[axis]}else if e.starts_with("planckian("){locus(t)[axis]}else if e.starts_with("normalize("){let a=locus(t);let b=locus(t*1.01);let d=[b[0]-a[0],b[1]-a[1]];d[axis]/(d[0]*d[0]+d[1]*d[1]).sqrt()}else if e.starts_with("white_cones(params"){white(t,tint)[axis]}else if e.starts_with("white_cones(6500"){white(6500.,0.)[axis]}else{continue};\nlet bits=c["bits"].as_u64().unwrap() as u32;let distance=v.to_bits().abs_diff(bits);max=max.max(distance);if distance>0{count+=1;println!("temperature={t} tint={tint} expression={e} native={v:?} gpu={} native_bits={:08x} gpu_bits={bits:08x} raw_bit_distance={distance}",c["value"],v.to_bits());}}\nprintln!("different_stages={count} maximum_raw_bit_distance={max}");}\n'
with tempfile.TemporaryDirectory(prefix="video-kadr-wb-stages-") as tmp:
    work = Path(tmp)
    (work / "main.rs").write_text(helpers + main)
    (work / "Cargo.toml").write_text('[package]\nname="wb-stage-diagnostic"\nversion="0.1.0"\nedition="2021"\n[[bin]]\nname="wb-stage-diagnostic"\npath="main.rs"\n[dependencies]\nserde_json="1"\n')
    subprocess.run(["cargo", "+" + a.toolchain, "run", "--offline", "--quiet", "--manifest-path", str(work / "Cargo.toml"), "--", str(a.readback.resolve())], check=True, env=dict(os.environ, CARGO_TARGET_DIR=str(a.target_dir.resolve())))
