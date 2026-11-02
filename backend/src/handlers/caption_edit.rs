//! Caption edit boundary preserves legacy microseconds and exact source tick strings.
use crate::{
    domain::{
        caption_edit::{edit_exact, CaptionEdit},
        caption_timeline::CaptionCue,
        caption_time::{CaptionTime, CaptionDelta},
        caption_command_time::CaptionCommandTime,
        composition::{Composition, CompositionClipId, TrackId},
    },
    error::{ApiJson, AppError, AppResult},
};
use axum::Json;
use serde::{Deserialize, Serialize};
fn default_caption_text() -> String {
    "New caption".into()
}
fn present_nullable<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Operation {
    Add {
        track_id: TrackId,
        #[serde(default)]
        start_tick: Option<CaptionTime>,
        #[serde(default)]
        start_input: Option<CaptionCommandTime>,
        #[serde(default)]
        end_tick: Option<CaptionTime>,
        #[serde(default)]
        end_input: Option<CaptionCommandTime>,
        #[serde(default = "default_caption_text")]
        text: String,
        #[serde(default)]
        speaker: Option<String>,
    },
    Split {
        id: CompositionClipId,
        #[serde(default)]
        time_tick: Option<CaptionTime>,
        #[serde(default)]
        time_input: Option<CaptionCommandTime>,
    },
    Merge {
        ids: Vec<CompositionClipId>,
    },
    Text {
        id: CompositionClipId,
        #[serde(default)]
        text: Option<String>,
        #[serde(default, deserialize_with = "present_nullable")]
        speaker: Option<Option<String>>,
    },
    Times {
        id: CompositionClipId,
        #[serde(default)]
        start_tick: Option<CaptionTime>,
        #[serde(default)]
        end_tick: Option<CaptionTime>,
        #[serde(default)]
        start_input: Option<CaptionCommandTime>,
        #[serde(default)]
        end_input: Option<CaptionCommandTime>,
    },
    Trim {
        id: CompositionClipId,
        in_edge: bool,
        delta_ticks: CaptionDelta,
    },
    Move {
        ids: Vec<CompositionClipId>,
        delta_ticks: CaptionDelta,
    },
    Delete {
        ids: Vec<CompositionClipId>,
        #[serde(default)]
        ripple: bool,
    },
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Request {
    pub composition: Composition,
    pub operation: Operation,
    #[serde(default="zero_caption_time")]
    pub playhead_tick: CaptionTime,
    #[serde(default)]
    pub drop_frame: bool,
}
fn zero_caption_time()->CaptionTime {CaptionTime::ZERO}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub composition: Composition,
    pub applied_delta: Option<crate::domain::caption_time::CaptionDelta>,
    pub deleted: usize,
}
fn apply(request: Request) -> anyhow::Result<Response> {
    let mut composition = request.composition;
    composition.validate()?;
    let uid = || CompositionClipId::parse(format!("caption-{}", uuid::Uuid::new_v4()));
    let ratio = composition.canvas.frame_rate();
    let rate = filmcraft_time::FrameRate { num:ratio.numerator as i64, den:ratio.denominator as i64 };
    let input=|legacy:Option<CaptionTime>,authored:Option<CaptionCommandTime>|->anyhow::Result<Option<CaptionTime>> {
        authored.map(|v|v.resolve(rate,request.drop_frame,request.playhead_tick).map_err(anyhow::Error::msg)).transpose().map(|value|value.or(legacy))
    };
    let command = match request.operation {
        Operation::Add {
            track_id,
            start_tick,
            end_tick,
            start_input,
            end_input,
            text,
            speaker,
        } => {
            let start_tick=input(start_tick,start_input)?.unwrap_or(request.playhead_tick);
            let end_tick=input(end_tick,end_input)?;
            let duration=match end_tick {
                Some(end)=>end.exact_ticks()-start_tick.exact_ticks(),
                None=>3*filmcraft_time::TICKS_PER_SECOND as i128,
            };
            let duration=filmcraft_time::Tick(i64::try_from(duration).map_err(|_|anyhow::anyhow!("caption duration overflow"))?);
            let duration=rate.snap_nearest(duration).max(rate.frame_duration());
            let start=start_tick.with_exact_ticks(rate.snap(start_tick.source_tick().map_err(anyhow::Error::msg)?).0.max(0) as i128).map_err(anyhow::Error::msg)?;
            CaptionEdit::Add {
                track:track_id,
                cue:CaptionCue{cue_id:None,id:uid()?,timeline_start_tick:start,
                    timeline_end_tick:start.checked_add_source_ticks(duration.0).map_err(anyhow::Error::msg)?,
                    text,speaker,settings:String::new()},
            }
        },
        Operation::Split { id, time_tick, time_input } => CaptionEdit::Split {
            id,
            time: input(time_tick,time_input)?.unwrap_or(request.playhead_tick),
            new_id: uid()?,
        },
        Operation::Merge { ids } => CaptionEdit::Merge { ids },
        Operation::Text { id, text, speaker } => CaptionEdit::Text { id, text, speaker },
        Operation::Times {
            id,
            start_tick,
            end_tick,
            start_input,
            end_input,
        } => {
            let start_tick=input(start_tick,start_input)?;
            let end_tick=input(end_tick,end_input)?;
            let current=composition.caption_tracks.iter().flat_map(|track|&track.clips)
                .find(|cue|cue.id==id).ok_or_else(||anyhow::anyhow!("caption not found"))?;
            CaptionEdit::Times {
                id,
                start:match start_tick {Some(t)=>t.snap_nearest_frame(rate).map_err(anyhow::Error::msg)?,None=>current.timeline_start_tick},
                end:match end_tick {Some(t)=>t.snap_nearest_frame(rate).map_err(anyhow::Error::msg)?,None=>current.timeline_end_tick},
            }
        },
        Operation::Trim {
            id,
            in_edge,
            delta_ticks,
        } => CaptionEdit::Trim {
            id,
            in_edge,
            delta: delta_ticks,
        },
        Operation::Move { ids, delta_ticks } => CaptionEdit::Move {
            ids,
            delta: delta_ticks,
        },
        Operation::Delete { ids, ripple } => CaptionEdit::Delete { ids, ripple },
    };
    let rate = composition.canvas.frame_rate();
    let min_duration = (filmcraft_time::TICKS_PER_SECOND as i128 * rate.denominator as i128)
        / rate.numerator as i128;
    let result = edit_exact(&composition.caption_tracks, command, min_duration)?;
    composition.caption_tracks = result.tracks;
    composition.validate()?;
    Ok(Response {
        composition,
        applied_delta: result.applied,
        deleted: result.deleted,
    })
}
pub async fn handle(ApiJson(request): ApiJson<Request>) -> AppResult<Json<Response>> {
    apply(request)
        .map(Json)
        .map_err(|error| AppError::bad_request(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request as HttpRequest, StatusCode},
        routing::post,
        Router,
    };
    use http_body_util::BodyExt;
    use serde_json::{json, Value};
    use tower::ServiceExt;
    async fn send(value: Value) -> (StatusCode, Value) {
        let response = Router::new()
            .route("/edit", post(handle))
            .oneshot(
                HttpRequest::builder()
                    .method("POST")
                    .uri("/edit")
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
    #[tokio::test]
    async fn caption_edit_http_split_merge_nullable_speaker_and_locked_refusal() {
        let canvas=serde_json::from_value(json!({"width":320,"height":180,"fpsMilli":25000,"background":{"red":0,"green":0,"blue":0,"alpha":1}})).unwrap();
        let mut document = serde_json::to_value(Composition::new(canvas)).unwrap();
        document["captionTracks"] = json!([{"id":"captions","name":"Captions","clips":[{"id":"cue","timelineStartTick":0,"timelineEndTick":2000000,"text":"Line","speaker":"Ann"}]}]);
        let (status, cleared) = send(
            json!({"composition":document,"operation":{"kind":"text","id":"cue","speaker":null}}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(cleared["composition"]["captionTracks"][0]["clips"][0]["speaker"].is_null());
        assert_eq!(document["captionTracks"][0]["clips"][0]["speaker"], "Ann");
        let(status,split)=send(json!({"composition":document,"operation":{"kind":"split","id":"cue","timeTick":1000000}})).await;
        assert_eq!(status, StatusCode::OK);
        let clips = split["composition"]["captionTracks"][0]["clips"]
            .as_array()
            .unwrap();
        assert_eq!(clips.len(), 2);
        let right = clips[1]["id"].clone();
        assert_eq!(clips[0]["timelineEndTick"], 1000000);
        assert_eq!(clips[1]["timelineStartTick"], 1000000);
        let(status,merged)=send(json!({"composition":split["composition"],"operation":{"kind":"merge","ids":["cue",right]}})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            merged["composition"]["captionTracks"][0]["clips"][0]["text"],
            "Line\nLine"
        );
        document["captionTracks"][0]["locked"] = json!(true);
        assert_eq!(
            send(json!({"composition":document,"operation":{"kind":"delete","ids":["cue"]}}))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    #[tokio::test]
    async fn caption_edit_http_accepts_exact_source_positions_and_signed_deltas() {
        let canvas=serde_json::from_value(json!({"width":320,"height":180,"fpsMilli":24000,"background":{"red":0,"green":0,"blue":0,"alpha":1}})).unwrap();
        let mut document=serde_json::to_value(Composition::new(canvas)).unwrap();
        document["captionTracks"]=json!([{"id":"captions","name":"Exact","clips":[{"id":"cue","timelineStartTick":"1","timelineEndTick":"254016000001","text":"Exact"}]}]);
        let (status,moved)=send(json!({"composition":document,"operation":{"kind":"move","ids":["cue"],"deltaTicks":"1"}})).await;
        assert_eq!(status,StatusCode::OK,"{moved}");
        assert_eq!(moved["appliedDelta"],"1");
        assert_eq!(moved["composition"]["captionTracks"][0]["clips"][0]["timelineStartTick"],"2");
        let (status,split)=send(json!({"composition":document,"operation":{"kind":"split","id":"cue","timeTick":"127008000001"}})).await;
        assert_eq!(status,StatusCode::OK,"{split}");
        assert_eq!(split["composition"]["captionTracks"][0]["clips"][0]["timelineEndTick"],"127008000001");
        let (status,trimmed)=send(json!({"composition":document,"operation":{"kind":"trim","id":"cue","inEdge":true,"deltaTicks":"1"}})).await;
        assert_eq!(status,StatusCode::OK,"{trimmed}");
        assert_eq!(trimmed["composition"]["captionTracks"][0]["clips"][0]["timelineStartTick"],"2");
        let (status,times)=send(json!({"composition":document,"operation":{"kind":"times","id":"cue","startTick":"3","endTick":"254016000003"}})).await;
        assert_eq!(status,StatusCode::OK,"{times}");
        assert_eq!(times["composition"]["captionTracks"][0]["clips"][0]["timelineStartTick"],"0");
        assert_eq!(document["captionTracks"][0]["clips"][0]["timelineStartTick"],"1");
    }

    #[test]
    fn caption_command_clock_matches_fifteen_original_engine_transactions() {
        let fixture:Value=serde_json::from_str(include_str!("../../tests/fixtures/filmcraft-caption-command-clock.json")).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let num=case["rate"]["num"].as_i64().unwrap();let den=case["rate"]["den"].as_i64().unwrap();
            let rate=filmcraft_time::FrameRate{num,den};
            let canvas=serde_json::from_value(json!({"width":320,"height":180,"fpsMilli":(num*1000+den/2)/den,"fpsRate":{"numerator":num,"denominator":den},"background":{"red":0,"green":0,"blue":0,"alpha":1}})).unwrap();
            let mut composition=serde_json::to_value(Composition::new(canvas)).unwrap();
            let source=&case["before"]["tracks"][0];
            let clips:Vec<_>=source["captions"].as_array().unwrap().iter().map(|c|json!({"id":format!("cue-{}",c["id"]),"timelineStartTick":c["start"].as_i64().unwrap().to_string(),"timelineEndTick":c["end"].as_i64().unwrap().to_string(),"text":c["text"]})).collect();
            composition["captionTracks"]=json!([{"id":"captions","name":"Captions","clips":clips}]);
            let params=&case["params"];let id=format!("cue-{}",source["captions"][0]["id"]);
            let operation=match case["name"].as_str().unwrap(){
                "add"=>{let start=params["time"].as_i64().unwrap();let duration=filmcraft_time::Tick::from_seconds_f64(params["durationSeconds"].as_f64().unwrap()).0;json!({"kind":"add","trackId":"captions","startTick":start.to_string(),"endTick":(start+duration).to_string(),"text":params["text"]})},
                "times"=>json!({"kind":"times","id":id,"startTick":params["startTime"].as_i64().unwrap().to_string(),"endTick":params["endTime"].as_i64().unwrap().to_string()}),
                "split"=>json!({"kind":"split","id":id,"timeTick":params["time"].as_i64().unwrap().to_string()}),
                "move"=>json!({"kind":"move","ids":[id],"deltaTicks":rate.tick_of(params["deltaFrames"].as_i64().unwrap()).0.to_string()}),
                _=>json!({"kind":"trim","id":id,"inEdge":true,"deltaTicks":rate.tick_of(params["deltaFrames"].as_i64().unwrap()).0.to_string()}),
            };
            let request=serde_json::from_value(json!({"composition":composition,"operation":operation})).unwrap();
            let result=apply(request).unwrap();
            let actual:Vec<_>=result.composition.caption_tracks[0].clips.iter().map(|c|(c.timeline_start_tick.exact_ticks(),c.timeline_end_tick.exact_ticks(),c.text.clone())).collect();
            let expected:Vec<_>=case["after"]["tracks"][0]["captions"].as_array().unwrap().iter().map(|c|(c["start"].as_i64().unwrap() as i128,c["end"].as_i64().unwrap() as i128,c["text"].as_str().unwrap().to_owned())).collect();
            assert_eq!(actual,expected,"{} {num}/{den}",case["name"]);
        }
    }

    #[test]
    fn caption_times_omitted_endpoint_retains_fractional_source_placement() {
        let canvas=serde_json::from_value(json!({"width":320,"height":180,"fpsMilli":24000,"background":{"red":0,"green":0,"blue":0,"alpha":1}})).unwrap();
        let mut composition=serde_json::to_value(Composition::new(canvas)).unwrap();
        composition["captionTracks"]=json!([{"id":"captions","name":"Exact","clips":[{"id":"cue","timelineStartTick":"1","timelineEndTick":"508032000001","text":"Exact"}]}]);
        let request=serde_json::from_value(json!({"composition":composition,"operation":{"kind":"times","id":"cue","startTick":750000}})).unwrap();
        let result=apply(request).unwrap();
        assert_eq!(result.composition.caption_tracks[0].clips[0].timeline_start_tick.exact_ticks(),190512000000);
        assert_eq!(result.composition.caption_tracks[0].clips[0].timeline_end_tick.exact_ticks(),508032000001);
    }

    #[tokio::test]
    async fn caption_command_http_frames_timecode_and_relative_drop_frame_preserve_exact_time() {
        let canvas=serde_json::from_value(json!({"width":320,"height":180,"fpsMilli":23976,"fpsRate":{"numerator":24000,"denominator":1001},"background":{"red":0,"green":0,"blue":0,"alpha":1}})).unwrap();
        let mut document=serde_json::to_value(Composition::new(canvas)).unwrap();
        document["captionTracks"]=json!([{"id":"captions","name":"Exact","clips":[]}]);
        let (status,added)=send(json!({"composition":document,"operation":{"kind":"add","trackId":"captions","startInput":{"unit":"frames","value":13},"endInput":{"unit":"frames","value":16}}})).await;
        assert_eq!(status,StatusCode::OK,"{added}");
        let cue=&added["composition"]["captionTracks"][0]["clips"][0];
        assert_eq!(cue["timelineStartTick"],"137729592000");
        let (status,split)=send(json!({"composition":added["composition"],"operation":{"kind":"split","id":cue["id"],"timeInput":{"unit":"timecode","value":"00:00:00:14"}}})).await;
        assert_eq!(status,StatusCode::OK,"{split}");
        assert_eq!(split["composition"]["captionTracks"][0]["clips"][0]["timelineEndTick"],"148324176000");
        let rate=filmcraft_time::FrameRate::FPS_29_97;
        let mut document=added["composition"].clone();
        document["canvas"]["fpsMilli"]=json!(29970);document["canvas"]["fpsRate"]=json!({"numerator":30000,"denominator":1001});
        for (timecode,playhead,expected) in [("00;01;00;02",0,1800),("+15",100,115)] {
            let(status,result)=send(json!({"composition":document,"dropFrame":true,"playheadTick":rate.tick_of(playhead).0.to_string(),"operation":{"kind":"times","id":cue["id"],"endInput":{"unit":"timecode","value":timecode}}})).await;
            assert_eq!(status,StatusCode::OK,"{result}");
            assert_eq!(result["composition"]["captionTracks"][0]["clips"][0]["timelineEndTick"],rate.tick_of(expected).0.to_string());
            assert_eq!(result["composition"]["captionTracks"][0]["clips"][0]["timelineStartTick"],cue["timelineStartTick"]);
        }
    }

}
