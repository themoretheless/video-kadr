//! Project Find queries from Filmcraft (MIT, pinned 52318524).
use super::{
    filmcraft_project_file::SourceProjectDocument,
    filmcraft_project_index::ProjectIndex,
    filmcraft_project_inspect::duration,
    filmcraft_project_sort::{item_media, item_rate, keys, timecode},
};
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

const BUILTINS: &[&str] = &[
    "Name",
    "Label",
    "Media Type",
    "Frame Rate",
    "Media Duration",
    "Video Info",
    "Audio Info",
    "File Path",
    "Tape Name",
    "Description",
    "Comment",
    "Log Note",
    "Scene",
    "Shot",
];

fn all() -> String {
    "All".into()
}
fn yes() -> bool {
    true
}

#[derive(Deserialize, Serialize)]
struct Row {
    #[serde(default = "all")]
    column: String,
    #[serde(default, rename = "op", alias = "operator")]
    operator: Option<String>,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    document: Value,
    rows: Option<Vec<Row>>,
    #[serde(flatten)]
    row: Row,
    #[serde(default = "yes")]
    match_all: bool,
    #[serde(default)]
    case_sensitive: bool,
    #[serde(default)]
    index: usize,
    scope: Option<String>,
    sequence: Option<Value>,
    #[serde(rename = "in")]
    within: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub(super) struct Query {
    rows: Vec<Row>,
    #[serde(default = "yes", alias = "matchAll")]
    match_all: bool,
    #[serde(default, alias = "caseSensitive")]
    case_sensitive: bool,
}

impl Query {
    pub(super) fn first_text(&self) -> Option<&str> {
        self.rows
            .iter()
            .map(|row| row.text.as_str())
            .find(|text| !text.is_empty())
    }

    pub(super) fn normalized(mut self) -> Result<Self> {
        for row in &mut self.rows {
            row.operator = Some(Operator::parse(row.operator.as_deref())?.name().into());
        }
        Ok(self)
    }

    pub(super) fn browser_view(&self) -> Value {
        let rows: Vec<_> = self.rows.iter().map(|row| json!({
            "column":row.column,"operator":row.operator.as_deref().unwrap_or("contains"),"text":row.text
        })).collect();
        json!({"rows":rows,"matchAll":self.match_all,"caseSensitive":self.case_sensitive})
    }
}

#[derive(Clone, Copy)]
enum Operator {
    Contains,
    Matches,
    BeginsWith,
    EndsWith,
    DoesNotContain,
}

impl Operator {
    fn parse(name: Option<&str>) -> Result<Self> {
        Ok(
            match name
                .unwrap_or("contains")
                .to_ascii_lowercase()
                .replace([' ', '_', '-'], "")
                .as_str()
            {
                "contains" => Self::Contains,
                "matches" => Self::Matches,
                "beginswith" => Self::BeginsWith,
                "endswith" => Self::EndsWith,
                "doesnotcontain" => Self::DoesNotContain,
                _ => bail!("unknown Filmcraft Find operator"),
            },
        )
    }

    fn name(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Matches => "matches",
            Self::BeginsWith => "beginsWith",
            Self::EndsWith => "endsWith",
            Self::DoesNotContain => "doesNotContain",
        }
    }

    fn matches(self, text: &str, query: &str) -> bool {
        match self {
            Self::Contains => text.contains(query),
            Self::Matches => text == query,
            Self::BeginsWith => text.starts_with(query),
            Self::EndsWith => text.ends_with(query),
            Self::DoesNotContain => !text.contains(query),
        }
    }
}

fn column_text(
    item: &Value,
    cells: &Value,
    media: Option<&Value>,
    rate: filmcraft_time::FrameRate,
    column: &str,
) -> String {
    let string = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    match column.to_ascii_lowercase().as_str() {
        "name" => string(&item["name"]),
        "label" => string(&item["label"]),
        "media type" | "type" => string(&cells["mediaType"]),
        "frame rate" if cells["frameRate"].as_f64().unwrap_or_default() > 0.0 => {
            format!("{} fps", rate.label())
        }
        "frame rate" => String::new(),
        "media duration" | "duration" => {
            timecode(i128::from(duration(item).unwrap_or_default()), rate)
        }
        "video info" => {
            media
                .filter(|m| !m["info"]["video"].is_null())
                .map_or_else(String::new, |_| {
                    format!(
                        "{} {}",
                        string(&cells["videoInfo"]),
                        string(&cells["videoCodec"])
                    )
                })
        }
        "audio info" => {
            let audio = media.map_or(&Value::Null, |m| &m["info"]["audio"]);
            match (audio["sample_rate"].as_u64(), audio["channels"].as_u64()) {
                (Some(rate), Some(channels)) => {
                    format!("{rate} Hz - {channels} ch {}", string(&cells["audioCodec"]))
                }
                _ => String::new(),
            }
        }
        "file path" | "path" => string(&cells["filePath"]),
        _ => item["metadata"]
            .as_object()
            .into_iter()
            .flatten()
            .find(|(name, _)| name.eq_ignore_ascii_case(column))
            .map_or_else(String::new, |(_, value)| string(value)),
    }
}

pub fn find(command: &str, params: Value) -> Result<Value> {
    let request: Request = serde_json::from_value(params)?;
    let scope = request
        .scope
        .as_deref()
        .unwrap_or("project")
        .to_ascii_lowercase();
    ensure!(
        matches!(scope.as_str(), "project" | "timeline"),
        "invalid Find scope"
    );
    let query = Query {
        rows: request.rows.unwrap_or_else(|| vec![request.row]),
        match_all: request.match_all,
        case_sensitive: request.case_sensitive,
    };
    ensure!(query.first_text().is_some(), "enter text to find");
    let bytes = match request.document {
        Value::String(text) => text.into_bytes(),
        value => serde_json::to_vec(&value)?,
    };
    let document = SourceProjectDocument::read(&bytes)?;
    let next = if command == "edit.findNext" {
        request.index.saturating_add(1)
    } else {
        0
    };
    if scope == "timeline" {
        let sequence = super::filmcraft_project_inspect::id(request.sequence)?
            .or_else(|| document.project()["active_sequence"].as_u64());
        let within = request
            .within
            .as_deref()
            .unwrap_or("all")
            .to_ascii_lowercase();
        return timeline_find(&document, &query, sequence, &within, next);
    }
    let hits = matching_ids(&document, &query)?;
    let selected = next % hits.len().max(1);
    Ok(json!({"item":hits.get(selected),"index":selected,"matches":hits.len(),"items":hits}))
}

fn prepare(query: &Query) -> Result<Vec<(&Row, Operator, String)>> {
    let rows = query
        .rows
        .iter()
        .map(|row| {
            let operator = Operator::parse(row.operator.as_deref())?;
            let text = if query.case_sensitive {
                row.text.clone()
            } else {
                row.text.to_lowercase()
            };
            Ok((row, operator, text))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter(|(_, _, text)| !text.is_empty())
        .collect())
}

fn timeline_find(
    document: &SourceProjectDocument,
    query: &Query,
    sequence: Option<u64>,
    within: &str,
    next: usize,
) -> Result<Value> {
    use anyhow::Context;
    let index = ProjectIndex::new(document)?;
    let sequence = sequence.context("select a sequence to search")?;
    let item = index.item(sequence).context("sequence not found")?;
    let timeline = item
        .pointer("/kind/Sequence")
        .context("Find target is not a sequence")?;
    let rate = item_rate(&index, sequence, item);
    let active = prepare(query)?;
    let test = |text: &str| {
        let folded;
        let text = if query.case_sensitive {
            text
        } else {
            folded = text.to_lowercase();
            &folded
        };
        let mut matches = active
            .iter()
            .map(|(_, op, needle)| op.matches(text, needle));
        if query.match_all {
            matches.all(|hit| hit)
        } else {
            matches.any(|hit| hit)
        }
    };
    let mut hits = Vec::new();
    if within != "markers" {
        for kind in ["video_tracks", "audio_tracks"] {
            for track in timeline[kind].as_array().into_iter().flatten() {
                for clip in track["items"].as_array().into_iter().flatten() {
                    let name = clip["name"]
                        .as_str()
                        .filter(|name| !name.is_empty())
                        .or_else(|| index.item(clip["item"].as_u64()?)?["name"].as_str())
                        .unwrap_or_default();
                    if test(name) {
                        if let (Some(id), Some(time)) =
                            (clip["id"].as_u64(), clip["start"].as_i64())
                        {
                            hits.push((time, "clip", id, name));
                        }
                    }
                }
            }
        }
    }
    if within != "clips" {
        for marker in timeline["markers"].as_array().into_iter().flatten() {
            let name = marker["name"].as_str().unwrap_or_default();
            let comment = marker["comment"].as_str().unwrap_or_default();
            if test(name) || (!comment.is_empty() && test(comment)) {
                if let (Some(id), Some(time)) = (marker["id"].as_u64(), marker["start"].as_i64()) {
                    hits.push((time, "marker", id, name));
                }
            }
        }
    }
    hits.sort_by_key(|hit| hit.0);
    let current = next % hits.len().max(1);
    if let Some(&(time, kind, id, name)) = hits.get(current) {
        let mut result = json!({"sequence":sequence.to_string(),"matches":hits.len(),"index":current,
            "name":name,"time":time.to_string(),"timeFrames":rate.frame_at(filmcraft_time::Tick(time)).to_string(),
            "timecode":timecode(i128::from(time), rate)});
        result[kind] = json!(id.to_string());
        Ok(result)
    } else {
        Ok(json!({"sequence":sequence.to_string(),"matches":0,"index":0}))
    }
}

pub(super) fn matching_ids(document: &SourceProjectDocument, query: &Query) -> Result<Vec<String>> {
    let active = prepare(query)?;
    if active.is_empty() {
        return Ok(vec![]);
    }
    let index = ProjectIndex::new(document)?;
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    let mut pending = vec![&document.project()["root"]];
    while let Some(entry) = pending.pop() {
        if let Some(id) = entry["Item"].as_u64() {
            if seen.insert(id) {
                order.push(id);
            }
        } else {
            let bin = entry.get("Bin").unwrap_or(entry);
            pending.extend(bin["children"].as_array().into_iter().flatten().rev());
        }
    }
    order.extend(
        index
            .items()
            .map(|(id, _)| id)
            .filter(|id| !seen.contains(id)),
    );
    let hits: Vec<String> = order
        .into_iter()
        .filter(|id| {
            let Some(item) = index.item(*id) else {
                return false;
            };
            if item.pointer("/kind/Graphic").is_some() {
                return false;
            }
            let cells = keys(&index, *id, item, [0; 2]);
            let media = item_media(&index, item);
            let rate = if item.pointer("/kind/Subclip").is_some() {
                filmcraft_time::FrameRate::default()
            } else {
                item_rate(&index, *id, item)
            };
            let mut matches = active.iter().map(|(row, operator, needle)| {
                let test = |text: &str| {
                    if query.case_sensitive {
                        operator.matches(text, needle)
                    } else {
                        operator.matches(&text.to_lowercase(), needle)
                    }
                };
                if row.column.is_empty() || row.column.eq_ignore_ascii_case("All") {
                    let mut values = BUILTINS
                        .iter()
                        .map(|column| column_text(item, &cells, media, rate, column))
                        .chain(
                            item["metadata"]
                                .as_object()
                                .into_iter()
                                .flatten()
                                .filter_map(|(_, v)| v.as_str().map(str::to_owned)),
                        );
                    return match operator {
                        Operator::DoesNotContain => values.all(|text| test(&text)),
                        _ => values.any(|text| test(&text)),
                    };
                }
                test(&column_text(item, &cells, media, rate, &row.column))
            });
            if query.match_all {
                matches.all(|hit| hit)
            } else {
                matches.any(|hit| hit)
            }
        })
        .map(|id| id.to_string())
        .collect();
    Ok(hits)
}
