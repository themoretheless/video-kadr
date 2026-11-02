#!/usr/bin/env python3
"""Create a source-pinned parity ledger; catalog presence never proves behavior."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib

PIN = "5231852443363f001c3f6b396dd9b1e6461ae2be"
ROOT = Path(__file__).resolve().parents[1]


def git(reference, *arguments):
    return subprocess.check_output(["git", "-C", str(reference), *arguments])


def emit(reference, toolchain):
    with tempfile.TemporaryDirectory(prefix="filmcraft-catalog-") as directory:
        build = Path(directory)
        (build / "src").mkdir()
        dependencies = "\n".join(
            f'filmcraft-{name}={{path={json.dumps(str(reference / "crates" / name))}}}'
            for name in ["engine", "project", "export"]
        )
        (build / "Cargo.toml").write_text(
            '[package]\nname="filmcraft-catalog-audit"\nversion="0.1.0"\n'
            'edition="2024"\n[dependencies]\n' + dependencies + '\nserde_json="1"\n'
        )
        (build / "src/main.rs").write_text((ROOT / "scripts/filmcraft-catalog.rs").read_text())
        output = build / "catalog.json"
        command = ["cargo"] + ([f"+{toolchain}"] if toolchain else [])
        subprocess.run(command + ["run", "--manifest-path", str(build / "Cargo.toml"),
                                  "--", str(output)], check=True)
        return json.loads(output.read_text())


def generate(reference, runtime, overrides):
    if git(reference, "rev-parse", "HEAD").decode().strip() != PIN:
        raise ValueError("reference HEAD differs from the pinned FilmCraft snapshot")
    if git(reference, "status", "--porcelain", "--untracked-files=no").strip():
        raise ValueError("reference has tracked changes; source provenance is ambiguous")
    if runtime.get("sourceCommit") != PIN or runtime.get("schemaVersion") != 1:
        raise ValueError("runtime catalog has wrong provenance/schema")
    requirements = []
    ids = set()

    def add(kind, key, label, source):
        identity = f"{kind}:{key}"
        if identity in ids:
            raise ValueError(f"duplicate requirement: {identity}")
        ids.add(identity)
        item = {"id": identity, "kind": kind, "label": label,
                "source": source, "status": "unreviewed", "evidence": []}
        requirements.append(item)

    for command in runtime["commands"]:
        add("command", command["id"], command["label"], "crates/engine/src/commands.rs")
    for effect in runtime["effects"]:
        add("effect", effect["id"], effect["name"], "crates/project/src/effect.rs")
        for param in effect["params"]:
            add("parameter", f'{effect["id"]}.{param["id"]}', param["label"],
                "crates/project/src/effect.rs")
    for name in runtime["exportFormats"]:
        add("export-format", name, name, "crates/export/src/lib.rs")
    for preset in runtime["exportPresets"]:
        add("export-preset", preset["name"], preset["name"], "crates/export/src/presets.rs")

    source_index, packages = [], []
    files = sorted(git(reference, "ls-files", "-z").decode().split("\0"))
    for relative in filter(None, files):
        path = reference / relative
        if not path.is_file():
            raise ValueError(f"tracked source missing: {relative}")
        content = path.read_bytes()
        entry = {"path": relative, "bytes": len(content),
                 "sha256": hashlib.sha256(content).hexdigest()}
        if path.suffix == ".rs":
            text = content.decode()
            product = text.split("#[cfg(test)]", 1)[0]
            entry["publicDeclarations"] = [
                {"kind": m[1], "name": m[2], "line": product.count("\n", 0, m.start()) + 1}
                for m in re.finditer(r"(?m)^\s*pub\s+(?:async\s+)?(fn|struct|enum|trait|type|const)\s+(\w+)", product)
            ]
            entry["reviewStatus"] = "unreviewed"
        elif path.name == "Cargo.toml":
            metadata = tomllib.loads(content.decode())
            if "package" in metadata:
                packages.append({"path": relative, "name": metadata["package"]["name"],
                                 "features": metadata.get("features", {})})
                for feature in metadata.get("features", {}):
                    add("build-feature", f'{metadata["package"]["name"]}.{feature}', feature, relative)
        elif path.suffix == ".md" and (relative.startswith("docs/") or relative in ["README.md", "ROADMAP.md"]):
            for line, text in enumerate(content.decode().splitlines(), 1):
                if re.match(r"^#{2,4} ", text):
                    add("doc-section", f"{relative}:{line}", text.lstrip("# "), relative)
        source_index.append(entry)

    server = reference / "crates/automation/src/server.rs"
    text = server.read_text()
    for match in re.finditer(r"#\[tool\([\s\S]*?\)\]\s*async fn (\w+)", text):
        add("mcp-tool", match[1], match[1], "crates/automation/src/server.rs")

    for item in requirements:
        override = overrides.get(item["id"])
        if not override:
            continue
        if override["status"] not in ["unreviewed", "missing", "partial", "verified"]:
            raise ValueError(f'invalid status: {item["id"]}')
        if override["status"] == "verified":
            qualification = override.get("qualification", {})
            if not override.get("evidence") or any(not qualification.get(key) for key in
                    ["sourceContract", "verificationCommand", "resultArtifact", "scope"]):
                raise ValueError(f'verified without contract/qualification: {item["id"]}')
            if not (ROOT / qualification["resultArtifact"]).is_file():
                raise ValueError(f'missing qualification artifact: {item["id"]}')
        for evidence in override.get("evidence", []):
            if not (ROOT / evidence["path"]).is_file():
                raise ValueError(f'missing host evidence: {evidence["path"]}')
        item.update(override)
    unknown = set(overrides) - ids
    if unknown:
        raise ValueError(f"overrides name absent source requirements: {sorted(unknown)}")
    summary = {"requirementsByKind": dict(sorted(Counter(r["kind"] for r in requirements).items())),
               "statuses": dict(sorted(Counter(r["status"] for r in requirements).items())),
               "effectKinds": dict(sorted(Counter(e["kind"] for e in runtime["effects"]).items())),
               "sourceFiles": len(source_index), "packages": len(packages)}
    return {"schemaVersion": 1, "sourceCommit": PIN,
            "scope": "Default-feature runtime catalogs plus tracked-source/package/doc inventory; behavior and optional-feature execution need separate qualification.",
            "summary": summary, "runtime": runtime, "requirements": requirements,
            "packages": packages, "sourceIndex": source_index}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--runtime-catalog", type=Path)
    parser.add_argument("--toolchain", help="toolchain >= FilmCraft's declared rust-version")
    parser.add_argument("--output", type=Path, default=ROOT / "docs/filmcraft-catalog.json")
    parser.add_argument("--overrides", type=Path, default=ROOT / "docs/filmcraft-parity-overrides.json")
    parser.add_argument("--check", action="store_true", help="compare with saved ledger without writing")
    args = parser.parse_args()
    reference = args.reference.resolve()
    runtime = json.loads(args.runtime_catalog.read_text()) if args.runtime_catalog else emit(reference, args.toolchain)
    overrides = json.loads(args.overrides.read_text())
    result = generate(reference, runtime, overrides)
    rendered = json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if args.check:
        if args.output.read_text() != rendered:
            raise ValueError("saved ledger differs from current pinned catalog/source/overrides")
    else:
        args.output.write_text(rendered)
    print(json.dumps(result["summary"], ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    main()
