//! Lossless source-project transport and bounded native audio downloads.
use crate::{domain::{filmcraft_project_file::SourceProjectDocument,filmcraft_project_index::ProjectIndex,
    filmcraft_project_audio::SourceProjectAudioRuntime,filmcraft_pcm_export::{export_pcm_with_reader,PcmFormat},filmcraft_wav_audio::SourceWavPool},
    error::{ApiJson,AppError,AppResult},state::AppState,runtime::cpu_pool::CpuTaskError};
use axum::{extract::State,http::{HeaderMap,header},response::{IntoResponse,Response},Json};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::collections::BTreeMap;
use tokio_util::sync::CancellationToken;
fn decode(document:&Value)->AppResult<SourceProjectDocument> {
    let bytes=match document {Value::String(text)=>text.as_bytes().to_vec(),_=>serde_json::to_vec(document).map_err(|e|AppError::bad_request(e.to_string()))?};
    SourceProjectDocument::read(&bytes).map_err(|e|AppError::bad_request(e.to_string()))
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ImportRequest { document:Value }
pub async fn import(ApiJson(request):ApiJson<ImportRequest>) -> AppResult<Json<Value>> {
    let document=decode(&request.document)?;
    manifest(&document).map(Json)
}
fn manifest(document:&SourceProjectDocument)->AppResult<Value> {
    let index=ProjectIndex::new(document).map_err(|e|AppError::bad_request(e.to_string()))?;
    let sequences:Vec<_>=index.sequences().map(|(id,sequence)| {
        let end= ["audio_tracks","video_tracks"].into_iter().flat_map(|key|sequence[key].as_array().into_iter().flatten())
            .flat_map(|track|track["items"].as_array().into_iter().flatten())
            .filter_map(|clip|Some(clip["start"].as_i64()? as i128+clip["duration"].as_i64()? as i128)).max().unwrap_or(0).max(0);
        let clips:Vec<_>=["video_tracks","audio_tracks"].into_iter().flat_map(|kind|sequence[kind].as_array().into_iter().flatten().map(move |track|(kind,track)))
            .flat_map(|(kind,track)|track["items"].as_array().into_iter().flatten().map(move |clip|json!({
                "id":clip["id"].as_u64().map(|v|v.to_string()),"name":clip["name"],"label":clip["label"],
                "trackId":track["id"].as_u64().map(|v|v.to_string()),"trackKind":kind,
                "locked":track["locked"].as_bool().unwrap_or(false),
                "linkId":clip["link"].as_u64().map(|v|v.to_string()),
                "startTicks":clip["start"].as_i64().map(|v|v.to_string()),
                "durationTicks":clip["duration"].as_i64().map(|v|v.to_string())
            }))).collect();
        let transitions:Vec<_>=["video_tracks","audio_tracks"].into_iter().flat_map(|kind|sequence[kind].as_array().into_iter().flatten().map(move |track|(kind,track)))
            .flat_map(|(kind,track)|track["transitions"].as_array().into_iter().flatten().map(move |transition|json!({
                "id":transition["id"].as_u64().map(|v|v.to_string()),"trackId":track["id"].as_u64().map(|v|v.to_string()),"trackKind":kind,
                "effect":transition["effect"]["effect"],"reverse":transition["reverse"].as_bool().unwrap_or(false),
                "startTicks":transition["start"].as_i64().map(|v|v.to_string()),"durationTicks":transition["duration"].as_i64().map(|v|v.to_string()),
                "paramsText":serde_json::to_string(&transition["effect"]["params"]).unwrap()
            }))).collect();
        let rate=serde_json::from_value::<filmcraft_time::FrameRate>(sequence["settings"]["frame_rate"].clone()).unwrap_or_default().sane();
        let den=254_016_000_000i128*rate.den as i128;let frames=(end*rate.num as i128+den-1)/den;
        let tracks:Vec<_>=["video_tracks","audio_tracks"].into_iter().flat_map(|kind|sequence[kind].as_array().into_iter().flatten().map(move |track|json!({"id":track["id"].as_u64().map(|id|id.to_string()),"name":track["name"],"kind":kind,"locked":track["locked"].as_bool().unwrap_or(false)}))).collect();
        json!({"tracks":tracks,"marks":crate::domain::filmcraft_marks::sequence_marks(sequence),"transitions":transitions,"clips":clips,"id":id.to_string(),"name":index.item(id).and_then(|v|v.get("name")),"settings":sequence["settings"],"sampleRate":sequence["settings"]["sample_rate"],"durationSeconds":end as f64/254_016_000_000.,"durationFrames":frames.to_string()})
    }).collect();
    let project_items:Vec<_>=index.items().map(|(id,item)|json!({
        "id":id.to_string(),"name":item["name"],"label":item["label"],"sourceMarks":crate::domain::filmcraft_marks::source_marks(item),"subclip":crate::domain::filmcraft_subclip::describe(item),
        "kind":item["kind"].as_object().and_then(|kind|kind.keys().next())
    })).collect();
    let media:Vec<_>=document.project()["items"].as_object().into_iter().flat_map(|items|items.iter()).filter_map(|(id,item)| {
        let file=item.pointer("/kind/Media/media/File/path")?.as_str()?;
        Some(json!({"id":id,"name":item["name"],"path":file,"offline":item.pointer("/kind/Media/offline").and_then(Value::as_bool).unwrap_or(false)}))
    }).collect();
    Ok(json!({"documentText":String::from_utf8(document.encode_preserved().map_err(|e|AppError::bad_request(e.to_string()))?).map_err(|e|AppError::bad_request(e.to_string()))?,"document":document.envelope,"originalSchema":document.original_schema,"sequences":sequences,"media":media,"projectItems":project_items,"labelPalette":crate::domain::filmcraft_labels::palette(),"projectBins":crate::domain::filmcraft_bin_rename::list(document).map_err(|e|AppError::bad_request(e.to_string()))?}))
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct AudioExportRequest {
    document:Value, sequence_id:String, start_sample:i64, frames:usize,
    channels:u16,bits:u16,format:String,
    #[serde(default)] sources:BTreeMap<String,String>,
}
pub async fn export_audio(State(state):State<AppState>,headers:HeaderMap,ApiJson(request):ApiJson<AudioExportRequest>)->AppResult<Response> {
    if request.frames>4_194_304 || request.sources.len()>64 || !matches!(request.channels,1|2) || !matches!(request.bits,16|24) {
        return Err(AppError::bad_request("invalid audio export range/format"));
    }
    let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
    let (format,content_type,name)=match request.format.as_str(){"wav"=>(PcmFormat::Wav,"audio/wav","sequence.wav"),"aiff"=>(PcmFormat::Aiff,"audio/aiff","sequence.aiff"),_=>return Err(AppError::bad_request("unsupported audio export format"))};
    let document=decode(&request.document)?;
    let index=ProjectIndex::new(&document).map_err(|e|AppError::bad_request(e.to_string()))?;
    let mut paths=vec![];
    for (id,source_id) in request.sources {
        let id=id.parse::<u64>().map_err(|_|AppError::bad_request("invalid source item ID"))?;
        let media=index.resolve_media(id).filter(|m|m.item_id==id).ok_or_else(||AppError::bad_request("binding must identify root media"))?;
        if media.media.pointer("/media/File").is_none(){return Err(AppError::bad_request("binding must identify file media"));}
        super::library::authorize_source_access(&state,&source_id,&headers).await?;
        let entry=state.library.get(&source_id).await.filter(|e|e.kind=="source").ok_or_else(||AppError::not_found("media source not found"))?;
        let path=state.library.resolve_media_path(&entry).await.map_err(|_|AppError::not_found("media source not found"))?;
        paths.push((id,path));
    }
    let data=state.cpu_pool.execute(CancellationToken::new(),move |cancel| {
        let mut sources=SourceWavPool::default();let mut remaining=128*1024*1024u64;
        for (id,path) in paths {
            anyhow::ensure!(!cancel.is_cancelled(),"audio export cancelled");
            let size=std::fs::metadata(&path)?.len();anyhow::ensure!(size<=remaining,"audio source budget exceeded");
            sources.bind_file(id,&path,size.max(1))?;remaining-=size;
        }
        let mut runtime=SourceProjectAudioRuntime::new(document)?;
        export_pcm_with_reader(&mut runtime,sequence,request.start_sample,request.frames,request.channels,request.bits,format,|id,start,frames,rate| {
            anyhow::ensure!(!cancel.is_cancelled(),"audio export cancelled");
            let audio=sources.read(id,start,frames,rate)?;
            anyhow::ensure!(audio.is_some(),"unbound file media {id}");Ok(audio)
        })
    }).await.map_err(|error|match error {
        CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),
        CpuTaskError::Saturated=>AppError::service_unavailable("audio export queue is full"),
        error=>AppError::internal("source audio export",anyhow::anyhow!(error)),
    })?;
    Ok(([(header::CONTENT_TYPE,content_type),(header::CONTENT_DISPOSITION,&format!("attachment; filename=\"{name}\""))],data).into_response())
}

#[derive(Deserialize)]
#[serde(rename_all="lowercase")]
pub enum TrimEdge { In, Out }
#[derive(Deserialize,Default)]
#[serde(rename_all="lowercase")]
pub enum TrimMode { #[default] Regular, Ripple }
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct TrimRequest {
    document:Value, sequence_id:String, clip_id:String, edge:TrimEdge,
    #[serde(default)] mode:TrimMode,
    #[serde(default)] delta_ticks:Option<String>,
    #[serde(default)] delta_frames:Option<String>,
    linked:bool,
}
pub async fn trim(State(state):State<AppState>,ApiJson(request):ApiJson<TrimRequest>)->AppResult<Json<Value>> {
    let mut document=decode(&request.document)?;
    let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
    let clip=request.clip_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid clip ID"))?;
    let delta=edit_delta(&document,sequence,request.delta_ticks,request.delta_frames)?;
    let result=state.cpu_pool.execute(CancellationToken::new(),move |_| {
        let edge=match request.edge {TrimEdge::In=>crate::domain::filmcraft_trim::Edge::In,TrimEdge::Out=>crate::domain::filmcraft_trim::Edge::Out};
        let applied=match request.mode {
            TrimMode::Regular=>crate::domain::filmcraft_trim::trim_linked(&mut document,sequence,clip,edge,delta,request.linked)?,
            TrimMode::Ripple=>crate::domain::filmcraft_trim::ripple_trim_linked(&mut document,sequence,clip,edge,delta,request.linked)?,
        };
        let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;
        result["appliedDeltaTicks"]=json!(applied.to_string());Ok(result)
    }).await.map_err(|error|match error {
        CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),
        CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),
        error=>AppError::internal("source project trim",anyhow::anyhow!(error)),
    })?;
    Ok(Json(result))
}

fn edit_frame_rate(document:&SourceProjectDocument,sequence:u64)->AppResult<(i64,i64)>{
            let rate=document.project().get("items").and_then(|v|v.get(sequence.to_string())).and_then(|v|v.pointer("/kind/Sequence/settings/frame_rate")).ok_or_else(||AppError::bad_request("missing sequence frame rate"))?;
            let (mut num,mut den)=(rate["num"].as_i64().ok_or_else(||AppError::bad_request("invalid frame rate"))?,rate["den"].as_i64().ok_or_else(||AppError::bad_request("invalid frame rate"))?);
            if num<=0 || den<=0 {num=24000;den=1001;}
 Ok((num,den))
}
fn frame_position(document:&SourceProjectDocument,sequence:u64,ticks:i64)->AppResult<String>{
 let (num,den)=edit_frame_rate(document,sequence)?;
 Ok(((ticks as i128*num as i128).div_euclid(254_016_000_000i128*den as i128)).to_string())
}
fn edit_delta(document:&SourceProjectDocument,sequence:u64,ticks:Option<String>,frames:Option<String>)->AppResult<i64>{
    let delta=match (ticks,frames) {
        (Some(ticks),None)=>ticks.parse::<i64>().map_err(|_|AppError::bad_request("invalid trim ticks"))?,
        (None,Some(frames))=>{
            let frames=frames.parse::<i64>().map_err(|_|AppError::bad_request("invalid trim frames"))?;
            let (num,den)=edit_frame_rate(document,sequence)?;
            let numerator=(frames as i128).checked_mul(254_016_000_000).and_then(|n|n.checked_mul(den as i128)).ok_or_else(||AppError::bad_request("trim frame clock overflow"))?;
            i64::try_from(numerator.div_euclid(num as i128)).map_err(|_|AppError::bad_request("trim frame clock overflow"))?
        }
        _=>return Err(AppError::bad_request("provide exactly one trim delta")),
    };
    Ok(delta)
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct SlipRequest { document:Value,sequence_id:String,clip_id:String,#[serde(default)] delta_ticks:Option<String>,#[serde(default)] delta_frames:Option<String> }
pub async fn slip(State(state):State<AppState>,ApiJson(request):ApiJson<SlipRequest>)->AppResult<Json<Value>>{
 let mut document=decode(&request.document)?;
 let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 let clip=request.clip_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid clip ID"))?;
 let delta=edit_delta(&document,sequence,request.delta_ticks,request.delta_frames)?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let applied=crate::domain::filmcraft_trim::slip(&mut document,sequence,clip,delta)?;
  let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;
  result["appliedDeltaTicks"]=json!(applied.to_string());Ok(result)
 }).await.map_err(|error|match error{
  CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),
  CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),
  error=>AppError::internal("source project slip",anyhow::anyhow!(error)),
 })?;Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct StretchRequest { document:Value,sequence_id:String,clip_id:String,edge:TrimEdge,#[serde(default)] delta_ticks:Option<String>,#[serde(default)] delta_frames:Option<String> }
pub async fn stretch(State(state):State<AppState>,ApiJson(request):ApiJson<StretchRequest>)->AppResult<Json<Value>>{
 let mut document=decode(&request.document)?;
 let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 let clip=request.clip_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid clip ID"))?;
 let delta=edit_delta(&document,sequence,request.delta_ticks,request.delta_frames)?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let edge=match request.edge {TrimEdge::In=>crate::domain::filmcraft_trim::Edge::In,TrimEdge::Out=>crate::domain::filmcraft_trim::Edge::Out};
  let applied=crate::domain::filmcraft_trim::rate_stretch(&mut document,sequence,clip,edge,delta)?;
  let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;
  result["appliedSpeed"]=json!(applied);Ok(result)
 }).await.map_err(|error|match error{
  CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),
  CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),
  error=>AppError::internal("source project stretch",anyhow::anyhow!(error)),
 })?;Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct RollRequest { document:Value,sequence_id:String,left_clip_id:String,right_clip_id:String,#[serde(default)] delta_ticks:Option<String>,#[serde(default)] delta_frames:Option<String> }
pub async fn roll(State(state):State<AppState>,ApiJson(request):ApiJson<RollRequest>)->AppResult<Json<Value>>{
 let mut document=decode(&request.document)?;
 let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 let left=request.left_clip_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid left clip ID"))?;
 let right=request.right_clip_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid right clip ID"))?;
 let delta=edit_delta(&document,sequence,request.delta_ticks,request.delta_frames)?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let applied=crate::domain::filmcraft_trim::roll(&mut document,sequence,left,right,delta)?;
  let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;
  result["appliedDeltaTicks"]=json!(applied.to_string());Ok(result)
 }).await.map_err(|error|match error{
  CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),
  CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),
  error=>AppError::internal("source project roll",anyhow::anyhow!(error)),
 })?;Ok(Json(result))
}


#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct SlideRequest { document:Value,sequence_id:String,clip_id:String,#[serde(default)] delta_ticks:Option<String>,#[serde(default)] delta_frames:Option<String> }
pub async fn slide(State(state):State<AppState>,ApiJson(request):ApiJson<SlideRequest>)->AppResult<Json<Value>>{
 let mut document=decode(&request.document)?;
 let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 let clip=request.clip_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid clip ID"))?;
 let delta=edit_delta(&document,sequence,request.delta_ticks,request.delta_frames)?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let applied=crate::domain::filmcraft_trim::slide(&mut document,sequence,clip,delta)?;
  let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;
  result["appliedDeltaTicks"]=json!(applied.to_string());Ok(result)
 }).await.map_err(|error|match error{
  CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),
  CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),
  error=>AppError::internal("source project slide",anyhow::anyhow!(error)),
 })?;Ok(Json(result))
}


#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct DeleteRequest { document:Value,sequence_id:String,clip_ids:Vec<String>,ripple:bool }
pub async fn delete(State(state):State<AppState>,ApiJson(request):ApiJson<DeleteRequest>)->AppResult<Json<Value>>{
 if request.clip_ids.len()>4096{return Err(AppError::bad_request("too many selected clips"));}
 let mut document=decode(&request.document)?;
 let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 let ids=request.clip_ids.iter().map(|id|id.parse::<u64>().map_err(|_|AppError::bad_request("invalid clip ID"))).collect::<AppResult<Vec<_>>>()?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let applied=crate::domain::filmcraft_trim::delete_selection(&mut document,sequence,&ids,request.ripple)?;
  let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;
  result["removed"]=json!(applied.removed);
  result["closedSpans"]=json!(applied.closed_spans.iter().map(|(start,duration)|json!({"startTicks":start.to_string(),"durationTicks":duration.to_string()})).collect::<Vec<_>>());Ok(result)
 }).await.map_err(|error|match error{
  CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),
  CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),
  error=>AppError::internal("source project delete",anyhow::anyhow!(error)),
 })?;Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct CloseGapRequest { document:Value,sequence_id:String,track_id:String,#[serde(default)] time_ticks:Option<String>,#[serde(default)] time_frames:Option<String> }
pub async fn close_gap(State(state):State<AppState>,ApiJson(request):ApiJson<CloseGapRequest>)->AppResult<Json<Value>>{
 let mut document=decode(&request.document)?;
 let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 let track=request.track_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid track ID"))?;
 let time=edit_delta(&document,sequence,request.time_ticks,request.time_frames)?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  crate::domain::filmcraft_trim::close_gap(&mut document,sequence,track,time)?;
  manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))
 }).await.map_err(|error|match error{
  CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),
  CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),
  error=>AppError::internal("source project close gap",anyhow::anyhow!(error)),
 })?;Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct RazorRequest { document:Value,sequence_id:String,#[serde(default)] track_ids:Option<Vec<String>>,#[serde(default)] clip_ids:Option<Vec<String>>,#[serde(default)] time_ticks:Option<String>,#[serde(default)] time_frames:Option<String> }
pub async fn razor(State(state):State<AppState>,ApiJson(request):ApiJson<RazorRequest>)->AppResult<Json<Value>>{
 if request.track_ids.is_some()==request.clip_ids.is_some(){return Err(AppError::bad_request("provide exactly one track or clip selection"));}
 let parse=|ids:Option<Vec<String>>|->AppResult<Option<Vec<u64>>>{ids.map(|ids|{if ids.len()>4096{return Err(AppError::bad_request("too many selected IDs"));}ids.iter().map(|id|id.parse::<u64>().map_err(|_|AppError::bad_request("invalid selection ID"))).collect()}).transpose()};
 let tracks=parse(request.track_ids)?;let clips=parse(request.clip_ids)?;
 let mut document=decode(&request.document)?;
 let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 let time=edit_delta(&document,sequence,request.time_ticks,request.time_frames)?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let created=crate::domain::filmcraft_trim::razor(&mut document,sequence,tracks.as_deref().unwrap_or(&[]),clips.as_deref(),time)?;
  let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;
  result["createdClipIds"]=json!(created.iter().map(u64::to_string).collect::<Vec<_>>());Ok(result)
 }).await.map_err(|error|match error{
  CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),
  CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),
  error=>AppError::internal("source project razor",anyhow::anyhow!(error)),
 })?;Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct RangeRequest { document:Value,sequence_id:String,track_ids:Vec<String>,ripple:bool,#[serde(default)] start_ticks:Option<String>,#[serde(default)] duration_ticks:Option<String>,#[serde(default)] start_frames:Option<String>,#[serde(default)] duration_frames:Option<String> }
pub async fn remove_range(State(state):State<AppState>,ApiJson(request):ApiJson<RangeRequest>)->AppResult<Json<Value>>{
 if request.track_ids.len()>4096{return Err(AppError::bad_request("too many selected tracks"));}
 let ticks=request.start_ticks.is_some() && request.duration_ticks.is_some() && request.start_frames.is_none() && request.duration_frames.is_none();
 let frames=request.start_frames.is_some() && request.duration_frames.is_some() && request.start_ticks.is_none() && request.duration_ticks.is_none();
 if !ticks && !frames{return Err(AppError::bad_request("provide start and duration in exactly one clock unit"));}
 let tracks=request.track_ids.iter().map(|id|id.parse::<u64>().map_err(|_|AppError::bad_request("invalid track ID"))).collect::<AppResult<Vec<_>>>()?;
 let mut document=decode(&request.document)?;
 let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 let start=edit_delta(&document,sequence,request.start_ticks,request.start_frames)?;
 let duration=edit_delta(&document,sequence,request.duration_ticks,request.duration_frames)?;
 if duration<0{return Err(AppError::bad_request("range duration must not be negative"));}
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let removed=crate::domain::filmcraft_trim::remove_range(&mut document,sequence,&tracks,start,duration,request.ripple)?;
  let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;
  result["removedClipIds"]=json!(removed.iter().map(u64::to_string).collect::<Vec<_>>());Ok(result)
 }).await.map_err(|error|match error{
  CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),
  CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),
  error=>AppError::internal("source project range edit",anyhow::anyhow!(error)),
 })?;Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ClipPlacement { track_id:String,clip_text:String }
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct PlacementRequest { document:Value,sequence_id:String,placements:Vec<ClipPlacement>,insert:bool }
pub async fn place_clips(State(state):State<AppState>,ApiJson(request):ApiJson<PlacementRequest>)->AppResult<Json<Value>>{
 if request.placements.len()>4096{return Err(AppError::bad_request("too many placements"));}
 let mut document=decode(&request.document)?;
 let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 let mut placements=Vec::with_capacity(request.placements.len());
 for placement in request.placements{
  let track=placement.track_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid placement track ID"))?;
  let clip:Value=serde_json::from_str(&placement.clip_text).map_err(|e|AppError::bad_request(format!("invalid clip text: {e}")))?;
  crate::domain::filmcraft_trim::validate_clip_fields(&clip).map_err(|e|AppError::bad_request(e.to_string()))?;
  placements.push((track,clip));
 }
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let placed=crate::domain::filmcraft_trim::place_clips(&mut document,sequence,&placements,request.insert)?;
  let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;
  result["placedClipIds"]=json!(placed.iter().map(u64::to_string).collect::<Vec<_>>());Ok(result)
 }).await.map_err(|error|match error{
  CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),
  CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),
  error=>AppError::internal("source project placement",anyhow::anyhow!(error)),
 })?;Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct CopyRequest { document:Value,sequence_id:String,clip_ids:Vec<String>,linked:bool,include_markers:bool }
pub async fn copy_clips(State(state):State<AppState>,ApiJson(request):ApiJson<CopyRequest>)->AppResult<Json<Value>>{
 if request.clip_ids.len()>4096{return Err(AppError::bad_request("too many copied clips"));}
 let document=decode(&request.document)?;let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 let ids=request.clip_ids.iter().map(|id|id.parse::<u64>().map_err(|_|AppError::bad_request("invalid clip ID"))).collect::<AppResult<Vec<_>>>()?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let clipboard=crate::domain::filmcraft_trim::copy_clips(&document,sequence,&ids,request.linked,request.include_markers)?;
  Ok(json!({"clipboardText":serde_json::to_string(&clipboard)?,"clipCount":clipboard["clips"].as_array().unwrap().len(),"markerCount":clipboard["markers"].as_array().unwrap().len()}))
 }).await.map_err(|error|match error{
  CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),error=>AppError::internal("source project copy",anyhow::anyhow!(error)),
 })?;Ok(Json(result))
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct PasteRequest { document:Value,sequence_id:String,clipboard_text:String,insert:bool,include_markers:bool,ripple_markers:bool,#[serde(default)] time_ticks:Option<String>,#[serde(default)] time_frames:Option<String> }
pub async fn paste_clips(State(state):State<AppState>,ApiJson(request):ApiJson<PasteRequest>)->AppResult<Json<Value>>{
 let mut document=decode(&request.document)?;let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 let at=edit_delta(&document,sequence,request.time_ticks,request.time_frames)?;
 let clipboard:Value=serde_json::from_str(&request.clipboard_text).map_err(|e|AppError::bad_request(format!("invalid clipboard text: {e}")))?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let pasted=crate::domain::filmcraft_trim::paste_clips(&mut document,sequence,&clipboard,at,request.insert,request.include_markers,request.ripple_markers)?;
  let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;result["selectedClipIds"]=json!(pasted.ids.iter().map(u64::to_string).collect::<Vec<_>>());result["playheadTicks"]=json!(pasted.playhead.to_string());result["playheadFrames"]=json!(frame_position(&document,sequence,pasted.playhead).map_err(|e|anyhow::anyhow!("{e:?}"))?);Ok(result)
 }).await.map_err(|error|match error{
  CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),error=>AppError::internal("source project paste",anyhow::anyhow!(error)),
 })?;Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct AddTransitionRequest { document:Value,sequence_id:String,track_id:String,transition_text:String }
pub async fn add_transition(State(state):State<AppState>,ApiJson(request):ApiJson<AddTransitionRequest>)->AppResult<Json<Value>>{
 let mut document=decode(&request.document)?;
 let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 let track=request.track_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid transition track ID"))?;
 let transition:Value=serde_json::from_str(&request.transition_text).map_err(|e|AppError::bad_request(format!("invalid transition text: {e}")))?;
 let valid=transition.is_object() && transition["id"].as_u64().is_some()
  && transition["start"].as_i64().is_some() && transition["duration"].as_i64().is_some()
  && ["from","to"].into_iter().all(|key|transition[key].is_null()||transition[key].as_u64().is_some())
  && matches!(transition["align"].as_str(),Some("CenterAtCut"|"StartAtCut"|"EndAtCut"))
  && (transition.get("reverse").is_none()||transition["reverse"].is_boolean())
  && transition["effect"]["effect"].is_string() && transition["effect"]["enabled"].is_boolean()
  && transition["effect"]["params"].is_object();
 if !valid{return Err(AppError::bad_request("invalid transition fields"));}
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let id=crate::domain::filmcraft_trim::add_transition(&mut document,sequence,track,&transition)?;
  let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;
  result["createdTransitionId"]=json!(id.to_string());Ok(result)
 }).await.map_err(|error|match error{
  CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),
  CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),
  error=>AppError::internal("source project transition",anyhow::anyhow!(error)),
 })?;Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="lowercase")]
pub enum TransitionTrackKind { Video, Audio }
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct TransitionDurationPreference { value:f64,unit:TransitionDurationUnit }
#[derive(Deserialize)]
#[serde(rename_all="lowercase")]
pub enum TransitionDurationUnit { Frames, Seconds }
fn transition_duration(document:&SourceProjectDocument,sequence:u64,frames:Option<String>,preference:Option<TransitionDurationPreference>,video:bool)->AppResult<i64>{
 if frames.is_some()&&preference.is_some(){return Err(AppError::bad_request("choose frame duration or duration preference"));}
 if let Some(frames)=frames{return edit_delta(document,sequence,None,Some(frames));}
 let preference=preference.unwrap_or(TransitionDurationPreference{value:if video{30.}else{1.},unit:if video{TransitionDurationUnit::Frames}else{TransitionDurationUnit::Seconds}});
 let (num,den)=edit_frame_rate(document,sequence)?;crate::domain::filmcraft_transition::preference_duration(preference.value,match preference.unit{TransitionDurationUnit::Frames=>"frames",TransitionDurationUnit::Seconds=>"seconds"},num,den).map_err(|e|AppError::bad_request(e.to_string()))
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ApplyTransitionRequest {
 document:Value,sequence_id:String,kind:TransitionTrackKind,effect:String,#[serde(default)] frames:Option<String>,#[serde(default)] duration:Option<TransitionDurationPreference>,
 #[serde(default)] targeted_track_ids:Vec<String>,#[serde(default)] clip_id:Option<String>,#[serde(default)] edge:Option<TrimEdge>,
 #[serde(default)] time_ticks:Option<String>,#[serde(default)] time_frames:Option<String>,
 #[serde(default="empty_transition_params")] params:Value,#[serde(default)] reverse:bool,
}
fn empty_transition_params()->Value{json!({})}
pub async fn apply_transition(State(state):State<AppState>,ApiJson(request):ApiJson<ApplyTransitionRequest>)->AppResult<Json<Value>>{
 if request.targeted_track_ids.len()>4096{return Err(AppError::bad_request("too many transition targets"));}
 let mut document=decode(&request.document)?;let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 let at=edit_delta(&document,sequence,request.time_ticks,request.time_frames)?;
 let duration=transition_duration(&document,sequence,request.frames,request.duration,matches!(request.kind,TransitionTrackKind::Video))?;
 let targets:Vec<u64>=request.targeted_track_ids.iter().map(|id|id.parse().map_err(|_|AppError::bad_request("invalid transition target ID"))).collect::<AppResult<_>>()?;
 let clip=request.clip_id.map(|id|id.parse::<u64>().map_err(|_|AppError::bad_request("invalid transition clip ID"))).transpose()?;
 let edge=request.edge.map(|edge|match edge{TrimEdge::In=>crate::domain::filmcraft_trim::Edge::In,TrimEdge::Out=>crate::domain::filmcraft_trim::Edge::Out});
 let (kind,effect_kind)=match request.kind{TransitionTrackKind::Video=>("video_tracks","VideoTransition"),TransitionTrackKind::Audio=>("audio_tracks","AudioTransition")};
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let mut effect=crate::domain::filmcraft_transition::instance(&request.effect,effect_kind)?;
  crate::domain::filmcraft_transition::set_params(&mut effect,&request.params)?;
  let plan=crate::domain::filmcraft_trim::plan_transition(&document,sequence,kind,&targets,clip,edge,at,duration)?;
  let transition=json!({"id":0,"effect":effect,"start":plan["start"],"duration":duration,"from":plan["from"],"to":plan["to"],"align":"CenterAtCut","reverse":request.reverse});
  let id=crate::domain::filmcraft_trim::add_transition(&mut document,sequence,plan["track"].as_u64().unwrap(),&transition)?;
  let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;result["createdTransitionId"]=json!(id.to_string());Ok(result)
 }).await.map_err(|error|match error{CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),error=>AppError::internal("source project apply transition",anyhow::anyhow!(error))})?;Ok(Json(result))
}

pub async fn transition_catalog()->Json<Value>{Json(crate::domain::filmcraft_transition::catalog().clone())}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct SetTransitionRequest { document:Value,sequence_id:String,transition_id:String,#[serde(default)] reset:bool,#[serde(default,deserialize_with="present_transition_params")] params:Option<Value>,#[serde(default)] reverse:Option<bool> }
fn present_transition_params<'de,D:serde::Deserializer<'de>>(deserializer:D)->Result<Option<Value>,D::Error>{Value::deserialize(deserializer).map(Some)}
pub async fn set_transition(State(state):State<AppState>,ApiJson(request):ApiJson<SetTransitionRequest>)->AppResult<Json<Value>>{
 let mut document=decode(&request.document)?;let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;let transition=request.transition_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid transition ID"))?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{crate::domain::filmcraft_transition::set_transition(&mut document,sequence,transition,request.reset,request.params.as_ref(),request.reverse)?;manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))}).await.map_err(|error|match error{CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),error=>AppError::internal("source project transition settings",anyhow::anyhow!(error))})?;Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct TransitionEditPoint { clip_id:String,edge:String }

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct BatchTransitionRequest { document:Value,sequence_id:String,clip_ids:Vec<String>,#[serde(default)] edit_points:Vec<TransitionEditPoint>,video_effect:String,audio_effect:String,#[serde(default)] video_frames:Option<String>,#[serde(default)] audio_frames:Option<String>,#[serde(default)] video_duration:Option<TransitionDurationPreference>,#[serde(default)] audio_duration:Option<TransitionDurationPreference> }
pub async fn batch_transitions(State(state):State<AppState>,ApiJson(request):ApiJson<BatchTransitionRequest>)->AppResult<Json<Value>>{
 if request.clip_ids.len()>4096||request.edit_points.len()>4096{return Err(AppError::bad_request("too many transition selections"));}
 let points:Vec<_>=request.edit_points.iter().map(|point|{let clip=point.clip_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid edit-point clip ID"))?;let edge=match point.edge.as_str(){"in"=>crate::domain::filmcraft_trim::Edge::In,"out"=>crate::domain::filmcraft_trim::Edge::Out,_=>return Err(AppError::bad_request("invalid edit-point edge"))};Ok((clip,edge))}).collect::<AppResult<_>>()?;
 let mut document=decode(&request.document)?;let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;let clips:Vec<u64>=request.clip_ids.iter().map(|id|id.parse().map_err(|_|AppError::bad_request("invalid transition clip ID"))).collect::<AppResult<_>>()?;
 let video_duration=transition_duration(&document,sequence,request.video_frames,request.video_duration,true)?;let audio_duration=transition_duration(&document,sequence,request.audio_frames,request.audio_duration,false)?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{let ids=if clips.is_empty(){crate::domain::filmcraft_transition::apply_edit_points(&mut document,sequence,&points,&request.video_effect,&request.audio_effect,video_duration,audio_duration)?}else{crate::domain::filmcraft_transition::apply_selection(&mut document,sequence,&clips,&request.video_effect,&request.audio_effect,video_duration,audio_duration)?};let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;result["createdTransitionIds"]=json!(ids.iter().map(u64::to_string).collect::<Vec<_>>());Ok(result)}).await.map_err(|error|match error{CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),error=>AppError::internal("source project batch transitions",anyhow::anyhow!(error))})?;Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct TrimMonitorRequest { document:Value,sequence_id:String,clip_id:String,edge:String,#[serde(default)] include_frames:bool,#[serde(default)]sources:BTreeMap<String,String> }
pub async fn trim_monitor(State(state):State<AppState>,headers:HeaderMap,ApiJson(request):ApiJson<TrimMonitorRequest>)->AppResult<Json<Value>>{
 let document=decode(&request.document)?;let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid monitor sequence ID"))?;let clip=request.clip_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid monitor clip ID"))?;
 let edge=match request.edge.as_str(){"in"=>crate::domain::filmcraft_trim::Edge::In,"out"=>crate::domain::filmcraft_trim::Edge::Out,_=>return Err(AppError::bad_request("invalid monitor edge"))};
 if request.sources.len()>64{return Err(AppError::bad_request("too many monitor bindings"));}
 let index=ProjectIndex::new(&document).map_err(|e|AppError::bad_request(e.to_string()))?;let mut paths=BTreeMap::new();let mut video_paths=BTreeMap::new();
 if request.include_frames{for (id,source_id) in &request.sources{
  let id=id.parse::<u64>().map_err(|_|AppError::bad_request("invalid source item ID"))?;
  let media=index.resolve_media(id).filter(|m|m.item_id==id&&m.media.pointer("/media/File").is_some()).ok_or_else(||AppError::bad_request("binding must identify root file media"))?;let _=media;
  super::library::authorize_source_access(&state,source_id,&headers).await?;
  let entry=state.library.get(source_id).await.filter(|e|e.kind=="source").ok_or_else(||AppError::not_found("media source not found"))?;
  let path=state.library.resolve_media_path(&entry).await.map_err(|_|AppError::not_found("media source not found"))?;if entry.media_type.as_deref()==Some("video")||matches!(path.extension().and_then(|s|s.to_str()).map(str::to_ascii_lowercase).as_deref(),Some("mp4"|"mov"|"mkv"|"webm"|"avi"|"mxf"|"mts"|"m2ts"|"mpg"|"mpeg"|"flv"|"ogv")){video_paths.insert(id,path.clone());}paths.insert(id,path);
 }}
 let video_roots=video_paths.keys().copied().collect::<std::collections::BTreeSet<_>>();
 let mut result=state.cpu_pool.execute(CancellationToken::new(),move |cancel|{
  use crate::domain::filmcraft_trim::{edit_point_context,trim_monitor_side};
  let context=edit_point_context(&document,sequence,clip,edge)?;
  let mut sides=vec![];
  for (key,outgoing) in [("outgoing",true),("incoming",false)]{
   let mut side=match context[key].as_u64(){Some(id)=>trim_monitor_side(&document,sequence,id,outgoing)?.unwrap_or(Value::Null),None=>Value::Null};
   if !side.is_null(){
    if request.include_frames&&side["video"]==true{
     // A source-sized scale is quantized exactly like GeneratorSource. Admission
     // in video_frame still bounds output allocation; this monitor requests 1/8.
     let item=side["item"].as_u64().unwrap();let time=side["mediaTime"].as_i64().unwrap();
     let index=ProjectIndex::new(&document)?;let file=index.resolve_media(item).filter(|source|source.media.pointer("/media/File").is_some());
     let kind=if file.is_some(){"image"}else{"generator"};
     let rendered=if let Some(source)=file{
      (||->anyhow::Result<crate::domain::filmcraft_generator_video::GeneratorVideoFrame>{
       anyhow::ensure!(source.media["offline"]!=true,"source media is offline");if video_roots.contains(&source.item_id){side["_videoSource"]=json!(source.item_id.to_string());anyhow::bail!("video preview pending");}let path=paths.get(&source.item_id).ok_or_else(||anyhow::anyhow!("source image is not bound to the library"))?;
       anyhow::ensure!(!cancel.is_cancelled(),"monitor cancelled");anyhow::ensure!(std::fs::metadata(path)?.len()<=64*1024*1024,"image source budget exceeded");
       let image=crate::domain::group_media::decode_image_surface(path)?;let scale=(480./image.width as f64).min(270./image.height as f64).min(1.);let width=((image.width as f64*scale).round() as u32).max(1);let height=((image.height as f64*scale).round() as u32).max(1);
       let raw=image::RgbaImage::from_raw(image.width as u32,image.height as u32,image.rgba).ok_or_else(||anyhow::anyhow!("invalid source image"))?;
       let resized=image::imageops::resize(&raw,width,height,image::imageops::FilterType::Triangle);anyhow::ensure!(!cancel.is_cancelled(),"monitor cancelled");
       Ok(crate::domain::filmcraft_generator_video::GeneratorVideoFrame{surface:crate::domain::group_surface::GroupSurface{width:width as usize,height:height as usize,rgba:resized.into_raw()},pts:0,frame:0})
      })()
     }else{crate::domain::filmcraft_generator_video::source_frame(&document,item,time,0.125,||cancel.is_cancelled())};
     match rendered{
      Ok(frame)=>{side["frame"]=json!({"kind":kind,"sampleTicks":time.to_string(),"width":frame.surface.width,"height":frame.surface.height,"ptsTicks":frame.pts.to_string(),"rgbaHex":frame.surface.rgba.iter().map(|v|format!("{v:02x}")).collect::<String>()});},
      Err(error)=>{side["frameUnavailable"]=json!(error.to_string());}
     }
    }
    for key in ["clip","item","time","mediaTime","mediaFrame","sourceIn","sourceOut"]{side[key]=Value::String(side[key].to_string());}
   }
   sides.push(side);
  }
  Ok(json!({"editTime":context["editTime"].as_i64().map(|v|v.to_string()),"rollPair":context["rollPair"].as_array().map(|ids|ids.iter().map(|id|id.as_u64().unwrap().to_string()).collect::<Vec<_>>()),"outgoing":sides[0],"incoming":sides[1]}))
 }).await.map_err(|error|match error{CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),CpuTaskError::Saturated=>AppError::service_unavailable("project monitor queue is full"),error=>AppError::internal("source project trim monitor",anyhow::anyhow!(error))})?;
 for key in ["outgoing","incoming"]{
  let pending=result[key].as_object_mut().and_then(|side|side.remove("_videoSource"));let Some(root)=pending.and_then(|v|v.as_str().and_then(|s|s.parse::<u64>().ok()))else{continue};
  let Some(path)=video_paths.get(&root)else{continue};let ticks=result[key]["mediaTime"].as_str().unwrap().parse::<i64>().unwrap();
  let rendered=async{
   let _permit=tokio::time::timeout(std::time::Duration::from_secs(2),state.acquire_render_slot()).await.map_err(|_|anyhow::anyhow!("video preview queue is busy"))?.map_err(|_|anyhow::anyhow!("video preview queue unavailable"))?;
   crate::services::filmcraft_video_preview::frame_cached(&state.filmcraft_video_previews,&state.process_runtime,path,ticks).await
  }.await;
  match rendered{
   Ok(Some(frame))=>{result[key].as_object_mut().unwrap().remove("frameUnavailable");result[key]["frame"]=json!({"kind":"video","sampleTicks":ticks.to_string(),"width":frame.surface.width,"height":frame.surface.height,"ptsTicks":frame.pts.to_string(),"sourcePts":frame.source_pts.to_string(),"originPts":frame.origin.to_string(),"ordinal":frame.ordinal.to_string(),"timeBase":{"num":frame.num.to_string(),"den":frame.den.to_string()},"rgbaHex":frame.surface.rgba.iter().map(|v|format!("{v:02x}")).collect::<String>()});},
   Ok(None)=>{result[key]["frameUnavailable"]=json!("no video frame at this source time");},
   Err(error)=>{result[key]["frameUnavailable"]=json!(error.to_string());}
  }
 }
 Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct NearestEditPointsRequest { document:Value,sequence_id:String,kind:String,#[serde(default)] targeted_track_ids:Vec<String>,#[serde(default)] time_ticks:Option<String>,#[serde(default)] time_frames:Option<String> }
pub async fn nearest_edit_points(State(state):State<AppState>,ApiJson(request):ApiJson<NearestEditPointsRequest>)->AppResult<Json<Value>>{
 if !matches!(request.kind.as_str(),"trimIn"|"trimOut"|"rippleIn"|"rippleOut"|"roll"){return Err(AppError::bad_request("invalid nearest-edit kind"));}
 if request.targeted_track_ids.len()>4096{return Err(AppError::bad_request("too many targeted tracks"));}
 let document=decode(&request.document)?;let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid nearest-edit sequence ID"))?;
 let targets:Vec<u64>=request.targeted_track_ids.iter().map(|id|id.parse().map_err(|_|AppError::bad_request("invalid targeted track ID"))).collect::<AppResult<_>>()?;
 let time=edit_delta(&document,sequence,request.time_ticks,request.time_frames)?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let mut result=crate::domain::filmcraft_trim::nearest_edit_points(&document,sequence,&targets,time,&request.kind)?;
  for point in result["editPoints"].as_array_mut().unwrap(){point["clip"]=json!(point["clip"].as_u64().unwrap().to_string());}
  Ok(result)
 }).await.map_err(|error|match error{CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),CpuTaskError::Saturated=>AppError::service_unavailable("project selection queue is full"),error=>AppError::internal("source project nearest edit points",anyhow::anyhow!(error))})?;Ok(Json(result))
}

#[derive(Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct TrimPointInput { clip:String,out:bool,kind:String }
#[derive(Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct TrimShiftInput { out_shift:String,in_shift:String }
impl Default for TrimShiftInput {fn default()->Self{Self{out_shift:"0".into(),in_shift:"0".into()}}}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct TrimPointsRequest { document:Value,sequence_id:String,points:Vec<TrimPointInput>,linked:bool,#[serde(default)] shift:TrimShiftInput,#[serde(default)] delta_ticks:Option<String>,#[serde(default)] delta_frames:Option<String>,#[serde(default)] playhead_ticks:Option<String>,#[serde(default)] playhead_frames:Option<String> }
pub async fn trim_points(State(state):State<AppState>,ApiJson(request):ApiJson<TrimPointsRequest>)->AppResult<Json<Value>>{
 use crate::domain::filmcraft_trim::{TrimEditPoint,TrimKind,Edge};
 if request.points.is_empty()||request.points.len()>4096{return Err(AppError::bad_request("select between one and 4096 edit points"));}
 let mut document=decode(&request.document)?;let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid trim-point sequence ID"))?;
 if document.envelope.pointer(&format!("/project/items/{sequence}/kind/Sequence")).is_none(){return Err(AppError::bad_request("missing trim-point sequence"));}
 let points:Vec<_>=request.points.iter().map(|p|{let clip=p.clip.parse::<u64>().map_err(|_|AppError::bad_request("invalid trim-point clip ID"))?;let kind=match p.kind.as_str(){"trim"=>TrimKind::Trim,"ripple"=>TrimKind::Ripple,"roll"=>TrimKind::Roll,_=>return Err(AppError::bad_request("invalid trim-point kind"))};Ok(TrimEditPoint{clip,edge:if p.out{Edge::Out}else{Edge::In},kind})}).collect::<AppResult<_>>()?;
 let clock_count=[request.delta_ticks.is_some(),request.delta_frames.is_some(),request.playhead_ticks.is_some(),request.playhead_frames.is_some()].into_iter().filter(|present|*present).count();
 if clock_count!=1{return Err(AppError::bad_request("provide exactly one trim delta or playhead clock"));}
 let extend=request.playhead_ticks.is_some()||request.playhead_frames.is_some();
 let delta=if extend{edit_delta(&document,sequence,request.playhead_ticks,request.playhead_frames)?}else{edit_delta(&document,sequence,request.delta_ticks,request.delta_frames)?};
 let shift=crate::domain::filmcraft_trim::TrimShift{outgoing:request.shift.out_shift.parse::<i64>().map_err(|_|AppError::bad_request("invalid outgoing trim shift"))?,incoming:request.shift.in_shift.parse::<i64>().map_err(|_|AppError::bad_request("invalid incoming trim shift"))?};
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let edit=if extend{crate::domain::filmcraft_trim::extend_edit_points_to_playhead(&mut document,sequence,&points,delta,request.linked)}else{crate::domain::filmcraft_trim::trim_edit_points(&mut document,sequence,&points,delta,request.linked)};
  let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;
  let edit=edit.and_then(|applied|crate::domain::filmcraft_trim::trim_shift_after(shift,points.first().copied(),&applied).map(|updated|(applied,updated)));
  result["shift"]=json!({"outShift":shift.outgoing.to_string(),"inShift":shift.incoming.to_string()});
  match edit{Ok((applied,updated))=>{result["shift"]=json!({"outShift":updated.outgoing.to_string(),"inShift":updated.incoming.to_string()});result["appliedTicks"]=json!(applied.iter().map(i64::to_string).collect::<Vec<_>>());result["refusal"]=Value::Null;},Err(error)=>{result["appliedTicks"]=Value::Null;result["refusal"]=json!(error.to_string());}}
  let frame_shift=|key:&str|->anyhow::Result<String>{let ticks=result["shift"][key].as_str().unwrap().parse::<i64>()?;frame_position(&document,sequence,ticks).map_err(|e|anyhow::anyhow!("{e:?}"))};
  result["shiftFrames"]=json!({"outShift":frame_shift("outShift")?,"inShift":frame_shift("inShift")?});

  Ok(result)
 }).await.map_err(|error|match error{CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),error=>AppError::internal("source project trim points",anyhow::anyhow!(error))})?;Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ToPlayheadRequest { document:Value,sequence_id:String,clip_ids:Vec<String>,targeted_track_ids:Vec<String>,side:String,ripple:bool,linked:bool,#[serde(default)] time_ticks:Option<String>,#[serde(default)] time_frames:Option<String> }
pub async fn to_playhead(State(state):State<AppState>,ApiJson(request):ApiJson<ToPlayheadRequest>)->AppResult<Json<Value>>{
 if request.clip_ids.len()>4096||request.targeted_track_ids.len()>4096{return Err(AppError::bad_request("too many selected clips or tracks"));}
 let next=match request.side.as_str(){"previous"=>false,"next"=>true,_=>return Err(AppError::bad_request("invalid trim-to-playhead side"))};
 let clips=request.clip_ids.iter().map(|id|id.parse::<u64>().map_err(|_|AppError::bad_request("invalid clip ID"))).collect::<AppResult<Vec<_>>>()?;
 let tracks=request.targeted_track_ids.iter().map(|id|id.parse::<u64>().map_err(|_|AppError::bad_request("invalid track ID"))).collect::<AppResult<Vec<_>>>()?;
 let mut document=decode(&request.document)?;let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 if document.envelope.pointer(&format!("/project/items/{sequence}/kind/Sequence")).is_none(){return Err(AppError::bad_request("missing trim-to-playhead sequence"));}
 let time=edit_delta(&document,sequence,request.time_ticks,request.time_frames)?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  let edit=crate::domain::filmcraft_trim::trim_to_playhead(&mut document,sequence,&clips,&tracks,time,next,request.ripple,request.linked)?;
  let mut result=manifest(&document).map_err(|e|anyhow::anyhow!("{e:?}"))?;
  let playhead=edit["playhead"].as_i64().unwrap();result["rangeTicks"]=json!([edit["range"][0].as_i64().unwrap().to_string(),edit["range"][1].as_i64().unwrap().to_string()]);result["playheadTicks"]=json!(playhead.to_string());result["playheadFrames"]=json!(frame_position(&document,sequence,playhead).map_err(|e|anyhow::anyhow!("{e:?}"))?);result["trackIds"]=json!(edit["tracks"].as_array().unwrap().iter().map(|id|id.as_u64().unwrap().to_string()).collect::<Vec<_>>());Ok(result)
 }).await.map_err(|error|match error{CpuTaskError::Work(error)=>AppError::bad_request(error.to_string()),CpuTaskError::Saturated=>AppError::service_unavailable("project edit queue is full"),error=>AppError::internal("source project trim to playhead",anyhow::anyhow!(error))})?;Ok(Json(result))
}

async fn trim_session_owner(state:&AppState,headers:&HeaderMap)->AppResult<String>{
 let token=headers.get(header::AUTHORIZATION).and_then(|v|v.to_str().ok()).and_then(|v|v.strip_prefix("Bearer ")).filter(|v|!v.is_empty()).ok_or_else(||AppError::unauthorized("Требуется Bearer-сессия"))?;
 state.db.resolve_auth_session(token).await.map_err(|e|AppError::internal("resolve trim session owner",e))?.map(|u|u.id).ok_or_else(||AppError::unauthorized("Сессия недействительна или истекла"))
}
fn trim_store_error(error:crate::domain::filmcraft_trim_sessions::SessionError)->AppError{use crate::domain::filmcraft_trim_sessions::SessionError;match error{SessionError::Missing=>AppError::not_found("trim session not found"),SessionError::Stale=>AppError::conflict("trim session revision changed"),SessionError::CreationConflict=>AppError::conflict("trim creation key reused with different parameters"),SessionError::Capacity=>AppError::service_unavailable("trim session capacity exceeded"),SessionError::Invalid(message)=>AppError::bad_request(message),SessionError::Unavailable=>AppError::service_unavailable("trim session store unavailable")}}
async fn trim_session_cpu(state:&AppState,work:impl FnOnce()->AppResult<Value>+Send+'static)->AppResult<Json<Value>>{
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|Ok(work())).await.map_err(|e|match e{CpuTaskError::Saturated=>AppError::service_unavailable("trim session queue is full"),e=>AppError::internal("trim session command",anyhow::anyhow!(e))})?;Ok(Json(result?))
}
fn trim_session_manifest(snapshot:crate::domain::filmcraft_trim_sessions::SessionSnapshot,command:Option<Value>)->AppResult<Value>{
 let session=snapshot.session;let mut result=manifest(&session.document)?;let native=session.status().map_err(|e|AppError::bad_request(e.to_string()))?;let mut dynamic=native["dynamic"].clone();if !dynamic.is_null(){dynamic["offsetFrames"]=json!(dynamic["offsetFrames"].as_i64().unwrap().to_string());}
 result["sessionId"]=json!(snapshot.id);result["revision"]=json!(snapshot.revision.to_string());result["shift"]=json!({"outShift":session.shift.outgoing.to_string(),"inShift":session.shift.incoming.to_string()});result["shiftFrames"]=json!({"outShift":frame_position(&session.document,session.sequence,session.shift.outgoing)?,"inShift":frame_position(&session.document,session.sequence,session.shift.incoming)?});
 result["status"]=json!({"playing":native["playing"],"playheadTicks":session.playhead.to_string(),"playheadFrames":frame_position(&session.document,session.sequence,session.playhead)?,"dynamic":dynamic,"around":session.around_status()});
 result["commit"]=command.as_ref().filter(|c|c.get("committed").is_some()).map(|c|json!({"committed":c["committed"],"label":c.get("label").cloned().unwrap_or(Value::Null),"offsetTicks":c["offset"].as_i64().unwrap_or(0).to_string(),"offsetFrames":c["offsetFrames"].as_i64().unwrap_or(0).to_string()})).unwrap_or(Value::Null);Ok(result)
}
#[derive(Deserialize,Serialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct CreateTrimSessionRequest{#[serde(default)]creation_key:Option<String>,document:Value,sequence_id:String,points:Vec<TrimPointInput>,linked:bool,shift:TrimShiftInput,#[serde(default)]playhead_ticks:Option<String>,#[serde(default)]playhead_frames:Option<String>}
pub async fn create_trim_session(State(state):State<AppState>,headers:HeaderMap,ApiJson(request):ApiJson<CreateTrimSessionRequest>)->AppResult<Json<Value>>{
 use crate::domain::{filmcraft_dynamic_trim::DynamicTrimSession,filmcraft_trim::{TrimEditPoint,TrimKind,Edge,TrimShift}};
 use sha2::{Digest,Sha256};
 let owner=trim_session_owner(&state,&headers).await?;
 let fingerprint:[u8;32]=Sha256::digest(serde_json::to_vec(&request).map_err(|e|AppError::bad_request(e.to_string()))?).into();
 if request.points.len()>4096{return Err(AppError::bad_request("too many edit points"));}let document=decode(&request.document)?;let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;
 let points=request.points.into_iter().map(|p|{let clip=p.clip.parse::<u64>().map_err(|_|AppError::bad_request("invalid clip ID"))?;let kind=match p.kind.as_str(){"trim"=>TrimKind::Trim,"ripple"=>TrimKind::Ripple,"roll"=>TrimKind::Roll,_=>return Err(AppError::bad_request("invalid trim kind"))};Ok(TrimEditPoint{clip,kind,edge:if p.out{Edge::Out}else{Edge::In}})}).collect::<AppResult<Vec<_>>>()?;
 let playhead=edit_delta(&document,sequence,request.playhead_ticks,request.playhead_frames)?;let mut session=DynamicTrimSession::new(document,sequence,points,request.linked).map_err(|e|AppError::bad_request(e.to_string()))?;session.playhead=playhead;session.shift=TrimShift{outgoing:request.shift.out_shift.parse::<i64>().map_err(|_|AppError::bad_request("invalid outgoing shift"))?,incoming:request.shift.in_shift.parse::<i64>().map_err(|_|AppError::bad_request("invalid incoming shift"))?};
 let store=state.filmcraft_trim_sessions.clone();trim_session_cpu(&state,move||trim_session_manifest(match request.creation_key{Some(key)=>store.create_keyed(&owner,session,key,fingerprint),None=>store.create(&owner,session)}.map_err(trim_store_error)?,None)).await
}
#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]pub struct ReadTrimSessionRequest{session_id:String}
pub async fn read_trim_session(State(state):State<AppState>,headers:HeaderMap,ApiJson(request):ApiJson<ReadTrimSessionRequest>)->AppResult<Json<Value>>{let owner=trim_session_owner(&state,&headers).await?;let store=state.filmcraft_trim_sessions.clone();trim_session_cpu(&state,move||trim_session_manifest(store.read(&owner,&request.session_id).map_err(trim_store_error)?,None)).await}
#[derive(Deserialize)]#[serde(tag="op",rename_all="camelCase",deny_unknown_fields)]
pub enum TrimSessionAction{Shuttle{clock:f64,direction:String,slow:bool},Tick{clock:f64},Stop{#[serde(default)]clock:Option<f64>},Cancel{},Around{clock:f64,#[serde(rename="loop")]looping:bool,toggle:bool,#[serde(rename="centerPlayhead")]center_playhead:bool,#[serde(rename="prerollSeconds")]preroll_seconds:f64,#[serde(rename="postrollSeconds")]postroll_seconds:f64}}
#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]pub struct TrimSessionCommandRequest{session_id:String,revision:String,action:TrimSessionAction}
pub async fn command_trim_session(State(state):State<AppState>,headers:HeaderMap,ApiJson(request):ApiJson<TrimSessionCommandRequest>)->AppResult<Json<Value>>{
 let owner=trim_session_owner(&state,&headers).await?;let revision=request.revision.parse::<u64>().map_err(|_|AppError::bad_request("invalid trim session revision"))?;let store=state.filmcraft_trim_sessions.clone();trim_session_cpu(&state,move||{let (snapshot,result)=store.command(&owner,&request.session_id,revision,move|session|match request.action{TrimSessionAction::Shuttle{clock,direction,slow}=>{let direction=match direction.as_str(){"forward"=>1.0,"reverse"=>-1.0,_=>anyhow::bail!("invalid shuttle direction")};session.shuttle(clock,direction,slow)},TrimSessionAction::Tick{clock}=>session.tick(clock),TrimSessionAction::Stop{clock}=>session.stop(clock),TrimSessionAction::Cancel{}=>session.cancel(),TrimSessionAction::Around{clock,looping,toggle,center_playhead,preroll_seconds,postroll_seconds}=>session.play_around(clock,looping,toggle,center_playhead,preroll_seconds,postroll_seconds)}).map_err(trim_store_error)?;trim_session_manifest(snapshot,Some(result))}).await
}
#[derive(Deserialize)]#[serde(rename_all="camelCase",deny_unknown_fields)]pub struct CloseTrimSessionRequest{session_id:String,revision:String}
pub async fn close_trim_session(State(state):State<AppState>,headers:HeaderMap,ApiJson(request):ApiJson<CloseTrimSessionRequest>)->AppResult<Json<Value>>{let owner=trim_session_owner(&state,&headers).await?;let revision=request.revision.parse::<u64>().map_err(|_|AppError::bad_request("invalid trim session revision"))?;let store=state.filmcraft_trim_sessions.clone();trim_session_cpu(&state,move||{store.close(&owner,&request.session_id,revision).map_err(trim_store_error)?;Ok(json!({"closed":true}))}).await}

#[derive(Clone,Deserialize,Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct PreviewFrameRequest {
 pub(super) document:Value,pub(super) sequence_id:String,
 #[serde(default)] pub(super) sources:BTreeMap<String,String>,
 #[serde(default)] time_ticks:Option<String>,#[serde(default)] time_frames:Option<String>,
 #[serde(default="preview_enabled")] effects:bool,#[serde(default="preview_enabled")] captions:bool,
}
pub(super) fn validate_analysis_request(request:&PreviewFrameRequest)->AppResult<()>{
 if request.time_ticks.is_some()==request.time_frames.is_some(){return Err(AppError::bad_request("provide exactly one analysis time"))}
 let document=decode(&request.document)?;let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid analysis sequence"))?;
 edit_delta(&document,sequence,request.time_ticks.clone(),request.time_frames.clone())?;Ok(())
}
fn preview_enabled()->bool{true}
/// Stateless composed program frame. File access is never delegated to the domain.
pub async fn preview_frame(State(state):State<AppState>,headers:HeaderMap,ApiJson(request):ApiJson<PreviewFrameRequest>)->AppResult<Json<Value>>{
 execute_preview(state,headers,request,None).await
}
#[derive(Clone)]
pub(super) struct BackgroundPreview{pub owner:String,pub principal:[u8;32],pub cancel:CancellationToken,pub job_id:String}
pub(super) fn preview_principal(headers:&HeaderMap)->[u8;32]{use sha2::Digest;let mut hash=sha2::Sha256::new();for name in ["authorization","cookie"]{let value=headers.get(name).map(|v|v.as_bytes()).unwrap_or_default();hash.update(&(value.len()as u64).to_le_bytes());hash.update(value);}hash.finalize().into()}
async fn preview_access(state:&AppState,source:&str,headers:&HeaderMap,background:&Option<BackgroundPreview>)->AppResult<()>{
 if let Some(background)=background{super::stabilization_jobs::authorize_source_owner(state,source,&background.owner).await}else{super::library::authorize_source_access(state,source,headers).await}
}
pub(super) async fn execute_preview(state:AppState,headers:HeaderMap,request:PreviewFrameRequest,background:Option<BackgroundPreview>)->AppResult<Json<Value>>{
 let (metadata,_)=execute_frame(state,headers,request,background,false,None,None,None).await?;Ok(Json(metadata))
}
pub(super) async fn render_export_frame(state:AppState,headers:HeaderMap,request:PreviewFrameRequest)->AppResult<crate::domain::group_surface::GroupSurface>{
 let (_,surface)=execute_frame(state,headers,request,None,true,None,None,None).await?;surface.ok_or_else(||AppError::bad_request("export frame missing"))
}
pub(super) async fn render_export_frame_owned(state:AppState,request:PreviewFrameRequest,background:BackgroundPreview)->AppResult<crate::domain::group_surface::GroupSurface>{
 let (_,surface)=execute_frame(state,HeaderMap::new(),request,Some(background),true,None,None,None).await?;surface.ok_or_else(||AppError::bad_request("export frame missing"))
}
pub(super) async fn render_video_frame(state:AppState,headers:HeaderMap,request:PreviewFrameRequest,background:Option<BackgroundPreview>,canvas:(crate::domain::filmcraft_export_geometry::Geometry,bool),labels:Vec<(crate::domain::filmcraft_export_text::TextOverlay,String)>,image_overlay:Option<crate::domain::filmcraft_export_image::ResolvedImage>)->AppResult<crate::domain::group_surface::GroupSurface>{
 let (_,surface)=execute_frame(state,headers,request,background,true,Some(canvas),Some(std::sync::Arc::new(labels)),image_overlay).await?;surface.ok_or_else(||AppError::bad_request("export frame missing"))
}
pub async fn export_frame(State(state):State<AppState>,headers:HeaderMap,ApiJson(request):ApiJson<PreviewFrameRequest>)->AppResult<Response>{
 let (_,surface)=execute_frame(state.clone(),headers,request,None,true,None,None,None).await?;
 let surface=surface.ok_or_else(||AppError::bad_request("export frame missing"))?;
 let bytes=state.cpu_pool.execute(CancellationToken::new(),move |_|{
  use image::ImageEncoder;
  let mut bytes=Vec::new();image::codecs::png::PngEncoder::new(&mut bytes).write_image(&surface.rgba,surface.width as u32,surface.height as u32,image::ExtendedColorType::Rgba8)?;Ok(bytes)
 }).await.map_err(preview_cpu_error)?;
 Ok(([(header::CONTENT_TYPE,"image/png"),(header::CONTENT_DISPOSITION,"attachment; filename=frame.png"),(header::CACHE_CONTROL,"no-store")],bytes).into_response())
}
async fn execute_frame(state:AppState,headers:HeaderMap,request:PreviewFrameRequest,background:Option<BackgroundPreview>,full_size:bool,video_canvas:Option<(crate::domain::filmcraft_export_geometry::Geometry,bool)>,export_labels:Option<std::sync::Arc<Vec<(crate::domain::filmcraft_export_text::TextOverlay,String)>>>,export_image:Option<crate::domain::filmcraft_export_image::ResolvedImage>)->AppResult<(Value,Option<crate::domain::group_surface::GroupSurface>)>{
 use std::sync::{Arc,atomic::{AtomicBool,Ordering}};
 use sha2::Digest;
 use crate::domain::{filmcraft_source_frame::{self,FrameOptions},filmcraft_generator_source_provider::{GeneratorSourceProvider,ImageSourceProvider,VideoSourceProvider,VideoSourceFrame,VideoFrames},filmcraft_source_pixels,transition_kernel_pack::context::RenderContext};
 let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid preview sequence ID"))?;
 if request.time_ticks.is_some()==request.time_frames.is_some(){return Err(AppError::bad_request("provide exactly one preview time: timeTicks or timeFrames"))}
 if request.sources.len()>64{return Err(AppError::bad_request("too many preview source bindings"))}
 let mut paths=BTreeMap::new();let mut bound_entries=BTreeMap::new();
 for (item,source) in &request.sources{
  let item=item.parse::<u64>().map_err(|_|AppError::bad_request("invalid preview source item ID"))?;
  preview_access(&state,source,&headers,&background).await?;
  let entry=state.library.get(source).await.filter(|e|e.kind=="source").ok_or_else(||AppError::not_found("preview media source not found"))?;
  let path=state.library.resolve_media_path(&entry).await.map_err(|_|AppError::not_found("preview media source not found"))?;
  paths.insert(item,(path,entry.media_type.as_deref()==Some("video")));bound_entries.insert(item,entry);
 }
 let permit=Arc::new(if background.is_some(){state.acquire_render_slot().await.map_err(|_|AppError::service_unavailable("analysis render queue unavailable"))?}else{tokio::time::timeout(std::time::Duration::from_secs(2),state.acquire_render_slot()).await
  .map_err(|_|AppError::service_unavailable("preview queue is busy"))?
  .map_err(|_|AppError::service_unavailable("preview queue unavailable"))?});
 let cancel=background.as_ref().map(|b|b.cancel.child_token()).unwrap_or_default();let _cancel_guard=cancel.clone().drop_guard();
 struct CancelOnDrop(Arc<AtomicBool>);impl Drop for CancelOnDrop{fn drop(&mut self){self.0.store(true,Ordering::Relaxed)}}
 let flag=Arc::new(AtomicBool::new(false));let _flag_guard=CancelOnDrop(flag.clone());
 let effects=request.effects;let captions=request.captions;
 let principal=background.as_ref().map(|b|b.principal).unwrap_or_else(||preview_principal(&headers));
 let binding_hash=sha2::Sha256::digest(&serde_json::to_vec(&request.sources).map_err(|e|AppError::internal("preview bindings",e))?);
 // Each CPU closure retains the admission permit even if the HTTP future is dropped.
 let access_sources:Vec<_>=request.sources.values().cloned().collect();
 let work=async{
  let mut revisions=Vec::new();for(id,(path,video))in &paths{let revision=crate::services::filmcraft_video_preview::SourceVideoRevision::read(path).await.map_err(|e|AppError::bad_request(e.to_string()))?;revisions.push((*id,path.clone(),*video,revision));}
  let preparation_permit=permit.clone();
  let mut prepared=state.cpu_pool.execute(cancel.clone(),move |_|{
   let _permit=preparation_permit;
   let document=decode(&request.document).map_err(|e|anyhow::anyhow!("{e:?}"))?;
   let at=edit_delta(&document,sequence,request.time_ticks,request.time_frames).map_err(|e|anyhow::anyhow!("invalid preview time: {e:?}"))?;
   let index=ProjectIndex::new(&document)?;
   let seq=index.item(sequence).and_then(|v|v.pointer("/kind/Sequence")).ok_or_else(||anyhow::anyhow!("preview sequence not found"))?;
   let dimension=|key:&str|->anyhow::Result<u32>{let n=seq["settings"][key].as_u64().filter(|n|*n>0&&*n<=u32::MAX as u64).ok_or_else(||anyhow::anyhow!("invalid preview dimensions"))?;Ok(n as u32)};
   let (w,h)=(dimension("width")?,dimension("height")?);
   let scale=if let Some((geometry,_))=video_canvas{geometry.render_scale}else if full_size{anyhow::ensure!(w<=3840&&h<=2160,"frame export dimensions exceed 3840x2160");1.}else{(480./w as f32).min(270./h as f32).min(1.)};
   let context=RenderContext::new_with_budget(flag,1,if full_size{3840*2160}else{1920*1080},if full_size{512*1024*1024}else{128*1024*1024}).map_err(anyhow::Error::msg)?;
   let mut images=BTreeMap::new();let mut videos=BTreeMap::new();let mut decoded_bytes=0usize;let mut sequence_rates=BTreeMap::new();
   for (id,(path,video)) in paths{
    anyhow::ensure!(!context.is_cancelled(),"preview source preparation cancelled");
    let source=index.resolve_media(id).filter(|m|m.item_id==id&&m.media.pointer("/media/File").is_some()).ok_or_else(||anyhow::anyhow!("preview binding must identify root file media"))?;
    if source.media["info"]["video"].is_null()&&!source.media["info"]["audio"].is_null(){continue}
    if source.media["info"]["kind"]=="ImageSequence" {sequence_rates.insert(id,crate::domain::filmcraft_image_sequence::rate(source.media)?);videos.insert(id,path);continue}
    if video{videos.insert(id,path);continue}
    anyhow::ensure!(std::fs::metadata(&path)?.len()<=64*1024*1024,"preview source file budget exceeded");
    let image=crate::domain::group_media::decode_image_surface(&path)?;
    decoded_bytes=decoded_bytes.checked_add(image.rgba.capacity()).ok_or_else(||anyhow::anyhow!("preview decoded image size overflow"))?;
    anyhow::ensure!(decoded_bytes<=64*1024*1024,"preview decoded source budget exceeded");
    images.insert(id,image);
   }
   let document_hash:[u8;32]=sha2::Sha256::digest(document.encode_preserved()?).into();
   Ok((document,at,scale,context,Arc::new(images),Arc::new(videos),decoded_bytes,document_hash,sequence_rates))
  }).await.map_err(preview_cpu_error)?;
  let mut sequence_readers=BTreeMap::new();
  for id in prepared.8.keys(){
   let entry=bound_entries.get(id).ok_or_else(||AppError::bad_request("sequence binding disappeared"))?;
   let mut reader=super::filmcraft_image_sequence::open(&state,&headers,background.as_ref().map(|b|b.owner.as_str()),entry,cancel.clone()).await?;
   reader.clear_pixels();sequence_readers.insert(*id,reader);
  }
  let mut cache_key=sha2::Sha256::new();cache_key.update(&principal);cache_key.update(binding_hash.as_slice());cache_key.update(&sequence.to_le_bytes());cache_key.update(&[effects as u8,captions as u8]);cache_key.update(&prepared.7);
  for(id,path,video,revision)in &revisions{if crate::services::filmcraft_video_preview::SourceVideoRevision::read(path).await.map_err(|e|AppError::bad_request(e.to_string()))?!=*revision{return Err(AppError::bad_request("source changed during preview preparation"))}cache_key.update(&id.to_le_bytes());let path=path.as_os_str().as_encoded_bytes();cache_key.update(&(path.len()as u64).to_le_bytes());cache_key.update(path);cache_key.update(&[*video as u8]);cache_key.update(&revision.fingerprint());}
  if full_size{cache_key.update(b"full-size-frame-v1");}if let Some(geometry)=video_canvas{cache_key.update(b"video-output-geometry-v2");cache_key.update(serde_json::to_vec(&geometry).map_err(|e|AppError::internal("export geometry cache",e))?);}
  if let Some(labels)=&export_labels{cache_key.update(b"export-labels-v1");cache_key.update(serde_json::to_vec(labels.as_ref()).map_err(|e|AppError::internal("export label cache",e))?);}
  let cache_base=cache_key;
  let mut cache_key=cache_base.clone();let mut cache_verified=true;
  for(id,reader)in &mut sequence_readers {cache_key.update(b"image-sequence-v1");cache_key.update(id.to_le_bytes());if let Some(revision)=reader.revision().await?{cache_key.update(revision);}else{cache_verified=false;}}
  let cache_key:[u8;32]=cache_key.finalize().into();
  let mut frames=Arc::new(VideoFrames::new());
  let analysis=crate::domain::filmcraft_analysis_cache::SharedAnalysis::default();
  if cache_verified {if let Some(paths)=state.filmcraft_video_previews.restore_completed_paths(cache_key).await.map_err(|e|AppError::bad_request(e.to_string()))?{analysis.lock().map_err(|_|AppError::bad_request("analysis state lock poisoned"))?.seed(paths).map_err(|e|AppError::bad_request(e.to_string()))?;}}
  let reused_paths=analysis.lock().map_err(|_|AppError::bad_request("analysis state lock poisoned"))?.ready_paths().values().filter(|p|p.is_some()).count();
  let resumed_frames=if let Some(background)=background.as_ref().filter(|_|cache_verified){match super::stabilization_jobs::restore_checkpoint(&state,&background.job_id,cache_key,&analysis).await{Ok(frames)=>frames,Err(error)=>{tracing::warn!(error=%error,"ignoring unavailable analysis checkpoint");0}}}else{0};
  let mut source_revisions:BTreeMap<_,_>=revisions.iter().filter(|r|r.2).map(|r|(r.0,r.3.clone())).collect();
  let mut progress_at=std::time::Instant::now();let mut checkpoint_at=std::time::Instant::now();let mut saved_ready=std::collections::BTreeSet::new();
  loop{
   if let Some(background)=background.as_ref().filter(|_|cache_verified){if checkpoint_at.elapsed()>=std::time::Duration::from_secs(1){if let Err(error)=super::stabilization_jobs::save_checkpoint(&state,&background.job_id,cache_key,&analysis,&mut saved_ready).await{tracing::warn!(error=%error,"analysis checkpoint unavailable");}checkpoint_at=std::time::Instant::now();}}
   if let Some(background)=&background{if progress_at.elapsed()>=std::time::Duration::from_millis(250){let progress=analysis.lock().map_err(|_|AppError::bad_request("analysis lock poisoned"))?.progress_percent();state.update_job_if_open(&background.job_id,|job|job.progress=Some(job.progress.unwrap_or(0.).max(progress))).await;progress_at=std::time::Instant::now();}}
   analysis.lock().map_err(|_|AppError::bad_request("analysis state lock poisoned"))?.begin_attempt();
   let render_analysis=analysis.clone();let render_labels=export_labels.clone();let render_image=export_image.clone();
   let render_permit=permit.clone();let render_frames=frames.clone();
   let (returned,pending,output)=state.cpu_pool.execute(cancel.clone(),move |_|{
    let _permit=render_permit;
    let (document,at,scale,context,images,videos,_,_,_)=&prepared;
    let options=FrameOptions{scale:*scale,effects,captions,working_output:true,..FrameOptions::default()};
    let (result,pending)=if videos.is_empty(){
     let mut provider:Box<dyn filmcraft_source_frame::SourceLayerProvider>=if images.is_empty(){Box::new(GeneratorSourceProvider)}else{Box::new(ImageSourceProvider{images:images.clone()})};
     (filmcraft_source_frame::render(context,document,sequence,*at,options,provider.as_mut()),None)
    }else{
     let mut provider=VideoSourceProvider{analysis:render_analysis,images:images.clone(),videos:videos.keys().copied().collect(),frames:render_frames,pending:None};
     let result=filmcraft_source_frame::render(context,document,sequence,*at,options,&mut provider);(result,provider.pending)
    };
    if let Some(key)=pending{anyhow::ensure!(result.is_err()&&context.pixel_bytes_in_use()==0,"pending preview retained renderer storage");return Ok((prepared,Some(key),None))}
    let mut image=result?;
    if let Some((geometry,_))=video_canvas {image=geometry.fit(context,image)?;}
    if let Some(overlay)=render_image {
     let prepared=crate::domain::filmcraft_export_image::prepare(context,&overlay.1,&overlay.0,image.w)?;
     crate::domain::filmcraft_export_image::composite(context,&mut image,&prepared,&overlay.0)?;
    }
    if let Some(labels)=render_labels.filter(|labels|!labels.is_empty()) {
     image=context.image_work(&[],||{
      let _scratch=context.reserve_scratch(image.w*image.h*8);
      for (settings,text) in labels.iter(){
       crate::domain::filmcraft_export_text::paint(&mut image.px,image.w,image.h,settings,text,&mut ||{anyhow::ensure!(!context.is_cancelled(),"export label cancelled");Ok(())}).map_err(|e|e.to_string())?;
      }
      Ok(image)
     }).map_err(anyhow::Error::msg)?;
    }
    anyhow::ensure!(if full_size{image.w<=3840&&image.h<=2160}else{image.w<=480&&image.h<=270},"frame output dimensions exceeded");
    let output=if video_canvas.is_some_and(|(_,opaque)|opaque){filmcraft_source_pixels::encode_srgb_over_black(context,&image)?}else{filmcraft_source_pixels::encode_srgb(context,&image)?};let surface=output.surface();
    const HEX:&[u8;16]=b"0123456789abcdef";let mut rgba_hex=if full_size{String::new()}else{String::with_capacity(surface.rgba.len()*2)};
    if !full_size {for row in surface.rgba.chunks(surface.width*4){anyhow::ensure!(!context.is_cancelled(),"preview encoding cancelled");for b in row{rgba_hex.push(HEX[(b>>4)as usize]as char);rgba_hex.push(HEX[(b&15)as usize]as char);}}}
    let value=json!({"kind":"sequence","sequenceId":sequence.to_string(),"sampleTicks":at.to_string(),"ptsTicks":at.to_string(),"width":surface.width,"height":surface.height,"scale":scale,"effects":effects,"captions":captions,"rgbaHex":rgba_hex});
    let exported=full_size.then(||surface.clone());
    drop(output);drop(image);anyhow::ensure!(context.pixel_bytes_in_use()==0,"preview retained renderer storage");
    Ok((prepared,None,Some((value,exported))))
   }).await.map_err(preview_cpu_error)?;
   prepared=returned;
   if let Some((mut output,exported))=output{if background.is_some(){output["analysisResumedFrames"]=json!(resumed_frames);output["analysisReusedPaths"]=json!(reused_paths);}for source in &access_sources{preview_access(&state,source,&headers,&background).await?;}for(_,path,_,revision)in &revisions{if crate::services::filmcraft_video_preview::SourceVideoRevision::read(path).await.map_err(|e|AppError::bad_request(e.to_string()))?!=*revision{return Err(AppError::bad_request("source changed during preview publication"))}}let mut final_key=cache_base.clone();let mut final_verified=true;for(id,reader)in &mut sequence_readers {reader.verify().await?;final_key.update(b"image-sequence-v1");final_key.update(id.to_le_bytes());if let Some(revision)=reader.revision().await?{final_key.update(revision);}else{final_verified=false;}}if final_verified {let paths=analysis.lock().map_err(|_|AppError::bad_request("analysis state lock poisoned"))?.ready_paths();state.filmcraft_video_previews.persist_completed_paths(final_key.finalize().into(),paths).await.map_err(|e|AppError::bad_request(e.to_string()))?;}return Ok((output,exported))}
   let key=pending.ok_or_else(||AppError::internal("preview missing result",anyhow::anyhow!("missing source request")))?;
   if frames.contains_key(&key){return Err(AppError::bad_request("preview decoded frame request budget exceeded"))}
   let path=prepared.5.get(&key.item).ok_or_else(||AppError::bad_request("video source is not bound to the library"))?;
   for(id,reader)in &mut sequence_readers {if *id!=key.item {reader.clear_pixels();}}
   let decoded=if let Some(reader)=sequence_readers.get_mut(&key.item){
    let rate=*prepared.8.get(&key.item).ok_or_else(||AppError::bad_request("sequence rate missing"))?;
    Some(Arc::new(VideoSourceFrame{surface:reader.frame(rate,key.ticks).await?,color:filmcraft_color::ColorInfo::SRGB_FULL}))
   }else{
   let frame=crate::services::filmcraft_video_preview::frame_cached_full(&state.filmcraft_video_previews,&state.process_runtime,path,key.ticks).await.map_err(|e|AppError::bad_request(e.to_string()))?;
   let decoded=if let Some(frame)=frame{
    if let Some(previous)=source_revisions.get(&key.item){if previous!=&frame.revision(){return Err(AppError::bad_request("video source changed during preview"))}}else{source_revisions.insert(key.item,frame.revision());}
    Some(Arc::new(VideoSourceFrame{surface:frame.surface.clone(),color:frame.color}))
   }else{None};decoded};
   crate::domain::filmcraft_analysis_cache::admit_frame(&analysis,Arc::make_mut(&mut frames),key,decoded,prepared.6,64*1024*1024,128).map_err(|e|AppError::bad_request(e.to_string()))?;
  }
 };
 let deadline=if background.is_some()||full_size{state.job_timeout()}else{std::time::Duration::from_secs(15)};
 tokio::select!{_ = cancel.cancelled()=>Err(AppError::service_unavailable("preview analysis cancelled")),result=tokio::time::timeout(deadline,work)=>result.map_err(|_|AppError::service_unavailable("preview deadline exceeded"))?}
}
fn preview_cpu_error(e:CpuTaskError)->AppError{match e{CpuTaskError::Work(e)=>AppError::bad_request(e.to_string()),CpuTaskError::Saturated=>AppError::service_unavailable("preview CPU queue is full"),e=>AppError::internal("source project preview",anyhow::anyhow!(e))}}


#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ParameterReadRequest {document:Value,sequence_id:String,clip_id:String,#[serde(default)] time_ticks:Option<String>,#[serde(default)] time_frames:Option<String>}
pub async fn parameters(State(state):State<AppState>,ApiJson(request):ApiJson<ParameterReadRequest>)->AppResult<Json<Value>>{
 let document=decode(&request.document)?;let sequence=request.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;let clip=request.clip_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid clip ID"))?;let timeline=edit_delta(&document,sequence,request.time_ticks,request.time_frames)?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|crate::domain::filmcraft_parameter_edit::inspect(&document,sequence,clip,timeline)).await.map_err(|error|AppError::bad_request(error.to_string()))?;Ok(Json(result))
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ParameterEditRequest {document:Value,address:crate::domain::filmcraft_parameter_edit::Address,command:crate::domain::filmcraft_parameter_edit::Command,#[serde(default)] time_ticks:Option<String>,#[serde(default)] time_frames:Option<String>}
pub async fn edit_parameter(State(state):State<AppState>,ApiJson(request):ApiJson<ParameterEditRequest>)->AppResult<Json<Value>>{
 let document=decode(&request.document)?;let sequence=request.address.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid sequence ID"))?;let timeline=edit_delta(&document,sequence,request.time_ticks,request.time_frames)?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{let changed=crate::domain::filmcraft_parameter_edit::edit(&document,&request.address,request.command,timeline)?;manifest(&changed).map_err(|error|anyhow::anyhow!("{error:?}"))}).await.map_err(|error|AppError::bad_request(error.to_string()))?;Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct MaskEditRequest {document:Value,address:crate::domain::filmcraft_parameter_edit::EffectAddress,command:crate::domain::filmcraft_parameter_edit::MaskCommand}
pub async fn edit_mask(State(state):State<AppState>,ApiJson(request):ApiJson<MaskEditRequest>)->AppResult<Json<Value>> {
 let document=decode(&request.document)?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_| {let changed=crate::domain::filmcraft_parameter_edit::edit_mask(&document,&request.address,request.command)?;manifest(&changed).map_err(|error|anyhow::anyhow!("{error:?}"))}).await.map_err(preview_cpu_error)?;
 Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct MaskTrackRequest {document:Value,address:crate::domain::filmcraft_parameter_edit::Address,sources:BTreeMap<String,String>,direction:crate::domain::filmcraft_mask_track::Direction,#[serde(default)] cursor_ticks:Option<String>,#[serde(default)] time_ticks:Option<String>,#[serde(default)] time_frames:Option<String>}
fn mask_tracking_output(document:&SourceProjectDocument,address:&crate::domain::filmcraft_parameter_edit::Address,plan:&crate::domain::filmcraft_mask_track::Plan,a:&crate::domain::group_surface::GroupSurface,b:&crate::domain::group_surface::GroupSurface)->anyhow::Result<Value>{
 let result=crate::domain::filmcraft_mask_track::apply(document,address,plan,a,b)?;
 let (mut output,status,inliers,ticks)=match result{Some((changed,inliers))=>(manifest(&changed).map_err(|e|anyhow::anyhow!("{e:?}"))?,"tracked",inliers,plan.to.ok_or_else(||anyhow::anyhow!("tracking boundary"))?),None=>(manifest(document).map_err(|e|anyhow::anyhow!("{e:?}"))?,"lost",0,plan.from)};
 output["tracking"]=json!({"status":status,"sourceTicks":ticks.to_string(),"more":status=="tracked"&&plan.more,"inliers":inliers});Ok(output)
}
pub async fn track_mask(State(state):State<AppState>,headers:HeaderMap,ApiJson(request):ApiJson<MaskTrackRequest>)->AppResult<Json<Value>> {
 use crate::domain::filmcraft_mask_track;
 if request.sources.len()>64{return Err(AppError::bad_request("too many tracking bindings"))}
 let document=decode(&request.document)?;let sequence=request.address.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid tracking sequence"))?;
 let timeline=edit_delta(&document,sequence,request.time_ticks,request.time_frames)?;
 let cursor=request.cursor_ticks.map(|v|v.parse::<i64>()).transpose().map_err(|_|AppError::bad_request("invalid tracking source cursor"))?;
 let plan=filmcraft_mask_track::plan(&document,&request.address,timeline,cursor,request.direction).map_err(|e|AppError::bad_request(e.to_string()))?;
 let Some(to)=plan.to else {let mut output=manifest(&document)?;output["tracking"]=json!({"status":"boundary","sourceTicks":plan.from.to_string(),"more":false,"inliers":0});return Ok(Json(output))};
 if plan.generated {
  let cancel=CancellationToken::new();let _guard=cancel.clone().drop_guard();
  let output=state.cpu_pool.execute(cancel,move |cancel|{
   let scale=plan.generator_scale();
   let a=crate::domain::filmcraft_generator_video::source_frame(&document,plan.source,plan.from,scale,||cancel.is_cancelled())?;
   let b=crate::domain::filmcraft_generator_video::source_frame(&document,plan.source,to,scale,||cancel.is_cancelled())?;
   let output=mask_tracking_output(&document,&request.address,&plan,&a.surface,&b.surface)?;
   anyhow::ensure!(!cancel.is_cancelled(),"tracking cancelled");Ok(output)
  }).await.map_err(preview_cpu_error)?;
  return Ok(Json(output))
 }
 let source=request.sources.get(&plan.source.to_string()).ok_or_else(||AppError::bad_request("tracking source is not bound to the library"))?;
 super::library::authorize_source_access(&state,source,&headers).await?;
 let entry=state.library.get(source).await.filter(|e|e.kind=="source").ok_or_else(||AppError::not_found("tracking media source not found"))?;
 let cancel=CancellationToken::new();let _guard=cancel.clone().drop_guard();
 if let Some(rate)=plan.sequence_rate {
  let pair=super::filmcraft_image_sequence::pair(&state,&headers,&entry,rate,plan.from,to,cancel.clone()).await?;
  let a=pair.a.clone();let b=pair.b.clone();
  let output=state.cpu_pool.execute(cancel,move |cancel|{
   anyhow::ensure!(!cancel.is_cancelled(),"tracking cancelled");
   let output=mask_tracking_output(&document,&request.address,&plan,&a,&b)?;
   anyhow::ensure!(!cancel.is_cancelled(),"tracking cancelled");Ok(output)
  }).await.map_err(preview_cpu_error)?;
  pair.verify(&state,&headers).await?;
  return Ok(Json(output))
 }
 let path=state.library.resolve_media_path(&entry).await.map_err(|_|AppError::not_found("tracking media source not found"))?;
 let (a,b,revision)=if entry.media_type.as_deref()==Some("image") {
  let revision=crate::services::filmcraft_video_preview::SourceVideoRevision::read(&path).await.map_err(|e|AppError::bad_request(e.to_string()))?;
  let image_path=path.clone();
  let image=state.cpu_pool.execute(cancel.clone(),move |cancel|{
   anyhow::ensure!(!cancel.is_cancelled(),"tracking cancelled");
   anyhow::ensure!(std::fs::metadata(&image_path)?.len()<=64*1024*1024,"tracking image file budget exceeded");
   let image=crate::domain::group_media::decode_image_surface(&image_path)?;
   anyhow::ensure!(!cancel.is_cancelled(),"tracking cancelled");
   Ok(std::sync::Arc::new(image))
  }).await.map_err(preview_cpu_error)?;
  (image.clone(),image,revision)
 }else{
  let a=crate::services::filmcraft_video_preview::frame_cached_full(&state.filmcraft_video_previews,&state.process_runtime,&path,plan.from).await.map_err(|e|AppError::bad_request(e.to_string()))?.ok_or_else(||AppError::bad_request("tracking seed frame unavailable"))?;
  let b=crate::services::filmcraft_video_preview::frame_cached_full(&state.filmcraft_video_previews,&state.process_runtime,&path,to).await.map_err(|e|AppError::bad_request(e.to_string()))?.ok_or_else(||AppError::bad_request("tracking next frame unavailable"))?;
  if !a.same_source_revision(&b){return Err(AppError::bad_request("tracking source changed during decoding"))}
  (a.surface.clone(),b.surface.clone(),b.revision())
 };
 let output=state.cpu_pool.execute(cancel,move |cancel|{
  anyhow::ensure!(!cancel.is_cancelled(),"tracking cancelled");
  let output=mask_tracking_output(&document,&request.address,&plan,&a,&b)?;
  anyhow::ensure!(!cancel.is_cancelled(),"tracking cancelled");Ok(output)
 }).await.map_err(preview_cpu_error)?;
 super::library::authorize_source_access(&state,source,&headers).await?;
 if crate::services::filmcraft_video_preview::SourceVideoRevision::read(&path).await.map_err(|e|AppError::bad_request(e.to_string()))?!=revision{return Err(AppError::bad_request("tracking source changed before publication"))}
 Ok(Json(output))
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct MaskMonitorRequest {document:Value,#[serde(default)] create_mask:bool,address:crate::domain::filmcraft_parameter_edit::Address,#[serde(default)] operation:crate::domain::filmcraft_mask_monitor::Operation,#[serde(default)] time_ticks:Option<String>,#[serde(default)] time_frames:Option<String>}
pub async fn mask_monitor(State(state):State<AppState>,ApiJson(request):ApiJson<MaskMonitorRequest>)->AppResult<Json<Value>> {
 let document=decode(&request.document)?;let sequence=request.address.sequence_id.parse::<u64>().map_err(|_|AppError::bad_request("invalid mask sequence"))?;let at=edit_delta(&document,sequence,request.time_ticks,request.time_frames)?;
 let result=state.cpu_pool.execute(CancellationToken::new(),move |_|{let(overlay,changed)=if request.create_mask {crate::domain::filmcraft_mask_monitor::evaluate_new(&document,&request.address,at,request.operation)?}else{crate::domain::filmcraft_mask_monitor::evaluate(&document,&request.address,at,request.operation)?};let project=changed.as_ref().map(manifest).transpose().map_err(|e|anyhow::anyhow!("{e:?}"))?;Ok(json!({"overlay":overlay,"project":project}))}).await.map_err(preview_cpu_error)?;Ok(Json(result))
}
