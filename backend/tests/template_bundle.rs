use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
};
use video_kadr_backend::{
    domain::{
        composition::{CanvasSpec, Composition, SourceId},
        nested_composition::NestedCompositionProject,
    },
    template_bundle::{self, Input},
};
fn project() -> NestedCompositionProject {
    let mut content = serde_json::to_value(Composition::new(CanvasSpec::default())).unwrap();
    let source = |id: &str| json!({"id":id,"kind":"image","durationTicks":0,"width":16,"height":8,"hasAudio":false});
    content["sources"] =
        json!({"slot":source("slot"),"music":source("music"),"alias":source("alias")});
    serde_json::from_value(json!({"schemaVersion":1,"root":"root","timelineOrder":["root","other"],"projectMedia":{"slot":{"source":source("slot"),"librarySourceId":"private-original","name":"Replace me","placeholder":true},"music":{"source":source("music"),"librarySourceId":"design-file","name":"Design"},"alias":{"source":source("alias"),"librarySourceId":"design-file","name":"Other use"}},"compositions":{"root":{"content":content,"instances":[]},"other":{"content":content,"instances":[]}}})).unwrap()
}
fn package() -> (tempfile::TempDir, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("design.png");
    image::RgbaImage::from_pixel(16, 8, image::Rgba([21, 43, 65, 255]))
        .save(&path)
        .unwrap();
    let bytes = template_bundle::write(
        Cursor::new(Vec::new()),
        "Reusable edit",
        &project(),
        &[Input {
            source_id: SourceId::parse("design-file").unwrap(),
            path,
        }],
    )
    .unwrap()
    .into_inner();
    (dir, bytes)
}
#[test]
fn bundles_only_design_bytes_deduplicates_aliases_and_roundtrips_native_document() {
    let (dir, bytes) = package();
    let before = serde_json::to_value(project()).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    assert_eq!(zip.len(), 2);
    assert_eq!(zip.by_index(0).unwrap().name(), "template.json");
    let parsed = template_bundle::read(Cursor::new(&bytes), dir.path()).unwrap();
    assert_eq!(parsed.assets.len(), 1);
    let original = std::fs::read(dir.path().join("design.png")).unwrap();
    assert_eq!(std::fs::read(parsed.assets[0].path()).unwrap(), original);
    let document = serde_json::to_value(&parsed.manifest.project).unwrap();
    assert!(document["projectMedia"]["slot"]
        .get("librarySourceId")
        .is_none());
    assert_eq!(
        document["projectMedia"]["music"]["librarySourceId"],
        document["projectMedia"]["alias"]["librarySourceId"]
    );
    assert_eq!(document["compositions"], before["compositions"]);
    assert_eq!(document["timelineOrder"], before["timelineOrder"]);
    let text = serde_json::to_string(&parsed.manifest).unwrap();
    assert!(!text.contains("private-original"));
    assert!(!text.contains("design-file"));
    let staged = parsed.assets[0].path().to_owned();
    drop(parsed);
    assert!(!staged.exists());
}
fn rewrite(bytes: &[u8], change: impl FnOnce(&mut Value), payload: &[u8], extra: bool) -> Vec<u8> {
    let mut old = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut manifest: Value = serde_json::from_reader(old.by_index(0).unwrap()).unwrap();
    let key = manifest["assets"][0]["key"].as_str().unwrap().to_owned();
    change(&mut manifest);
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    out.start_file("template.json", options).unwrap();
    out.write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    out.start_file(format!("assets/{key}"), options).unwrap();
    out.write_all(payload).unwrap();
    if extra {
        out.start_file("../../escape", options).unwrap();
        out.write_all(b"bad").unwrap();
    }
    out.finish().unwrap().into_inner()
}
#[test]
fn rejects_corrupt_assets_extra_records_and_incomplete_or_bound_slot_manifests_without_staged_leaks(
) {
    let (dir, bytes) = package();
    let original = std::fs::read(dir.path().join("design.png")).unwrap();
    let stage = dir.path().join("stage");
    std::fs::create_dir(&stage).unwrap();
    let mut changed = original.clone();
    changed[0] ^= 1;
    let cases = [
        rewrite(&bytes, |_| {}, &changed, false),
        rewrite(&bytes, |_| {}, &original, true),
        rewrite(&bytes, |m| m["assets"] = json!([]), &original, false),
        rewrite(
            &bytes,
            |m| m["project"]["projectMedia"]["slot"]["librarySourceId"] = json!("private-original"),
            &original,
            false,
        ),
        rewrite(&bytes, |m| m["schemaVersion"] = json!(99), &original, false),
    ];
    for case in cases {
        assert!(template_bundle::read(Cursor::new(case), &stage).is_err());
        assert_eq!(std::fs::read_dir(&stage).unwrap().count(), 0);
    }
    assert!(template_bundle::read(Cursor::new(&bytes[..bytes.len() / 2]), &stage).is_err());
    assert_eq!(std::fs::read_dir(&stage).unwrap().count(), 0);
}
#[test]
fn packing_requires_exact_design_inventory_and_leaves_the_editor_unchanged() {
    let (dir, _) = package();
    let p = project();
    let before = serde_json::to_value(&p).unwrap();
    assert!(template_bundle::write(Cursor::new(Vec::new()), "Edit", &p, &[]).is_err());
    assert!(template_bundle::write(
        Cursor::new(Vec::new()),
        "Edit",
        &p,
        &[Input {
            source_id: SourceId::parse("private-original").unwrap(),
            path: dir.path().join("design.png")
        }]
    )
    .is_err());
    assert_eq!(serde_json::to_value(&p).unwrap(), before);
    let mut empty = p;
    empty.project_media.clear();
    for node in empty.compositions.values_mut() {
        node.content.sources.clear();
    }
    let bytes = template_bundle::write(Cursor::new(Vec::new()), "Empty", &empty, &[])
        .unwrap()
        .into_inner();
    let parsed = template_bundle::read(Cursor::new(bytes), dir.path()).unwrap();
    assert!(parsed.assets.is_empty());
    let _: BTreeMap<SourceId, SourceId> = template_bundle::plan(&project()).unwrap().assets;
}

#[test]
fn changed_design_file_during_write_refuses_the_package() {
    use std::io::{Seek, SeekFrom};
    struct MutatingWriter {
        inner: Cursor<Vec<u8>>,
        path: std::path::PathBuf,
        changed: bool,
    }
    impl Write for MutatingWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if !self.changed {
                let mut data = std::fs::read(&self.path)?;
                data[0] ^= 1;
                std::fs::write(&self.path, data)?;
                self.changed = true;
            }
            self.inner.write(bytes)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.inner.flush()
        }
    }
    impl Seek for MutatingWriter {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(pos)
        }
    }
    let (dir, _) = package();
    let path = dir.path().join("design.png");
    let writer = MutatingWriter {
        inner: Cursor::new(Vec::new()),
        path: path.clone(),
        changed: false,
    };
    let result = template_bundle::write(
        writer,
        "Edit",
        &project(),
        &[Input {
            source_id: SourceId::parse("design-file").unwrap(),
            path,
        }],
    );
    assert!(result.is_err());
}
