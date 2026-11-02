//! Lossless caption-file transport. Decimal strings preserve Filmcraft's signed
//! 254016000000-Hz ticks across JavaScript JSON boundaries.
use crate::{
    domain::graphic_template_fonts::{base64_decode, base64_encode},
    error::{ApiJson, AppError, AppResult},
};
use axum::Json;
use filmcraft_captions::{Cue, Document, Format, WriteOptions};
use filmcraft_time::{FrameRate, Tick, TICKS_PER_SECOND};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileCue {
    pub start_tick: String,
    pub end_tick: String,
    pub text: String,
    #[serde(default)]
    pub speaker: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub settings: String,
}
#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileDocument {
    pub ticks_per_second: String,
    pub cues: Vec<FileCue>,
    #[serde(default)]
    pub blocks: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}
impl From<Document> for FileDocument {
    fn from(doc: Document) -> Self {
        Self {
            ticks_per_second: TICKS_PER_SECOND.to_string(),
            cues: doc
                .cues
                .into_iter()
                .map(|c| FileCue {
                    start_tick: c.start.0.to_string(),
                    end_tick: c.end.0.to_string(),
                    text: c.text,
                    speaker: c.speaker,
                    id: c.id,
                    settings: c.settings,
                })
                .collect(),
            blocks: doc.blocks,
            warnings: doc.warnings,
        }
    }
}
impl TryFrom<FileDocument> for Document {
    type Error = anyhow::Error;
    fn try_from(doc: FileDocument) -> anyhow::Result<Self> {
        anyhow::ensure!(
            doc.ticks_per_second == TICKS_PER_SECOND.to_string(),
            "unsupported caption timebase"
        );
        let cues = doc
            .cues
            .into_iter()
            .map(|c| {
                Ok(Cue {
                    start: Tick(c.start_tick.parse()?),
                    end: Tick(c.end_tick.parse()?),
                    text: c.text,
                    speaker: c.speaker,
                    id: c.id,
                    settings: c.settings,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self {
            cues,
            blocks: doc.blocks,
            warnings: doc.warnings,
        })
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadRequest {
    pub data_base64: String,
    pub format: Option<String>,
    pub extension: Option<String>,
    pub snap_to_rate: Option<FrameRate>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResponse {
    pub format: String,
    pub document: FileDocument,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteRequest {
    pub format: String,
    pub document: FileDocument,
    #[serde(default = "drop_frame_default")]
    pub drop_frame: bool,
    pub rate: Option<FrameRate>,
    pub offset_tick: Option<String>,
}
fn drop_frame_default() -> bool {
    true
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResponse {
    pub extension: String,
    pub data_base64: String,
}
fn read_file(request: ReadRequest) -> anyhow::Result<ReadResponse> {
    let bytes = base64_decode(&request.data_base64)
        .ok_or_else(|| anyhow::anyhow!("invalid caption base64"))?;
    let format = match request.format {
        Some(name) => {
            Format::from_name(&name).ok_or_else(|| anyhow::anyhow!("unsupported caption format"))?
        }
        None => filmcraft_captions::detect(&bytes, request.extension.as_deref())
            .ok_or_else(|| anyhow::anyhow!("unrecognized caption file"))?,
    };
    let document = filmcraft_captions::parse(&bytes, format)?;
    let document = match request.snap_to_rate {
        Some(rate) => {
            crate::domain::caption_file_timing::snap_document_for_sequence(document, rate)
        }
        None => document,
    };
    Ok(ReadResponse {
        format: format.extension().into(),
        document: document.into(),
    })
}
fn write_file(request: WriteRequest) -> anyhow::Result<WriteResponse> {
    let format = Format::from_name(&request.format)
        .ok_or_else(|| anyhow::anyhow!("unsupported caption format"))?;
    let document = Document::try_from(request.document)?;
    let document = match request.offset_tick {
        Some(offset) => {
            crate::domain::caption_file_timing::document_for_range(&document, Tick(offset.parse()?))
        }
        None => document,
    };
    let bytes = filmcraft_captions::write(
        &document,
        format,
        WriteOptions {
            drop_frame: request.drop_frame,
            rate: request.rate,
        },
    );
    Ok(WriteResponse {
        extension: format.extension().into(),
        data_base64: base64_encode(&bytes),
    })
}
pub async fn read(ApiJson(request): ApiJson<ReadRequest>) -> AppResult<Json<ReadResponse>> {
    read_file(request)
        .map(Json)
        .map_err(|e| AppError::bad_request(e.to_string()))
}
pub async fn write(ApiJson(request): ApiJson<WriteRequest>) -> AppResult<Json<WriteResponse>> {
    write_file(request)
        .map(Json)
        .map_err(|e| AppError::bad_request(e.to_string()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caption_files_preserve_exact_signed_ticks_metadata_and_all_codecs() {
        let original = Document {
            cues: vec![Cue {
                start: Tick(-9007199254740993),
                end: Tick(9007199254740993),
                text: "Hello".into(),
                speaker: Some("Voice".into()),
                id: Some("cue-id".into()),
                settings: "align:start".into(),
            }],
            blocks: vec!["NOTE original".into()],
            warnings: vec!["warning".into()],
        };
        let wire: FileDocument = serde_json::from_str(
            &serde_json::to_string(&FileDocument::from(original.clone())).unwrap(),
        )
        .unwrap();
        assert_eq!(Document::try_from(wire).unwrap(), original);
        let source = Document {
            cues: vec![Cue {
                start: Tick(TICKS_PER_SECOND),
                end: Tick(TICKS_PER_SECOND * 3),
                text: "Hello".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        for format in Format::ALL {
            let expected = filmcraft_captions::write(&source, format, WriteOptions::default());
            let output = write_file(WriteRequest {
                format: format.extension().into(),
                document: source.clone().into(),
                drop_frame: true,
                rate: None,
                offset_tick: None,
            })
            .unwrap();
            assert_eq!(
                base64_decode(&output.data_base64).unwrap(),
                expected,
                "{}",
                format.extension()
            );
            let parsed = read_file(ReadRequest {
                data_base64: output.data_base64,
                format: Some(format.extension().into()),
                extension: None,
                snap_to_rate: None,
            })
            .unwrap();
            assert_eq!(
                Document::try_from(parsed.document).unwrap(),
                filmcraft_captions::parse(&expected, format).unwrap()
            );
        }
        assert!(read_file(ReadRequest {
            data_base64: "%%%".into(),
            format: None,
            extension: None,
            snap_to_rate: None
        })
        .is_err());
        let mut invalid = FileDocument::from(source);
        invalid.ticks_per_second = "1000000".into();
        assert!(Document::try_from(invalid).is_err());
    }
    #[tokio::test]
    async fn caption_files_http_preserves_vtt_metadata_and_rejects_bad_timebase() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
            routing::post,
            Router,
        };
        use http_body_util::BodyExt;
        use serde_json::{json, Value};
        use tower::ServiceExt;
        async fn send(path: &str, value: Value) -> (StatusCode, Value) {
            let response = Router::new()
                .route("/read", post(read))
                .route("/write", post(write))
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(path)
                        .header("content-type", "application/json")
                        .body(Body::from(value.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            (status, serde_json::from_slice(&bytes).unwrap())
        }
        let vtt="WEBVTT\n\nNOTE original\n\ncue-1\n00:00:01.000 --> 00:00:03.000 align:start\n<v Voice>Hello\n";
        let (status, parsed) = send(
            "/read",
            json!({"dataBase64":base64_encode(vtt.as_bytes()),"extension":"vtt"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(parsed["format"], "vtt");
        let (status,snapped)=send("/read",json!({"dataBase64":base64_encode(vtt.as_bytes()),"snapToRate":{"num":24000,"den":1001}})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(snapped["document"]["cues"][0]["startTick"], "254270016000");

        assert_eq!(
            parsed["document"]["cues"][0]["startTick"],
            TICKS_PER_SECOND.to_string()
        );
        assert_eq!(parsed["document"]["cues"][0]["id"], "cue-1");
        assert_eq!(parsed["document"]["cues"][0]["speaker"], "Voice");
        assert_eq!(parsed["document"]["blocks"][0], "NOTE original");
        let (status, written) = send(
            "/write",
            json!({"format":"vtt","document":parsed["document"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, reparsed) = send(
            "/read",
            json!({"dataBase64":written["dataBase64"],"format":"vtt"}),
        )
        .await;
        assert_eq!(reparsed["document"], parsed["document"]);
        let mut invalid = parsed["document"].clone();
        invalid["ticksPerSecond"] = json!("1000000");
        assert_eq!(
            send("/write", json!({"format":"vtt","document":invalid}))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            send("/read", json!({"dataBase64":"%%%"})).await.0,
            StatusCode::BAD_REQUEST
        );
    }
}
