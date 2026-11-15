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
    ensure!(
        request
            .scope
            .as_deref()
            .is_none_or(|scope| scope.eq_ignore_ascii_case("project")),
        "Find requires project scope"
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
    let hits = matching_ids(&document, &query)?;
    let next = if command == "edit.findNext" {
        request.index.saturating_add(1)
    } else {
        0
    };
    let selected = next % hits.len().max(1);
    Ok(json!({"item":hits.get(selected),"index":selected,"matches":hits.len(),"items":hits}))
}

pub(super) fn matching_ids(document: &SourceProjectDocument, query: &Query) -> Result<Vec<String>> {
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
    let active: Vec<_> = rows
        .into_iter()
        .filter(|(_, _, text)| !text.is_empty())
        .collect();
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
