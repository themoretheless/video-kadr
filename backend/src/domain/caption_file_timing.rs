//! Caption file timing in Filmcraft's exact source clock. No host-us conversion.
use filmcraft_captions::Document;
use filmcraft_time::Tick;

/// Original import normalization: nearest frame, one-frame minimum, cut overlaps.
pub fn snap_document_for_sequence(
    mut document: Document,
    rate: filmcraft_time::FrameRate,
) -> Document {
    document.snap_to_frames(rate);
    document
}

/// Original document_from_track range semantics; retained cues keep file metadata.
pub fn document_for_range(document: &Document, offset: Tick) -> Document {
    Document {
        cues: document
            .cues
            .iter()
            .filter(|cue| cue.end > offset)
            .map(|cue| {
                let mut cue = cue.clone();
                cue.start = (cue.start - offset).max(Tick::ZERO);
                cue.end = cue.end - offset;
                cue
            })
            .collect(),
        blocks: document.blocks.clone(),
        warnings: Vec::new(),
    }
}

/// Import preparation keeps exact source time and file metadata. IDs and the
/// authored-document transaction belong to the composition adapter.
pub struct PreparedCaptionImport {
    pub name: String,
    pub format: filmcraft_captions::CaptionFormat,
    pub language: String,
    pub enabled: bool,
    pub locked: bool,
    pub sync_lock: bool,
    pub background_color: [u8; 4],
    pub document: Document,
}
pub fn prepare_file_import(
    bytes: &[u8],
    format: filmcraft_captions::Format,
    path: &str,
    name: Option<&str>,
    rate: filmcraft_time::FrameRate,
) -> filmcraft_captions::Result<PreparedCaptionImport> {
    let document = snap_document_for_sequence(filmcraft_captions::parse(bytes, format)?, rate);
    let stem = std::path::Path::new(path)
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Captions".into());
    Ok(PreparedCaptionImport {
        name: name.map(str::to_string).unwrap_or(stem),
        format: format.track_format(),
        language: "en".into(),
        enabled: true,
        locked: false,
        sync_lock: true,
        background_color: if format == filmcraft_captions::Format::Scc {
            [0, 0, 0, 255]
        } else {
            [0, 0, 0, 191]
        },
        document,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use filmcraft_captions::Cue;
    use serde_json::{json, Value};
    fn decode(value: &Value) -> Document {
        Document {
            cues: value["cues"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| Cue {
                    start: Tick(c["start"].as_i64().unwrap()),
                    end: Tick(c["end"].as_i64().unwrap()),
                    text: c["text"].as_str().unwrap().into(),
                    speaker: serde_json::from_value(c["speaker"].clone()).unwrap(),
                    id: serde_json::from_value(c["id"].clone()).unwrap(),
                    settings: c["settings"].as_str().unwrap().into(),
                })
                .collect(),
            blocks: serde_json::from_value(value["blocks"].clone()).unwrap(),
            warnings: serde_json::from_value(value["warnings"].clone()).unwrap(),
        }
    }
    #[test]
    fn caption_file_timing_matches_original_frames_and_export_ranges() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-caption-file-timing.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["commit"],
            json!("5231852443363f001c3f6b396dd9b1e6461ae2be")
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 6);
        let mut fractional = false;
        for case in cases {
            let input = decode(&case["input"]);
            let before = input.clone();
            let snapped = snap_document_for_sequence(
                input,
                serde_json::from_value(case["rate"].clone()).unwrap(),
            );
            assert_eq!(snapped, decode(&case["snapped"]));
            fractional |= snapped
                .cues
                .iter()
                .any(|c| c.start.0 % 254016 != 0 || c.end.0 % 254016 != 0);
            let output = document_for_range(&snapped, Tick(case["offset"].as_i64().unwrap()));
            assert_eq!(output, decode(&case["exported"]));
            assert_eq!(before, decode(&case["input"]));
            assert_eq!(snapped, decode(&case["snapped"]));
        }
        assert!(
            fractional,
            "must exercise submicrosecond source frame boundaries"
        );
    }
    #[test]
    fn caption_file_import_preparation_matches_original_seven_format_transactions() {
        use filmcraft_captions::{CaptionFormat, Format};
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/filmcraft-caption-file-import.json"
        ))
        .unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 14);
        for case in cases {
            assert_eq!(case["undoRestored"], true);
            let format = Format::from_name(case["format"].as_str().unwrap()).unwrap();
            let bytes: Vec<u8> = serde_json::from_value(case["bytes"].clone()).unwrap();
            let custom = case["customName"].as_bool().unwrap();
            let prepared = prepare_file_import(
                &bytes,
                format,
                &format!("/captions/talk.{}", format.extension()),
                custom.then_some("Custom captions"),
                serde_json::from_value(case["frameRate"].clone()).unwrap(),
            )
            .unwrap();
            let expected = &case["after"][0];
            assert_eq!(prepared.name, expected["name"]);
            assert_eq!(prepared.language, expected["language"]);
            assert_eq!(prepared.enabled, expected["enabled"]);
            assert_eq!(prepared.locked, expected["locked"]);
            assert_eq!(prepared.sync_lock, expected["sync_lock"]);
            let label = match prepared.format {
                CaptionFormat::Subtitle => "Subtitle",
                CaptionFormat::Cea608 => "Cea608",
                CaptionFormat::Cea708 => "Cea708",
                CaptionFormat::Teletext => "Teletext",
            };
            assert_eq!(label, expected["format"]);
            assert_eq!(
                json!(prepared.background_color),
                expected["style"]["background_color"]
            );
            assert_eq!(json!(prepared.document.blocks), expected["vtt_blocks"]);
            assert_eq!(
                json!(prepared.document.warnings),
                case["result"]["warnings"]
            );
            let cues = expected["captions"].as_array().unwrap();
            assert_eq!(prepared.document.cues.len(), cues.len());
            for (cue, expected) in prepared.document.cues.iter().zip(cues) {
                assert_eq!(cue.start.0, expected["start"].as_i64().unwrap());
                assert_eq!(
                    cue.end.0 - cue.start.0,
                    expected["duration"].as_i64().unwrap()
                );
                assert_eq!(cue.text, expected["text"]);
                assert_eq!(json!(cue.speaker), expected["speaker"]);
                assert_eq!(json!(cue.id), expected["cue_id"]);
                assert_eq!(cue.settings, expected["settings"]);
            }
        }
    }
}
