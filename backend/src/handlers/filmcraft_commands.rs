//! Source command names over owner-scoped native export and snapshot services.
use crate::{
    error::{ApiJson, AppError, AppResult},
    state::AppState,
};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    command: String,
    #[serde(default = "empty")]
    params: Value,
    #[serde(default, rename = "deadlineUnixMs")]
    deadline_unix_ms: Option<u64>,
}
fn empty() -> Value {
    json!({})
}
fn bad(error: impl std::fmt::Display) -> AppError {
    AppError::bad_request(error.to_string())
}
fn source_id(value: &Value) -> AppResult<Value> {
    let id = value
        .as_u64()
        .or_else(|| value.as_str().and_then(|value| value.parse::<u64>().ok()))
        .ok_or_else(|| bad("expected unsigned source ID (use decimal strings for large IDs)"))?;
    Ok(json!(id.to_string()))
}

async fn queue_add(state: &AppState, owner: &str, mut params: Value) -> AppResult<Value> {
    if params.get("input").is_some() {
        params["action"] = json!("add");
        return Ok(params);
    }
    // File outputs belong to authenticated native jobs. Do not silently pretend
    // a caller-selected source filesystem path was written.
    if params.get("path").is_some() {
        return Err(bad("path export is not supported by this transport; use the owned output returned by the job"));
    }
    let mut settings = if let Some(name) = params.get("preset").filter(|value| !value.is_null()) {
        let name = name.as_str().ok_or_else(|| bad("preset must be a name"))?;
        state
            .db
            .filmcraft_export_presets(owner)
            .await
            .map_err(|error| AppError::internal("read command presets", error))?
            .find(name)
            .ok_or_else(|| bad("unknown export preset"))?
            .settings
    } else {
        serde_json::to_value(
            crate::domain::filmcraft_export_preset_settings::ExportSettings::default(),
        )
        .map_err(bad)?
    };
    if let Some(patch) = params.get("settings").filter(|value| !value.is_null()) {
        if !patch.is_object() {
            return Err(bad("settings must be an object"));
        }
        super::filmcraft_export_presets::merge(&mut settings, patch.clone());
    }
    crate::domain::filmcraft_export_preset_overrides::apply(&mut settings, &params).map_err(bad)?;
    let settings =
        crate::domain::filmcraft_export_preset_settings::normalize(settings, true).map_err(bad)?;
    let mut input =
        crate::domain::filmcraft_export_preset_resolve::request_patch(&settings).map_err(bad)?;
    if let Some(name) = params.get("preset").filter(|value| !value.is_null()) {
        input["libraryPreset"] = name.clone();
    }
    input["document"] = params
        .get("document")
        .cloned()
        .ok_or_else(|| bad("document snapshot is required"))?;
    input["sources"] = params.get("sources").cloned().unwrap_or_else(empty);
    if let Some(sequence) = params.get("sequenceId").or_else(|| params.get("sequence")) {
        input["sequenceId"] = source_id(sequence)?;
    }
    let sequences = params
        .get("sequences")
        .map(|sequences| {
            sequences
                .as_array()
                .ok_or_else(|| bad("sequences must be an array"))?
                .iter()
                .map(source_id)
                .collect::<AppResult<Vec<_>>>()
        })
        .transpose()?;
    let times: serde_json::Map<String, Value> = [
        "startTime",
        "endTime",
        "startFrame",
        "endFrame",
        "startSeconds",
        "endSeconds",
        "startTimecode",
        "endTimecode",
    ]
    .into_iter()
    .filter_map(|key| params.get(key).map(|value| (key.to_owned(), value.clone())))
    .collect();
    let context_sequence = params
        .get("activeSequenceId")
        .or_else(|| params.get("sequenceId"))
        .or_else(|| params.get("sequence"))
        .map(source_id)
        .transpose()?;
    let playhead = params
        .get("playheadTime")
        .map(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|text| text.parse::<i64>().ok()))
                .map(|tick| json!(tick.to_string()))
                .ok_or_else(|| bad("playheadTime must be exact source ticks"))
        })
        .transpose()?;
    let supplied = params
        .get("ranges")
        .map(|ranges| {
            ranges
                .as_array()
                .ok_or_else(|| bad("ranges must be an array"))
        })
        .transpose()?;
    let ranges: Vec<Value> = match supplied {
        Some(ranges) if !ranges.is_empty() => ranges.clone(),
        _ => vec![params.get("range").cloned().unwrap_or(Value::Null)],
    }.into_iter().map(|range| {
        if range.get("startTicks").is_some() {range}
        else {json!({"source":range,"times":times,"contextSequenceId":context_sequence,"playheadTime":playhead})}
    }).collect();
    Ok(
        json!({"action":"add","input":input,"sequences":sequences,"ranges":ranges,"start":params.get("start").cloned().unwrap_or(json!(false)),"wait":params.get("wait").cloned().unwrap_or(json!(false))}),
    )
}

async fn quick(
    state: AppState,
    headers: HeaderMap,
    owner: String,
    mut params: Value,
) -> AppResult<Json<Value>> {
    if params.get("input").is_some() {
        return Err(bad("Quick Export uses explicit document/sequence params"));
    }
    let selected = params
        .get("sequenceId")
        .or_else(|| params.get("sequence"))
        .or_else(|| params.get("activeSequenceId"))
        .ok_or_else(|| bad("Quick Export needs an active sequence ID"))?;
    let sequence = source_id(selected)?;
    params["sequence"] = sequence.clone();
    let name = match params.get("preset").filter(|value| !value.is_null()) {
        Some(value) => value
            .as_str()
            .ok_or_else(|| bad("preset must be a name"))?
            .to_owned(),
        None => state
            .filmcraft_quick_presets
            .lock()
            .await
            .get(&owner)
            .cloned()
            .unwrap_or_else(|| crate::domain::filmcraft_export_presets::DEFAULT_PRESET.into()),
    };
    let library = state
        .db
        .filmcraft_export_presets(&owner)
        .await
        .map_err(|error| AppError::internal("read quick command presets", error))?;
    let name = library
        .find(&name)
        .ok_or_else(|| bad("unknown quick export preset"))?
        .name;
    params["preset"] = json!(name);
    let wait = params["wait"].as_bool().unwrap_or(false);
    params
        .as_object_mut()
        .ok_or_else(|| bad("command params must be an object"))?
        .remove("sequences");
    params.as_object_mut().unwrap().remove("ranges");
    let mut prepared = queue_add(&state, &owner, params).await?;
    let mut input = prepared
        .as_object_mut()
        .and_then(|object| object.remove("input"))
        .ok_or_else(|| bad("missing quick request"))?;
    let bytes = match input["document"].as_str() {
        Some(text) => text.as_bytes().to_vec(),
        None => serde_json::to_vec(&input["document"]).map_err(bad)?,
    };
    let document =
        crate::domain::filmcraft_project_file::SourceProjectDocument::read(&bytes).map_err(bad)?;
    let sequence = sequence.as_str().unwrap().parse::<u64>().map_err(bad)?;
    let range: super::filmcraft_export_queue::Range =
        serde_json::from_value(prepared["ranges"][0].clone()).map_err(bad)?;
    let range = match range {
        super::filmcraft_export_queue::Range::Exact(range) => range,
        super::filmcraft_export_queue::Range::Named(mode) => {
            crate::domain::filmcraft_quick_export_range::resolve(
                &document,
                sequence,
                Some(&mode),
                None,
            )
            .map_err(bad)?
        }
        super::filmcraft_export_queue::Range::Source(range) => {
            range.resolve(&document, sequence).map_err(bad)?
        }
    };
    input["sourceRange"] = serde_json::to_value(range).map_err(bad)?;
    let (_, Json(mut accepted)) =
        super::filmcraft_quick_export::enqueue(&state, headers, owner, name, input, wait).await?;
    accepted["job"] = accepted["jobId"].clone();
    accepted["preset"] = accepted["presetName"].clone();
    Ok(Json(accepted))
}

async fn formats(state: &AppState) -> AppResult<Json<Value>> {
    use crate::domain::{
        filmcraft_export_formats::FORMATS, filmcraft_export_preset_settings::ExportSettings,
    };
    let hevc_available=crate::tools::native_hevc_capability::available(&state.cpu_pool).await;
    let mut formats = Vec::new();
    for format in &FORMATS {
        let settings = serde_json::to_value(ExportSettings {
            format: format.format,
            ..Default::default()
        })
        .map_err(bad)?;
        let capability = crate::domain::filmcraft_export_preset_resolve::request_patch(&settings)
            .and_then(|patch| {
                let preset: super::filmcraft_video_export::ExportPreset =
                    serde_json::from_value(patch["preset"].clone())?;
                let native = preset.native_output();
                let available = if matches!(preset,super::filmcraft_video_export::ExportPreset::Hevc) {hevc_available} else {native
                    || state.tools.ffmpeg
                        && state
                            .tools
                            .ffmpeg_encoders
                            .iter()
                            .any(|encoder| encoder == preset.encoder())
                        && state
                            .tools
                            .ffmpeg_muxers
                            .iter()
                            .any(|muxer| muxer == preset.muxer())
                        && (!format.audio
                            || state
                                .tools
                                .ffmpeg_encoders
                                .iter()
                                .any(|encoder| encoder == preset.audio_codec()))};
                Ok((available, native, preset.extension()))
            });
        let (available, native, host_extension, reason) = match capability {
            Ok((available, native, extension)) => (
                available,
                native,
                Some(extension),
                if available {
                    None
                } else {
                    Some("required host encoder or muxer is unavailable".to_owned())
                },
            ),
            Err(error) => (false, false, None, Some(error.to_string())),
        };
        formats.push(json!({"id":format.format,"label":format.label,"extension":format.extension,"video":format.video,"audio":format.audio,"imageSequence":format.image_sequence,"available":available,"native":native,"hostExtension":host_extension,"unavailableReason":reason,"availabilityScope":"default format settings; export validates individual settings and source bindings"}));
    }
    Ok(Json(Value::Array(formats)))
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListParams {
    filter: Option<String>,
    enabled_only: Option<bool>,
}
fn catalog(params: Value) -> AppResult<Value> {
    let params: ListParams = serde_json::from_value(params).map_err(bad)?;
    let filter = params.filter.unwrap_or_default().to_ascii_lowercase();
    let _enabled_only = params.enabled_only.unwrap_or(false);
    let specs = [
        ("masks.track","Track Selected Mask","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?,selection?,selectedMask?},clip?:id,effect?:str|index,mask?:n,direction?:str,method?:str,frames?:n,wait?:bool,sources?:{sourceItem:libraryId}}"),
        ("jobs.list","List Jobs","{job?:trackingId,instance?:uuid,snapshot?:bool}; owner tracking and export jobs; requested snapshot includes exact documentText"),
        ("jobs.cancel","Cancel Job","{job:trackingId|exportUuid,instance?:uuid,discard?:bool}; tracking cancellation retains keys; discard explicitly removes only a finished tracking snapshot"),
        ("masks.add","Mask add","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?,selection?,selectedMask?},clip?:id,effect?:str|index,mask?:n,...sourceParams}; explicit snapshot and returned selectedMask context"),
        ("masks.remove","Mask remove","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?,selection?,selectedMask?},clip?:id,effect?:str|index,mask?:n,...sourceParams}; explicit snapshot and returned selectedMask context"),
        ("masks.set","Mask set","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?,selection?,selectedMask?},clip?:id,effect?:str|index,mask?:n,...sourceParams}; explicit snapshot and returned selectedMask context"),
        ("masks.moveVertex","Mask moveVertex","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?,selection?,selectedMask?},clip?:id,effect?:str|index,mask?:n,...sourceParams}; explicit snapshot and returned selectedMask context"),
        ("masks.translate","Mask translate","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?,selection?,selectedMask?},clip?:id,effect?:str|index,mask?:n,...sourceParams}; explicit snapshot and returned selectedMask context"),
        ("masks.addVertex","Mask addVertex","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?,selection?,selectedMask?},clip?:id,effect?:str|index,mask?:n,...sourceParams}; explicit snapshot and returned selectedMask context"),
        ("masks.removeVertex","Mask removeVertex","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?,selection?,selectedMask?},clip?:id,effect?:str|index,mask?:n,...sourceParams}; explicit snapshot and returned selectedMask context"),
        ("masks.toggleVertexSmooth","Mask toggleVertexSmooth","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?,selection?,selectedMask?},clip?:id,effect?:str|index,mask?:n,...sourceParams}; explicit snapshot and returned selectedMask context"),
        ("masks.list","Mask list","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?,selection?,selectedMask?},clip?:id,effect?:str|index,mask?:n,...sourceParams}; explicit snapshot and returned selectedMask context"),
        ("masks.select","Mask select","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?,selection?,selectedMask?},clip?:id,effect?:str|index,mask?:n,...sourceParams}; explicit snapshot and returned selectedMask context"),
        ("effects.toggleAnimation","Toggle Animation","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?},clip:id,effect?:str|index,param:str,mask?:n,time?:ticks,frame?:n}; returns edited documentText"),
        ("effects.addKeyframe","Add/Remove Keyframe","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?},clip:id,effect?:str|index,param:str,mask?:n,time?:ticks,frame?:n}; returns edited documentText"),
        ("effects.deleteKeyframe","Delete Keyframe","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?},clip:id,effect?:str|index,param:str,mask?:n,mediaTime?:ticks,to?:ticks,interpolation?:str,value?:any,inInfluence?:number,outInfluence?:number}; returns edited documentText"),
        ("effects.moveKeyframe","Move Keyframe","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?},clip:id,effect?:str|index,param:str,mask?:n,mediaTime?:ticks,to?:ticks,interpolation?:str,value?:any,inInfluence?:number,outInfluence?:number}; returns edited documentText"),
        ("effects.setInterpolation","Keyframe Interpolation","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?},clip:id,effect?:str|index,param:str,mask?:n,mediaTime?:ticks,to?:ticks,interpolation?:str,value?:any,inInfluence?:number,outInfluence?:number}; returns edited documentText"),
        ("effects.setKeyframe","Edit Keyframe","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?},clip:id,effect?:str|index,param:str,mask?:n,mediaTime?:ticks,to?:ticks,interpolation?:str,value?:any,inInfluence?:number,outInfluence?:number}; returns edited documentText"),
        ("effects.setParam","Set Effect Parameter","{document:object|string,sequence?:id,context?:{activeSequence?,playhead?},clip:id,effect?:str|index,param:str,value:any,mask?:n,time?:ticks,frame?:n,seconds?:n,timecode?:str,merge?:bool,begin?:bool}; returns edited documentText; explicit snapshot history"),
        ("effects.apply","Apply Effect","{document:object|string,sequence?:id,effect:str,clips?:[id],clip?:id,context?:{activeSequence?,selection?,playhead?,targetedTracks?},edge?:in|out,frames?:n,params?:object,reverse?:bool}; returns edited documentText"),
        ("effects.remove","Remove Effect","{document:object|string,sequence?:id,context?:{activeSequence?:id},clip:id,index:n}; returns edited document snapshot"),
        ("effects.toggleEnabled","Toggle Effect","{document:object|string,sequence?:id,context?:{activeSequence?:id},clip:id,index?:n}; returns edited document snapshot"),
        ("effects.reset","Reset Effect","{document:object|string,sequence?:id,context?:{activeSequence?:id},clip:id,index?:n}; returns edited document snapshot"),
        ("clip.rename","Rename","{document:object|string,name:str,item?:id,clip?:id,sequence?:id,context?:{activeSequence?:id,selection?:[id]}}; snapshot adoption required"),
        ("effects.list","List Effects","{kind?:Video|Audio|VideoTransition|AudioTransition,folder?:str,detail?:bool}; pinned source catalog, not render admission"),
        ("source.inspect","Inspect Source Monitor","{document:object|string,item?:id,context?:{sourceItem?:id,sourcePlayhead?:ticks,stillImageDuration?:number,stillImageUnit?:seconds|frames}}; exact large IDs/ticks use decimal strings"),
        ("sequence.inspect","Inspect Sequence","{document:object|string,item?:id,context?:{activeSequence?:id,playheads?:{id:ticks},selection?:[id]}}; exact large IDs/ticks use decimal strings"),
        ("project.inspect","Inspect Project","{document:object|string,context?:{path?,dirty?,revision?,activeSequence?:id,sourceItem?:id}}; read-only snapshot; large IDs/ticks are decimal strings"),
        ("export.presets.list", "List Export Presets", "{query?,category?,format?,favorites?:bool}"),
        ("export.presets.get", "Get Export Preset", "{name:str}"),
        ("export.presets.save", "Save Export Preset", "{name:str,from?:str,settings?:object,...flatSettings}"),
        ("export.presets.delete", "Delete Export Preset", "{name:str}"),
        ("export.presets.favorite", "Favorite Export Preset", "{name:str,favorite?:bool}"),
        ("export.presets.import", "Import Export Presets", "{file:portablePresetJSON}; filesystem paths unsupported"),
        ("export.presets.export", "Export Export Presets", "{names?:[str]}; returns portable JSON"),
        ("export.queue.add", "Add to Export Queue", "{document:object|string,sequence?:id,sequences?:[id],sources?:object,preset?:str,settings?:object,...flatSettings,range?:named|{mode?,startTime?,endTime?,startFrame?,endFrame?,startSeconds?,endSeconds?,startTimecode?,endTimecode?}|{startTicks,endTicks},ranges?:[range],activeSequenceId?:id,playheadTime?:ticks,start?:bool,wait?:bool}; owned outputs"),
        ("export.formats", "List Export Formats", "{}; source metadata and actual host default-format availability"),
        ("export.quick", "Quick Export", "{document:object|string,sequence?:id,activeSequenceId?:id,sources?:object,preset?:str,settings?:object,...flatSettings,range?:range,...start/endTime/Frame/Seconds/Timecode,playheadTime?:ticks,wait?:bool}; remembered preset; owned outputs"),
        ("export.queue.list", "List Export Queue", "{}"),
        ("export.queue.start", "Start Export Queue", "{wait?:bool}"),
        ("export.queue.stop", "Stop Export Queue", "{}"),
        ("export.queue.cancel", "Cancel Queued Export", "{id?:id,wait?:bool}"),
        ("export.queue.retry", "Retry Queued Export", "{id:id,start?:bool,wait?:bool}"),
        ("export.queue.remove", "Remove Queued Export", "{id:id}"),
        ("export.queue.move", "Reorder Queued Export", "{id:id,to?:index,by?:int}"),
        ("export.queue.clear", "Clear Finished Exports", "{all?:bool}"),
    ];
    Ok(Value::Array(specs.into_iter().filter(|(id,label,_)| id.to_ascii_lowercase().contains(&filter) || label.to_ascii_lowercase().contains(&filter))
        .map(|(id,label,params)|json!({"id":id,"label":label,"menu":[],"shortcut":null,"params":params,"enabled":true,"partial":true,"transport":"owner HTTP; explicit source snapshots and library bindings"})).collect()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchParams {
    steps: Vec<BatchStep>,
    stop_on_error: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BatchStep {
    id: String,
    params: Option<Value>,
}
const UNCONFIRMED: &str = "Command completion unconfirmed; inspect queue/presets before retrying";

fn deadline(unix_ms: Option<u64>) -> AppResult<Option<tokio::time::Instant>> {
    let Some(unix_ms) = unix_ms else {
        return Ok(None);
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(bad)?
        .as_millis();
    if u128::from(unix_ms) <= now {
        return Err(AppError::request_timeout(UNCONFIRMED));
    }
    let remaining = u64::try_from(u128::from(unix_ms) - now).map_err(bad)?;
    tokio::time::Instant::now()
        .checked_add(std::time::Duration::from_millis(remaining))
        .map(Some)
        .ok_or_else(|| bad("deadline is out of range"))
}
async fn run_one(
    state: AppState,
    headers: HeaderMap,
    request: Request,
    deadline: Option<tokio::time::Instant>,
) -> AppResult<Json<Value>> {
    let work = dispatch(State(state), headers, ApiJson(request));
    if let Some(deadline) = deadline {
        if tokio::time::Instant::now() >= deadline {
            return Err(AppError::request_timeout(UNCONFIRMED));
        }
        tokio::time::timeout_at(deadline, work)
            .await
            .map_err(|_| AppError::request_timeout(UNCONFIRMED))?
    } else {
        work.await
    }
}

pub async fn execute(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Request>,
) -> AppResult<Json<Value>> {
    super::publish::authenticated_actor(&state, &headers).await?;
    let deadline = deadline(request.deadline_unix_ms)?;
    if request.command != "command.batch" {
        return run_one(state, headers, request, deadline).await;
    }
    // Deserialize every step envelope before the first mutation, as the source
    // MCP BatchParams does. Command failures remain sequential and non-atomic.
    let batch: BatchParams = serde_json::from_value(request.params).map_err(bad)?;
    let mut completed = 0usize;
    let mut failed = 0usize;
    let mut results = Vec::new();
    for step in batch.steps {
        let expired = || deadline.is_some_and(|at| tokio::time::Instant::now() >= at);
        let result = if expired() || state.is_shutting_down() {
            Err(AppError::request_timeout(UNCONFIRMED))
        } else {
            run_one(
                state.clone(),
                headers.clone(),
                Request {
                    command: step.id,
                    params: step.params.unwrap_or_else(empty),
                    deadline_unix_ms: None,
                },
                deadline,
            )
            .await
        };
        match result {
            Ok(Json(result)) => {
                completed += 1;
                results.push(json!({"ok":true,"result":result}));
            }
            Err(error) => {
                failed += 1;
                results.push(json!({"ok":false,"error":error.public_message()}));
                if batch.stop_on_error.unwrap_or(true) || expired() || state.is_shutting_down() {
                    break;
                }
            }
        }
    }
    Ok(Json(
        json!({"completed":completed,"failed":failed,"results":results}),
    ))
}

async fn dispatch(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<Request>,
) -> AppResult<Json<Value>> {
    let owner = super::publish::authenticated_actor(&state, &headers).await?;
    if !request.params.is_object() {
        return Err(bad("command params must be an object"));
    }
    let mut params = request.params;
    if request.command=="masks.track" { return Ok(Json(super::filmcraft_tracking_jobs::start(&state,&headers,&owner,params).await?)); }
    if request.command=="jobs.list" { return Ok(Json(super::filmcraft_tracking_jobs::list(&state,&headers,&owner,&params).await?)); }
    if request.command=="jobs.cancel" { return Ok(Json(super::filmcraft_tracking_jobs::cancel(&state,&headers,&owner,&params).await?)); }
    if matches!(request.command.as_str(),"masks.add"|"masks.remove"|"masks.set"|"masks.moveVertex"|"masks.translate"|"masks.addVertex"|"masks.removeVertex"|"masks.toggleVertexSmooth"|"masks.list"|"masks.select") {
        let value=state.cpu_pool.execute(tokio_util::sync::CancellationToken::new(),move |_|crate::domain::filmcraft_mask_commands::run(&request.command,params)).await.map_err(bad)?;
        return Ok(Json(value));
    }
    if matches!(request.command.as_str(),"effects.toggleAnimation"|"effects.addKeyframe"|"effects.setParam"|"effects.deleteKeyframe"|"effects.moveKeyframe"|"effects.setInterpolation"|"effects.setKeyframe") {
        let value=state.cpu_pool.execute(tokio_util::sync::CancellationToken::new(),move |_|crate::domain::filmcraft_effect_set_param::edit_command(&request.command,params)).await.map_err(bad)?;
        return Ok(Json(value));
    }
    if request.command == "clip.rename" {
        let value = state.cpu_pool.execute(tokio_util::sync::CancellationToken::new(), move |_| crate::domain::filmcraft_clip_rename::rename(params)).await.map_err(bad)?;
        return Ok(Json(value));
    }
    if request.command=="effects.apply" {
        let value=state.cpu_pool.execute(tokio_util::sync::CancellationToken::new(),move |_|crate::domain::filmcraft_effect_apply::apply(params)).await.map_err(bad)?;
        return Ok(Json(value));
    }
    if matches!(request.command.as_str(),"effects.remove"|"effects.toggleEnabled"|"effects.reset") {
        let value=state.cpu_pool.execute(tokio_util::sync::CancellationToken::new(),move |_|crate::domain::filmcraft_effect_edit::edit(&request.command,params)).await.map_err(bad)?;
        return Ok(Json(value));
    }
    if matches!(request.command.as_str(),"project.inspect"|"sequence.inspect"|"source.inspect"|"effects.list") {
        let value=state.cpu_pool.execute(tokio_util::sync::CancellationToken::new(),move |_|if request.command=="effects.list" {Ok(crate::domain::filmcraft_effect_catalog::list(&params))}else if request.command=="source.inspect" {crate::domain::filmcraft_source_inspect::inspect(params)}else if request.command=="sequence.inspect" {crate::domain::filmcraft_sequence_inspect::inspect(params)}else{crate::domain::filmcraft_project_inspect::inspect(params)}).await.map_err(bad)?;
        return Ok(Json(value));
    }
    if request.command == "export.formats" {
        return formats(&state).await;
    }
    if request.command == "export.quick" {
        return quick(state, headers, owner, params).await;
    }
    if request.command == "command.list" {
        return Ok(Json(catalog(params)?));
    }
    if let Some(action) = request.command.strip_prefix("export.presets.") {
        if action == "list" {
            let filters = serde_json::from_value(params).map_err(bad)?;
            return super::filmcraft_export_presets::list(State(state), headers, Query(filters))
                .await;
        }
        if !["get", "save", "delete", "favorite", "import", "export"].contains(&action) {
            return Err(bad("unknown Filmcraft command"));
        }
        if params.get("path").is_some() {
            return Err(bad(
                "preset file transport uses JSON file bodies, not server filesystem paths",
            ));
        }
        params["action"] = json!(action);
        let command = serde_json::from_value(params).map_err(bad)?;
        return super::filmcraft_export_presets::command(State(state), headers, ApiJson(command))
            .await;
    }
    if let Some(action) = request.command.strip_prefix("export.queue.") {
        if action == "list" {
            return super::filmcraft_export_queue::list(State(state), headers).await;
        }
        if ![
            "add", "start", "stop", "cancel", "retry", "remove", "move", "clear",
        ]
        .contains(&action)
        {
            return Err(bad("unknown Filmcraft command"));
        }
        if action == "add" {
            params = queue_add(&state, &owner, params).await?;
        } else {
            if let Some(id) = params.get("id").filter(|id| !id.is_null()) {
                params["id"] = source_id(id)?;
            }
            params["action"] = json!(action);
        }
        let command = serde_json::from_value(params).map_err(bad)?;
        return super::filmcraft_export_queue::command(State(state), headers, ApiJson(command))
            .await;
    }
    Err(bad("unknown Filmcraft command"))
}
