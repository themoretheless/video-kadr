//! Concat v1 project conversion with explicit registered-media bindings; never opens source paths.
use super::{
    composition::{Composition, CompositionSource},
    concat_effect_upgrade::{native_effect, upgrade, AppliedFilter},
    nested_composition::{NestedCompositionDocument, NestedCompositionProject},
};
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportRequest {
    pub schema_version: u32,
    pub document: Value,
    pub media_bindings: BTreeMap<String, CompositionSource>,
    /// Preserve Concat resource IDs and bind physical library files separately.
    #[serde(default)]
    pub preserve_media_ids: bool,
    #[serde(default)]
    pub font_bindings:BTreeMap<String,FontBinding>,
}
#[derive(Debug,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontBinding {#[serde(default)] pub data:Option<String>,#[serde(default)] pub fallback:bool}
fn import_fonts(document:&Value,bindings:&BTreeMap<String,FontBinding>)->Result<Vec<super::concat_project_fonts::FontResource>>{
 let fonts=entries(document,"fonts")?;ensure!(fonts.len()<=32,"Font limit is 32 files");let mut paths=BTreeSet::new();let mut resources=vec![];
 for font in fonts{fields(font,&["family","path"],"document.fonts")?;let path=text(font,"path")?;ensure!(!path.is_empty()&&path.len()<=4096&&paths.insert(path),"Invalid or duplicate font source path");let family=text(font,"family")?;let binding=bindings.get(path).with_context(||format!("Select font file for {family}; source path {path} is never opened"))?;ensure!(binding.data.is_some()!=binding.fallback,"Choose font bytes or explicit system fallback");if let Some(data)=&binding.data{resources.push(super::concat_project_fonts::FontResource{family:family.into(),data:data.clone()});}}
 ensure!(bindings.keys().all(|key|paths.contains(key.as_str())),"Font binding names an unknown source path");super::concat_project_fonts::bank(&resources)?;Ok(resources)
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub schema_version: u32,
    pub status: &'static str,
    pub project: Option<NestedCompositionProject>,
    pub diagnostics: Vec<String>,
    pub source_document: Value,
    pub timeline_ids: BTreeMap<String, String>,
}
fn number(v: &Value, key: &str, default: f64) -> Result<f64> {
    let n = match v.get(key) {
        None => default,
        Some(n) => n
            .as_f64()
            .with_context(|| format!("{key}: expected number"))?,
    };
    ensure!(n.is_finite(), "{key}: nonfinite number");
    Ok(n)
}
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("{key}: missing string"))
}
fn entries<'a>(v: &'a Value, key: &str) -> Result<&'a [Value]> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(a)) => Ok(a),
        _ => bail!("{key}: expected array"),
    }
}
fn fields(v: &Value, allowed: &[&str], path: &str) -> Result<()> {
    let object = v
        .as_object()
        .with_context(|| format!("{path}: expected object"))?;
    for key in object.keys() {
        ensure!(
            allowed.contains(&key.as_str()),
            "{path}.{key}: unsupported field (original document retained)"
        );
    }
    Ok(())
}
fn inactive(v: &Value, keys: &[&str], path: &str) -> Result<()> {
    for key in keys {
        if let Some(value) = v.get(key) {
            ensure!(
                value.is_null()
                    || value.as_array().is_some_and(Vec::is_empty)
                    || value == &json!(false),
                "{path}.{key}: conversion is not implemented"
            );
        }
    }
    Ok(())
}
fn ticks(seconds: f64) -> Result<u64> {
    ensure!(
        seconds.is_finite() && seconds >= 0. && seconds <= 86400.,
        "time outside native 24-hour range"
    );
    Ok((seconds * 1_000_000.).round() as u64)
}
fn value(v: f64) -> Value {
    json!({"mode":"constant","value":v})
}
/// Supplied only by server orchestration after authorizing/verifying ready assets.
/// This is deliberately not a deserializable field of the public import request.
#[derive(Debug,Clone)]
pub struct ReadyStillBinding {
    pub assets:super::concat_cutout_binding::StillAssets,
    pub source_settings:super::concat_cutout_import::Plan,
    pub source_hash:String,pub model_hash:String,pub runtime_hash:String,
}
pub type StillBindings=BTreeMap<String,BTreeMap<String,ReadyStillBinding>>;
/// Supplied only after server verification of store coverage and region ownership.
/// This schema binding does not itself grant an unsaved project cache ownership.
#[derive(Debug,Clone)]
pub struct ReadyVideoBinding {
    pub source:CompositionSource,pub source_settings:super::concat_cutout_import::Plan,
    pub binding:super::concat_video_binding::Binding,pub regions:Vec<CompositionSource>,
}
pub type VideoBindings=BTreeMap<String,BTreeMap<String,ReadyVideoBinding>>;

pub fn import(request: ImportRequest) -> Result<ImportReport> {
    import_with_still_bindings(request,&StillBindings::new())
}
pub fn import_with_still_bindings(request:ImportRequest,ready:&StillBindings)->Result<ImportReport>{
    import_with_ready_bindings(request,ready,&VideoBindings::new())
}
pub fn import_with_ready_bindings(request:ImportRequest,ready:&StillBindings,videos:&VideoBindings)->Result<ImportReport>{
    ensure!(request.schema_version == 1, "unsupported import schema");
    ensure!(serde_json::to_vec(&request.document)?.len()<=4*1024*1024,"Concat source document exceeds 4 MiB");
    let original = request.document;
    let converted = convert(&original, &request.media_bindings,&request.font_bindings,ready,videos,request.preserve_media_ids);
    let font_warnings=original.get("fonts").and_then(Value::as_array).into_iter().flatten().filter(|font|font.get("path").and_then(Value::as_str).and_then(|path|request.font_bindings.get(path)).is_some_and(|binding|binding.fallback)).map(|font|format!("Шрифт {} не встроен: выбрана системная замена.",font.get("family").and_then(Value::as_str).unwrap_or(""))).collect::<Vec<_>>();
    Ok(match converted {
        Ok((project, timeline_ids)) => ImportReport {
            schema_version: 1,
            status: "converted",
            project: Some(project),
            diagnostics: {let mut diagnostics=if original.get("timelines").and_then(Value::as_array).into_iter().flatten()
                .flat_map(|timeline|timeline.get("clips").and_then(Value::as_array).into_iter().flatten())
                .filter(|clip|matches!(clip.get("kind").and_then(Value::as_str),Some("audio"|"video")))
                .flat_map(|clip|clip.get("filters").and_then(Value::as_array).into_iter().flatten())
                .any(|filter|filter.get("keys").and_then(Value::as_object).is_some_and(|keys|keys.values().any(|keys|keys.as_array().is_some_and(|keys|!keys.is_empty())))){
                vec!["Ключи параметров аудиоэффектов сохранены в проекте как concatSourceKeys и в sourceDocument отчёта. Звук использует статические params, как исходная аудиоцепочка Concat.".into()]
            }else{vec![]};diagnostics.extend(font_warnings);diagnostics},
            source_document: original,
            timeline_ids,
        },
        Err(error) => ImportReport {
            schema_version: 1,
            status: "needs_attention",
            project: None,
            diagnostics: vec![format!("{error:#}")],
            source_document: original,
            timeline_ids: BTreeMap::new(),
        },
    })
}
/// Ready assets are already server verified. Rebind editorial IDs only; hashes,
/// encoded assets, pixel corrections and physical library ownership stay unchanged.
fn rebind_ready_stills(
    timelines:&[Value], ready:&StillBindings,
    physical:&BTreeMap<String,CompositionSource>, logical:&BTreeMap<String,CompositionSource>,
    project_media:&mut BTreeMap<super::composition::SourceId,super::project_media::ProjectMediaItem>,
)->Result<StillBindings>{
    use super::composition::SourceId;
    use sha2::{Digest,Sha256};
    let mut descriptors=BTreeMap::new();
    let mut reserved:BTreeSet<SourceId>=logical.values().map(|s|s.id.clone()).collect();
    for clips in ready.values(){for binding in clips.values(){
        for asset in [&binding.assets.original,&binding.assets.copy,&binding.assets.mask].into_iter().chain(binding.assets.regions.iter()){
            asset.validate()?;
            if let Some(previous)=descriptors.insert(asset.id.clone(),asset.clone()){ensure!(previous==*asset,"conflicting prepared asset metadata");}
            reserved.insert(asset.id.clone());
        }
    }}
    let mut remap:BTreeMap<SourceId,SourceId>=BTreeMap::new();
    let mut result=ready.clone();
    for (timeline,clips) in &mut result{for (clip_id,binding) in clips{
        let clip=timelines.iter().find(|t|t.get("id").and_then(Value::as_str)==Some(timeline.as_str()))
            .and_then(|t|t.get("clips")).and_then(Value::as_array)
            .and_then(|clips|clips.iter().find(|c|c.get("id").and_then(Value::as_str)==Some(clip_id.as_str())))
            .context("unused or stale cutout binding")?;
        binding.assets.snapshot.validate()?;
        ensure!(binding.assets.snapshot.mask_source_id==binding.assets.mask.id.as_str()&&binding.assets.copy.id!=binding.assets.original.id&&binding.assets.copy.id!=binding.assets.mask.id,"prepared cutout source IDs mismatch");
        let media=text(clip,"mediaId")?;
        let physical=physical.get(media).context("prepared cutout media binding missing")?;
        ensure!(&binding.assets.original==physical&&binding.assets.snapshot.source_id==physical.id.as_str(),"prepared cutout original differs from registered media binding");
        let original=logical.get(media).context("logical cutout source missing")?;
        binding.assets.original=original.clone();
        binding.assets.snapshot.source_id=original.id.as_str().to_owned();
        for stroke in &mut binding.assets.snapshot.correction.strokes{
            if let Some(region)=&mut stroke.region{
                ensure!(region.source_id==physical.id.as_str(),"prepared region belongs to another original");
                region.source_id=original.id.as_str().to_owned();
            }
        }
        // A source Concat slot may be named exactly like a processed library
        // asset. Allocate an independent logical name instead of overwriting it.
        for asset in [&mut binding.assets.copy,&mut binding.assets.mask].into_iter().chain(binding.assets.regions.iter_mut()){
            let old=asset.id.clone();
            let new=if let Some(id)=remap.get(&old){id.clone()}else{
                let mut id=old.clone();
                if logical.values().any(|s|s.id==old){
                    let mut nonce=0u64;
                    loop{
                        id=SourceId::parse(format!("concat-cutout-{:x}",Sha256::digest(format!("{}:{nonce}",old.as_str()))))?;
                        if reserved.insert(id.clone()){break}
                        nonce=nonce.checked_add(1).context("cutout ID allocation exhausted")?;
                    }
                }
                remap.insert(old.clone(),id.clone());id
            };
            if new!=old{
                asset.id=new.clone();
                let record=super::project_media::ProjectMediaItem{source:asset.clone(),library_source_id:Some(old.clone()),name:"Ресурс вырезания".into(),placeholder:false,concat_metadata:Default::default()};
                if let Some(previous)=project_media.insert(new.clone(),record.clone()){ensure!(previous==record,"conflicting prepared cutout asset metadata");}
            }
        }
        if let Some(id)=remap.get(&SourceId::parse(&binding.assets.snapshot.mask_source_id)?){binding.assets.snapshot.mask_source_id=id.as_str().to_owned();}
        for stroke in &mut binding.assets.snapshot.correction.strokes{
            if let Some(region)=&mut stroke.region{
                if let Some(id)=remap.get(&SourceId::parse(&region.mask_source_id)?){region.mask_source_id=id.as_str().to_owned();}
            }
        }
        binding.assets.snapshot.validate()?;
    }}
    Ok(result)
}
fn convert(
    document: &Value,
    bindings: &BTreeMap<String, CompositionSource>,
    font_bindings:&BTreeMap<String,FontBinding>,
    ready:&StillBindings,
    videos:&VideoBindings,
    preserve_media_ids:bool,
) -> Result<(NestedCompositionProject, BTreeMap<String, String>)> {
    let physical_bindings=bindings;
    let logical_bindings;
    let bindings=if preserve_media_ids {
        let mut physical=BTreeMap::new();
        logical_bindings=bindings.iter().map(|(id,source)|{
            source.validate()?;
            if let Some(previous)=physical.insert(source.id.clone(),source.clone()) {ensure!(previous==*source,"conflicting registered source metadata");}
            let mut logical=source.clone();logical.id=super::composition::SourceId::parse(id.clone()).with_context(||format!("Concat media ID {id} cannot be represented natively"))?;
            Ok((id.clone(),logical))
        }).collect::<Result<BTreeMap<_,_>>>()?;
        &logical_bindings
    }else{bindings};
    fields(
        document,
        &[
            "concat",
            "version",
            "name",
            "media",
            "fonts",
            "timelines",
            "activeTimelineId",
            "video",
            "tracks",
            "clips",
        ],
        "document",
    )?;
    ensure!(
        document
            .get("version")
            .map_or(true, |v| v.as_u64() == Some(1)),
        "document.version: only Concat v1 is supported"
    );
    let fonts=import_fonts(document,font_bindings)?;
    let media = entries(document, "media")?;
    ensure!(media.len() <= 32, "document.media: native source limit32");
    let mut media_ids = BTreeSet::new();
    let mut sources = BTreeMap::new();
    let mut source_names = serde_json::Map::new();
    let mut project_media = BTreeMap::new();
    let mut binding_owners = BTreeMap::new();
    for item in media {
        fields(
            item,
            &[
                "id",
                "path",
                "name",
                "duration",
                "kind",
                "width",
                "height",
                "frameRate",
                "frameRateFraction",
                "videoCodec",
                "audioCodec",
                "hasAudio",
                "audioTracks",
                "placeholder",
                "colorRange",
                "colorSpace",
                "origin",
            ],
            "document.media",
        )?;
        let id = text(item, "id")?;
        ensure!(media_ids.insert(id.to_owned()), "duplicate media id {id}");
        let binding = bindings.get(id).with_context(|| {
            format!("document.media[{id}]: register/relink media before import")
        })?;
        source_names.insert(
            binding.id.as_str().to_owned(),
            json!(item.get("name").and_then(Value::as_str).unwrap_or(id)),
        );
        ensure!(
            item.get("colorSpace").map_or(true, |v| v == "sdr"),
            "document.media[{id}].colorSpace: HDR mapping is not implemented"
        );
        inactive(
            item,
            &["colorRange"],
            &format!("document.media[{id}]"),
        )?;
        let placeholder = match item.get("placeholder") {
            None => false,
            Some(value) => value.as_bool().context("media.placeholder: expected boolean")?,
        };
        let concat_metadata=super::project_media::capture_concat_metadata(item);
        let owns_record=preserve_media_ids||item.get("placeholder").is_some()||!concat_metadata.is_empty();
        if let Some((previous_id, previous_record)) = binding_owners.insert(binding.id.clone(), (id, owns_record)) {
            ensure!(!owns_record && !previous_record,
                "distinct project media IDs {previous_id} and {id} require distinct registered source bindings");
        }
        if owns_record {
            project_media.insert(binding.id.clone(), super::project_media::ProjectMediaItem {
                source: binding.clone(),
                library_source_id: preserve_media_ids.then(||physical_bindings[id].id.clone()),
                name: item.get("name").and_then(Value::as_str).unwrap_or(id).to_owned(),
                placeholder,
                concat_metadata,
            });
        }
        if let Some(previous) = sources.insert(binding.id.clone(), binding.clone()) {
            ensure!(
                previous == *binding,
                "conflicting registered source metadata"
            );
        }
    }
    ensure!(
        bindings.keys().all(|id| media_ids.contains(id)),
        "binding names an unknown original media id"
    );
    let flat;
    let timelines = if entries(document, "timelines")?.is_empty() {
        flat = vec![
            json!({"id":"TL1","name":"Timeline 1","video":document.get("video").cloned().unwrap_or(json!({})),"tracks":entries(document,"tracks")?,"clips":entries(document,"clips")?}),
        ];
        flat.as_slice()
    } else {
        entries(document, "timelines")?
    };
    ensure!(
        !timelines.is_empty() && timelines.len() <= 16,
        "document.timelines: expected1..16 timelines"
    );
    ensure!(ready.len()<=16&&ready.values().all(|clips|clips.len()<=512),"cutout binding budget exceeded");
    ensure!(videos.len()<=16&&videos.values().all(|clips|clips.len()<=512),"video cutout binding budget exceeded");
    ensure!(ready.iter().all(|(timeline,clips)|clips.keys().all(|clip|!videos.get(timeline).is_some_and(|entries|entries.contains_key(clip)))),"duplicate still/video cutout destination");
    let rebound;
    let ready=if preserve_media_ids{
        rebound=rebind_ready_stills(timelines,ready,physical_bindings,bindings,&mut project_media)?;
        &rebound
    }else{ready};
    let videos=super::concat_project_ready_video::prepare(timelines,videos,physical_bindings,bindings,&mut project_media,ready,preserve_media_ids)?;
    let mut used_videos=BTreeSet::new();
    let mut used_cutouts=BTreeSet::new();
    let mut compositions = BTreeMap::new();
    let mut mapping = BTreeMap::new();
    let mut order = vec![];
    for (index, timeline) in timelines.iter().enumerate() {
        let path = format!("document.timelines[{index}]");
        fields(timeline, &["id", "name", "video", "tracks", "clips"], &path)?;
        let old_id = text(timeline, "id")?;
        let id = format!("concat-timeline-{index}");
        ensure!(
            mapping.insert(old_id.to_owned(), id.clone()).is_none(),
            "{path}.id: duplicate"
        );
        order.push(id.clone());
        let video = timeline
            .get("video")
            .or_else(|| document.get("video"))
            .cloned()
            .unwrap_or(json!({}));
        fields(
            &video,
            &["width", "height", "rateNum", "rateDen", "colorSpace"],
            &format!("{path}.video"),
        )?;
        ensure!(
            video.get("colorSpace").map_or(true, |v| v == "sdr"),
            "{path}.video.colorSpace: HDR conversion is not implemented"
        );
        let shared = document.get("video").cloned().unwrap_or(json!({}));
        let width = number(&video, "width", number(&shared, "width", 1920.)?)?;
        let height = number(&video, "height", number(&shared, "height", 1080.)?)?;
        let num = number(&video, "rateNum", number(&shared, "rateNum", 30.)?)?;
        let den = number(&video, "rateDen", number(&shared, "rateDen", 1.)?)?;
        ensure!(
            [width, height, num, den]
                .iter()
                .all(|v| *v > 0. && v.fract() == 0. && *v <= u32::MAX as f64),
            "{path}.video: invalid integer frame/rate"
        );
        let tracks = entries(timeline, "tracks")?;
        let clips = entries(timeline, "clips")?;
        ensure!(
            tracks.len() <= 16 && clips.len() <= 512,
            "{path}: native track/clip limit exceeded"
        );
        let mut track_ids = BTreeSet::new();
        let mut clip_ids = BTreeSet::new();
        let mut native_tracks = vec![];
        let mut names = serde_json::Map::new();
        let mut lane_groups = serde_json::Map::new();
        for (ti, track) in tracks.iter().enumerate() {
            let track_path = format!("{path}.tracks[{ti}]");
            fields(track, &["id", "name", "visible", "muted"], &track_path)?;
            let track_id = text(track, "id")?;
            ensure!(
                track_ids.insert(track_id.to_owned()),
                "{track_path}: duplicate id"
            );
            let mut visual_spans = clips
                .iter()
                .filter(|clip| {
                    clip["trackId"] == track_id
                        && matches!(clip["kind"].as_str(), Some("video" | "image" | "text" | "shape"))
                })
                .map(|clip| {
                    let start = number(clip, "start", 0.)?;
                    Ok((start, start + number(clip, "duration", 1.)?))
                })
                .collect::<Result<Vec<_>>>()?;
            visual_spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            // Source JSON roundtrips can differ by a few floating-point ulps
            // at a touching cut; do not mistake that for an authored overlap.
            ensure!(visual_spans.windows(2).all(|pair|{let end=pair[0].1;let start=pair[1].0;end<=start||(end-start)<=4.*f64::EPSILON*end.abs().max(start.abs()).max(1.)}),"{track_path}: overlapping visual clips on one untyped lane require explicit layer conversion");
            for kind in ["layer", "video", "image", "audio", "text", "shape"] {
                let mut native_clips = vec![];
                for (ci, clip) in clips.iter().enumerate() {
                    if text(clip, "trackId")? != track_id || text(clip, "kind")? != kind {
                        continue;
                    }
                    let clip_path = format!("{path}.clips[{ci}]");
                    let clip_id=text(clip,"id")?;
                    let prepared=ready.get(old_id).and_then(|clips|clips.get(clip_id));
                    let prepared_video=videos.get(old_id).and_then(|clips|clips.get(clip_id));
                    let mut view=clip.clone();
                    if kind=="video"{view.as_object_mut().context("invalid video clip")?.remove("transitionIn");}
                    if prepared.is_some(){
                        ensure!(kind=="image"&&clip.get("cutout").is_some_and(|v|!v.is_null()),"ready still cutout does not match an image cutout clip");
                        super::concat_cutout_import::plan(&clip["cutout"])?;
                        view.as_object_mut().context("invalid clip")?.remove("cutout");
                        used_cutouts.insert((old_id.to_owned(),clip_id.to_owned()));
                    }
                    if prepared_video.is_some(){
                        ensure!(kind=="video"&&clip.get("cutout").is_some_and(|v|!v.is_null()),"ready video cutout does not match a video cutout clip");
                        view.as_object_mut().context("invalid video clip")?.remove("cutout");
                        used_videos.insert((old_id.to_owned(),clip_id.to_owned()));
                    }
                    let mut native = convert_clip(&view, bindings, width, height)
                        .with_context(|| clip_path.clone())?;
                    if native.get("detachedFrom").is_some_and(Value::is_string){native["concatDetachedOrder"]=json!(ci);}
                    let clip_id = text(clip, "id")?;
                    ensure!(
                        clip_ids.insert(clip_id.to_owned()),
                        "{clip_path}.id: duplicate"
                    );
                    names.insert(
                        clip_id.into(),
                        json!(clip.get("name").and_then(Value::as_str).unwrap_or(clip_id)),
                    );
                    native_clips.push(native);
                }
                if native_clips.is_empty() {
                    continue;
                }
                if kind!="layer"{native_clips.sort_by_key(|clip| {
                    clip.get("placement")
                        .and_then(|p| p.get("timelineStartTick"))
                        .or_else(|| clip.get("timelineStartTick"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                });}
                let transitions=if kind=="video"{super::concat_project_transitions::import(clips,track,bindings,num as u64,den as u64)?}else{vec![]};
                for transition in &transitions{
                    if !matches!(transition["concatOverlap"]["legacy"].as_str(),Some("push"|"zoom")){continue;}
                    for id in [&transition["fromClipId"],&transition["toClipId"]]{
                        let native=native_clips.iter_mut().find(|clip|&clip["id"]==id).context("Legacy movement endpoint missing")?;
                        if native["transform"]["concatAnimation"].is_null(){
                            let authored=clips.iter().find(|clip|&clip["id"]==id).context("Legacy authored pose missing")?;
                            native["transform"]["concatAnimation"]=serde_json::to_value(retained_visual_animation(authored,width,height,ticks(number(authored,"duration",1.)?)?,BTreeMap::new())?)?;
                        }
                    }
                }
                native_tracks.push(json!({"id":format!("lane-{ti}-{kind}"),"kind":if kind=="shape"||kind=="layer"{"graphic"}else{kind},"name":track.get("name").and_then(Value::as_str).unwrap_or(track_id),"hidden":!track.get("visible").and_then(Value::as_bool).unwrap_or(true),"muted":track.get("muted").and_then(Value::as_bool).unwrap_or(false),"locked":false,"clips":native_clips,"transitions":transitions}));
            }
            let kinds: BTreeSet<_> = clips.iter().filter(|clip|clip["trackId"]==track_id).filter_map(|clip|clip["kind"].as_str()).collect();
            if kinds==BTreeSet::from(["video","image"]) {
                lane_groups.insert(format!("lane-{ti}-video"),json!([format!("lane-{ti}-video"),format!("lane-{ti}-image")]));
            }
            if !native_tracks.iter().any(|track| {
                track["id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with(&format!("lane-{ti}-")))
            }) {
                native_tracks.push(json!({"id":format!("lane-{ti}-video"),"kind":"video","name":track.get("name").and_then(Value::as_str).unwrap_or(track_id),"hidden":!track.get("visible").and_then(Value::as_bool).unwrap_or(true),"muted":track.get("muted").and_then(Value::as_bool).unwrap_or(false),"locked":false,"clips":[],"transitions":[]}));
            }
        }
        ensure!(
            clip_ids.len() == clips.len(),
            "{path}.clips: unsupported clip kind or missing lane"
        );
        // Concat stores bottom-most tracks first; native track zero is top-most.
        native_tracks.reverse();
        // Strict native track schemas differ by media kind.
        for track in &mut native_tracks {
            if track["kind"] != "video" {
                track.as_object_mut().unwrap().remove("transitions");
            }
            if track["kind"] == "image" || track["kind"] == "text" || track["kind"] == "graphic" {
                track.as_object_mut().unwrap().remove("muted");
            }
            if track["kind"] == "audio" {
                track.as_object_mut().unwrap().remove("hidden");
            }
        }
        let mut content:Composition=serde_json::from_value(json!({"schemaVersion":1,"timeBase":1000000,"canvas":{"width":width as u32,"height":height as u32,"fpsMilli":(num/den*1000.).round() as u32,"fpsRate":{"numerator":num as u32,"denominator":den as u32},"background":{"red":0,"green":0,"blue":0,"alpha":1}},"sources":sources,"tracks":native_tracks,"metadata":{"sequenceName":timeline.get("name").and_then(Value::as_str).unwrap_or(old_id),"clipNames":names,"sourceNames":source_names,"concatLaneGroups":lane_groups}})).with_context(||format!("{path}: native document shape"))?;
        for clip in clips{
            let clip_id=text(clip,"id")?;
            if let Some(binding)=ready.get(old_id).and_then(|clips|clips.get(clip_id)){
                let plan=super::concat_cutout_import::plan(&clip["cutout"])?;
                ensure!(plan==binding.source_settings,"cutout subject/settings changed since analysis");
                content=super::concat_cutout_binding::bind_still(&content,&super::composition::CompositionClipId::parse(clip_id)?,&plan,&binding.assets,&binding.source_hash,&binding.model_hash,&binding.runtime_hash)?;
            }
        }
        for clip in clips{
            let clip_id=text(clip,"id")?;
            if let Some(binding)=videos.get(old_id).and_then(|entries|entries.get(clip_id)){
                super::concat_project_ready_video::bind(&mut content,&super::composition::CompositionClipId::parse(clip_id)?,binding)?;
            }
        }
        content
            .validate_native_surface()
            .with_context(|| format!("{path}: native qualification"))?;
        compositions.insert(
            id,
            NestedCompositionDocument {
                duration_us: None,
                content,
                instances: vec![],
            },
        );
    }
    ensure!(ready.iter().all(|(timeline,clips)|mapping.contains_key(timeline)&&clips.keys().all(|clip|used_cutouts.contains(&(timeline.clone(),clip.clone())))),"unused or stale cutout binding");
    ensure!(videos.iter().all(|(timeline,clips)|mapping.contains_key(timeline)&&clips.keys().all(|clip|used_videos.contains(&(timeline.clone(),clip.clone())))),"unused or stale video cutout binding");
    let root = document
        .get("activeTimelineId")
        .and_then(Value::as_str)
        .and_then(|id| mapping.get(id))
        .cloned()
        .unwrap_or_else(|| order[0].clone());
    let project = NestedCompositionProject {
        project_media,
        fonts,
        schema_version: 1,
        root,
        timeline_order: order,
        luts: vec![],
        compositions,
    };
    project.dependency_order()?;
    Ok((project, mapping))
}
pub(crate) fn convert_clip(
    clip: &Value,
    bindings: &BTreeMap<String, CompositionSource>,
    width: f64,
    height: f64,
) -> Result<Value> {
    if clip.get("kind").and_then(Value::as_str)==Some("text"){return convert_title(clip,width,height);}
    if clip.get("kind").and_then(Value::as_str)==Some("shape"){return convert_shape(clip,width,height);}
    if clip.get("kind").and_then(Value::as_str)==Some("layer"){return convert_treatment(clip,width,height);}
    // Original audio_pieces/playback consume audio controls only. Detachment
    // clones visual clip metadata, which must not change admission of its sound.
    let audio_view=if clip.get("kind").and_then(Value::as_str)==Some("audio") {
        let mut view=clip.clone();
        if let Some(object)=view.as_object_mut(){
            for field in ["scale","stretchX","stretchY","offsetX","offsetY","rotation","opacity","blend","flipH","flipV","crop","cutout","videoEffects"]{object.remove(field);}
            if let Some(keys)=object.get_mut("keys").and_then(Value::as_array_mut){keys.retain(|key|!matches!(key.get("property").and_then(Value::as_str),Some("scale"|"offsetX"|"offsetY"|"rotation"|"opacity")));}
        }
        Some(view)
    }else{None};
    let clip=audio_view.as_ref().unwrap_or(clip);
    fields(
        clip,
        &[
            "id",
            "trackId",
            "mediaId",
            "name",
            "kind",
            "start",
            "duration",
            "sourceStart",
            "volume",
            "fadeIn",
            "fadeOut",
            "scale",
            "offsetX",
            "offsetY",
            "rotation",
            "stretchX",
            "stretchY",
            "opacity",
            "speed",
            "speedCurve",
            "keys",
            "flipH",
            "flipV",
            "blend",
            "crop",
            "cutout",
            "preservePitch",
            "filters",
            "videoEffects",
            "audioStream",
            "muted",
            "detachedFrom",
            "transitionIn",
            "enhanced",
            "text",
            "shape",
        ],
        "clip",
    )?;
    inactive(
        clip,
        &[
            "transitionIn",
            "enhanced",
            "text",
            "shape",
        ],
        "clip",
    )?;
    if let Some(cutout)=clip.get("cutout").filter(|value|!value.is_null()){
        let plan=super::concat_cutout_import::plan(cutout)?;
        bail!("Concat cutout requires source-owned model/mask analysis before import: subject={:?}, custom={}, strokes={}, smart source frames={}. Original cutout retained in sourceDocument",plan.subject,plan.correction.custom,plan.correction.strokes.len(),plan.region_source_seconds.len());
    }
    let kind = text(clip, "kind")?;
    if kind != "image" { inactive(clip, &["speedCurve"], "clip")?; }
    let audio_stream=match clip.get("audioStream") {None|Some(Value::Null)=>None,Some(value)=>Some(value.as_u64().filter(|index|*index<=u32::MAX as u64).context("audioStream: expected absolute unsigned stream index")? as u32)};
    ensure!(audio_stream.is_none()||kind=="audio"||kind=="video","audio stream selection requires audio/video");

    ensure!(kind=="audio"||clip.get("detachedFrom").map_or(true,Value::is_null),"detachedFrom is only supported on audio clips");
    let source = bindings
        .get(text(clip, "mediaId")?)
        .context("missing media binding")?;
    let source_kind = serde_json::to_value(source.kind)?;
    ensure!(
        source_kind == kind || (kind == "audio" && source.has_audio),
        "media binding kind mismatch"
    );
    let start = ticks(number(clip, "start", 0.)?)?;
    let duration = ticks(number(clip, "duration", 1.)?)?;
    ensure!(duration > 0, "empty clip");
    let source_start = ticks(number(clip, "sourceStart", 0.)?)?;
    let speed = number(clip, "speed", 1.)?;
    ensure!(speed >= 0.0625 && speed <= 16., "speed out of range");
    let source_out = source_start
        .checked_add(ticks(duration as f64 / 1_000_000. * speed)?)
        .context("source time overflow")?;
    let scale = number(clip, "scale", 1.)?;
    ensure!(
        (0.05..=8.).contains(&scale)
            && (0.1..=10.).contains(&number(clip, "stretchX", 1.)?)
            && (0.1..=10.).contains(&number(clip, "stretchY", 1.)?)
            && number(clip, "offsetX", 0.)?.abs() <= 3.
            && number(clip, "offsetY", 0.)?.abs() <= 3.,
        "source transform outside Concat authored range"
    );
    let sx = scale * number(clip, "stretchX", 1.)?;
    let sy = scale * number(clip, "stretchY", 1.)?;
    let mut transform = json!({"x":value(number(clip,"offsetX",0.)?*width),"y":value(number(clip,"offsetY",0.)?*height),"scaleX":value(sx),"scaleY":value(sy),"rotationDegrees":value(number(clip,"rotation",0.)?),"anchorX":0.5,"anchorY":0.5});
    if kind=="video"||kind=="image"{
        let mut edges=[0f64;4];
        if let Some(crop)=clip.get("crop").filter(|v|!v.is_null()){
            fields(crop,&["left","top","right","bottom"],"clip.crop")?;
            for(index,name)in ["left","top","right","bottom"].into_iter().enumerate(){edges[index]=number(crop,name,0.)?.clamp(0.,0.9);}
            if edges[0]+edges[2]>0.9{edges[2]=(0.9-edges[0]).max(0.);}
            if edges[1]+edges[3]>0.9{edges[3]=(0.9-edges[1]).max(0.);}
        }
        let preparation=super::concat_picture::Preparation{crop:edges,canvas:[width as usize,height as usize],flip_h:clip.get("flipH").and_then(Value::as_bool)==Some(true),flip_v:clip.get("flipV").and_then(Value::as_bool)==Some(true)};
        preparation.validate()?;transform["concatPreparation"]=serde_json::to_value(preparation)?;
    }
    let mut effects = vec![];
    for entry in entries(clip, "videoEffects")? {
        let link: AppliedFilter = serde_json::from_value(entry.clone())?;
        let link = upgrade(link)?.link;
        let effect=if link.enabled{native_effect(&link)?.with_context(||format!("unsupported visual effect {}",link.id))?}else{
            super::concat_effect_upgrade::validate_bypassed(&link)?;
            super::composition::VideoEffect::ConcatBypass{link}
        };
        effects.push(serde_json::to_value(effect)?);
    }
    let blend = clip
        .get("blend")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .unwrap_or("normal");
    let blend=if blend=="add"{"addition"}else{blend};
    ensure!(matches!(blend,"normal"|"multiply"|"screen"|"addition"|"lighten"|"darken"),"unsupported original Concat blend {blend}");
    let opacity = number(clip, "opacity", 1.)?;
    let gain = number(clip, "volume", 1.)?;
    let mut gain_keys=Vec::new();
    let mut visual_keys:BTreeMap<String,Vec<super::concat_effect_upgrade::ParamKey>>=BTreeMap::new();
    for key in entries(clip,"keys")? {
        fields(key,&["property","at","value","ease"],"clip.keys")?;
        let property=text(key,"property")?.to_owned();
        ensure!(matches!(property.as_str(),"volume"|"scale"|"offsetX"|"offsetY"|"rotation"|"opacity"),"unknown clip-property key {property}");
        let mut key=key.clone();key.as_object_mut().unwrap().remove("property");
        let mut key:super::concat_effect_upgrade::ParamKey=serde_json::from_value(key)?;
        if property=="volume"{key.value=key.value.max(0.);gain_keys.push(key);}else{visual_keys.entry(property).or_default().push(key);}
    }
    gain_keys.sort_by(|a,b|a.at.total_cmp(&b.at));
    ensure!(kind=="audio"||kind=="video"||gain_keys.is_empty(),"volume keys on this visual kind are not converted");

    if !visual_keys.is_empty(){
        ensure!(kind=="video"||kind=="image","visual keys require video/image");
        let animation=retained_visual_animation(clip,width,height,duration,visual_keys)?;
        transform["concatAnimation"]=serde_json::to_value(animation)?;
    }

    let mut placement = json!({"timelineStartTick":start,"sourceInTick":source_start,"sourceOutTick":source_out,"speed":speed});
    if matches!(kind,"video"|"audio") && source_out > source.duration_ticks {
        placement["concatSourceEof"] = json!(true);
    }
    // Preserve the authored timeline when rounded source microseconds cannot
    // reconstruct its duration at the original constant speed.
    if ((source_out-source_start) as f64/speed).round() as u64 != duration {
        placement["timelineDurationTick"]=json!(duration);
    }
    let mut out = json!({"id":text(clip,"id")?,"sourceId":source.id,"enabled":true});
    if let Some(index)=audio_stream{out["audioStream"]=json!(index);}
    let links = entries(clip, "filters")?
        .iter()
        .map(|entry| {
            serde_json::from_value::<AppliedFilter>(entry.clone()).map_err(anyhow::Error::from)
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        kind == "audio" || kind == "video" || links.is_empty(),
        "audio filters on this visual kind are not converted"
    );
    if kind == "audio" {
        ensure!(
            clip.get("preservePitch")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            "tape-speed pitch mode is not converted"
        );
        ensure!(
            effects.is_empty() && opacity == 1. && blend == "normal",
            "visual controls on audio are not converted"
        );
        out["placement"] = placement;
        out["gain"] = value(gain);
        if let Some(origin)=clip.get("detachedFrom").filter(|value|!value.is_null()){ensure!(origin.is_string(),"detachedFrom: expected video clip id");out["detachedFrom"]=origin.clone();}
        out["concatGainKeys"]=serde_json::to_value(&gain_keys)?;
        out["enabled"] = json!(clip.get("muted").and_then(Value::as_bool) != Some(true));
        out["dspEffects"] = serde_json::to_value(super::concat_audio_import::convert(&links)?)?;
        out["pan"] = value(0.);
        out["fadeInTicks"] = json!(ticks(number(clip, "fadeIn", 0.)?)?);
        out["fadeOutTicks"] = json!(ticks(number(clip, "fadeOut", 0.)?)?);
    } else {
        ensure!(
            kind == "video" || (number(clip, "fadeIn", 0.)? == 0. && number(clip, "fadeOut", 0.)? == 0.),
            "embedded-audio/visual fades are not converted"
        );
        out["transform"] = transform;
        out["opacity"] = value(opacity);
        out["blendMode"] = json!(blend);
        out["effects"] = json!(effects);
        if kind == "video" {
            ensure!(
                clip.get("preservePitch")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                "tape-speed pitch mode is not converted"
            );
            out["dspEffects"]=serde_json::to_value(super::concat_audio_import::convert(&links)?)?;
            out["independentAudio"]=json!(true);
            out["audioFadeInTicks"]=json!(ticks(number(clip,"fadeIn",0.)?)?);
            out["audioFadeOutTicks"]=json!(ticks(number(clip,"fadeOut",0.)?)?);
            out["placement"] = placement;
            out["sourceAudioEnabled"] =
                json!(source.has_audio && clip.get("muted").and_then(Value::as_bool) != Some(true));
            out["audioGain"] = value(gain);
            out["concatGainKeys"]=serde_json::to_value(&gain_keys)?;
        } else {
            let raw_start = number(clip, "sourceStart", 0.)?;
            let curve = match clip.get("speedCurve") {
                None|Some(Value::Null)=>None,
                Some(Value::Array(points)) if points.is_empty()=>None,
                Some(value)=>Some(serde_json::from_value::<Vec<super::concat_retime_edit::Point>>(value.clone())?),
            };
            if speed != 1. || raw_start != 0. || curve.is_some() {
                let timing = super::composition::ImageSourceTiming{source_start:raw_start,speed,speed_curve:curve};
                timing.validate()?;
                out["concatSourceTiming"] = serde_json::to_value(timing)?;
            }
            out["timelineStartTick"] = json!(start);
            out["durationTicks"] = json!(duration);
        }
    }
    Ok(out)
}


fn convert_title(clip:&Value,width:f64,height:f64)->Result<Value>{
 let mut title=clip.get("text").filter(|v|v.is_object()).context("text clip needs its retained style")?.clone();
 let content=text(&title,"content")?.to_owned();
 title.as_object_mut().unwrap().remove("content");
 let style:super::concat_title::TitleStyle=serde_json::from_value(title)?;style.validate()?;
 let mut view=clip.clone();let fields=view.as_object_mut().context("invalid text clip")?;
 fields.remove("text");fields.insert("kind".into(),json!("image"));fields.insert("mediaId".into(),json!("concat-title-source"));
 let source:CompositionSource=serde_json::from_value(json!({"id":"concat-title-source","kind":"image","durationTicks":0,"width":width as u32,"height":height as u32,"hasAudio":false}))?;
 let converted=convert_clip(&view,&BTreeMap::from([("concat-title-source".into(),source)]),width,height)?;
 let transform=converted["transform"].clone();
 let start=converted["timelineStartTick"].as_u64().context("title start missing")?;
 let duration=converted["durationTicks"].as_u64().context("title duration missing")?;
 Ok(json!({"id":text(clip,"id")?,"timelineStartTick":start,"timelineEndTick":start.checked_add(duration).context("title end overflow")?,"text":content,"concatTitle":style,"blendMode":converted["blendMode"],"effects":converted["effects"],"style":{"fontFamily":"Noto Sans","fontSize":24,"color":{"red":1,"green":1,"blue":1,"alpha":1},"background":{"red":0,"green":0,"blue":0,"alpha":0},"stroke":{"red":0,"green":0,"blue":0,"alpha":0},"strokeWidth":0,"shadow":{"red":0,"green":0,"blue":0,"alpha":0},"shadowX":0,"shadowY":0},"transform":transform,"opacity":converted["opacity"],"enabled":true}))
}

fn convert_shape(clip:&Value,width:f64,height:f64)->Result<Value>{
 let shape:super::concat_shape::ShapeStyle=serde_json::from_value(clip.get("shape").filter(|v|v.is_object()).context("shape clip needs its retained style")?.clone())?;shape.validate()?;
 let mut view=clip.clone();let fields=view.as_object_mut().context("invalid shape clip")?;
 fields.remove("shape");fields.insert("kind".into(),json!("image"));fields.insert("mediaId".into(),json!("concat-shape-source"));
 let source:CompositionSource=serde_json::from_value(json!({"id":"concat-shape-source","kind":"image","durationTicks":0,"width":width as u32,"height":height as u32,"hasAudio":false}))?;
 let converted=convert_clip(&view,&BTreeMap::from([("concat-shape-source".into(),source)]),width,height)?;
 let start=converted["timelineStartTick"].as_u64().context("shape start missing")?;let duration=converted["durationTicks"].as_u64().context("shape duration missing")?;
 Ok(json!({"id":text(clip,"id")?,"timelineStartTick":start,"timelineEndTick":start.checked_add(duration).context("shape end overflow")?,"width":width as u32,"height":height as u32,"graphic":{"layers":[]},"concatShape":shape,"concatShapeBlend":converted["blendMode"],"effects":converted["effects"],"transform":converted["transform"],"opacity":converted["opacity"],"enabled":true}))
}

fn retained_visual_animation(clip:&Value,width:f64,height:f64,duration:u64,keys:BTreeMap<String,Vec<super::concat_effect_upgrade::ParamKey>>)->Result<super::concat_clip_animation::SourceAnimation>{
    let scale=number(clip,"scale",1.)?;
    let animation=super::concat_clip_animation::SourceAnimation{duration_ticks:i64::try_from(duration)?,keys,
        rest:super::concat_clip_animation::Pose{scale_x:scale,scale_y:scale,x:number(clip,"offsetX",0.)?,y:number(clip,"offsetY",0.)?,rotation_degrees:number(clip,"rotation",0.)?,opacity:number(clip,"opacity",1.)?},
        stretch:[number(clip,"stretchX",1.)?,number(clip,"stretchY",1.)?],canvas:[width,height]};
    animation.validate()?;Ok(animation)
}

fn convert_treatment(clip:&Value,width:f64,height:f64)->Result<Value>{
 let start=ticks(number(clip,"start",0.)?)?;let duration=ticks(number(clip,"duration",1.)?)?;
 let treatment=super::concat_treatment::Treatment{strength:number(clip,"opacity",1.)?,fade_in:number(clip,"fadeIn",0.)?,fade_out:number(clip,"fadeOut",0.)?,duration_ticks:duration};treatment.validate()?;
 let mut view=clip.clone();let fields=view.as_object_mut().context("invalid treatment clip")?;
 // The source layer flattening path consumes only placement, opacity, ramps and videoEffects.
 for key in ["volume","fadeIn","fadeOut","scale","offsetX","offsetY","rotation","stretchX","stretchY","sourceStart","speed","speedCurve","keys","flipH","flipV","blend","crop","cutout","preservePitch","filters","audioStream","muted","detachedFrom","transitionIn","enhanced","text","shape"]{fields.remove(key);}
 fields.insert("kind".into(),json!("image"));fields.insert("mediaId".into(),json!("concat-treatment-source"));fields.insert("opacity".into(),json!(1));
 let source:CompositionSource=serde_json::from_value(json!({"id":"concat-treatment-source","kind":"image","durationTicks":0,"width":width as u32,"height":height as u32,"hasAudio":false}))?;
 let converted=convert_clip(&view,&BTreeMap::from([("concat-treatment-source".into(),source)]),width,height)?;
 Ok(json!({"id":text(clip,"id")?,"timelineStartTick":start,"timelineEndTick":start.checked_add(duration).context("treatment end overflow")?,"width":width as u32,"height":height as u32,"graphic":{"layers":[]},"concatTreatment":treatment,"effects":converted["effects"],"transform":super::composition::TransformSpec::default(),"opacity":{"mode":"constant","value":1},"enabled":true}))
}
