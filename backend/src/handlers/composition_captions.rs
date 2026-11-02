//! Stateless caption transactions: persistence and undo remain owned by the composition editor.
use crate::{domain::composition::{Composition, CompositionClipId}, error::{ApiJson, AppError, AppResult}};
use axum::Json;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct UpgradeRequest {
    pub composition: Composition,
    pub caption_ids: Vec<CompositionClipId>,
}
pub async fn upgrade(ApiJson(request): ApiJson<UpgradeRequest>) -> AppResult<Json<Composition>> {
    request.composition.upgrade_captions(&request.caption_ids)
        .map(Json).map_err(|error| AppError::bad_request(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::post, body::Body, http::{Request, StatusCode}};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use serde_json::{json,Value};
    async fn send(value:Value)->(StatusCode,Value) {
        let response=Router::new().route("/upgrade",post(upgrade)).oneshot(Request::builder().method("POST").uri("/upgrade").header("content-type","application/json").body(Body::from(value.to_string())).unwrap()).await.unwrap();
        let status=response.status();let bytes=response.into_body().collect().await.unwrap().to_bytes();
        (status,serde_json::from_slice(&bytes).unwrap())
    }
    #[tokio::test]
    async fn caption_upgrade_http_preserves_timing_and_rejects_locked_selection() {
        let canvas=serde_json::from_value(json!({"width":320,"height":180,"fpsMilli":24000,"background":{"red":0,"green":0,"blue":0,"alpha":1}})).unwrap();
        let mut document=serde_json::to_value(Composition::new(canvas)).unwrap();
        document["captionTracks"]=json!([{"id":"captions","name":"Captions","clips":[{"id":"cue","timelineStartTick":123,"timelineEndTick":2000123,"text":"Hello <i>world</i>"}]}]);
        let (status,result)=send(json!({"composition":document,"captionIds":["cue"]})).await;
        assert_eq!(status,StatusCode::OK);assert_eq!(result["captionTracks"][0]["clips"],json!([]));
        assert_eq!(result["tracks"][0]["clips"][0]["timelineStartTick"],123);
        assert_eq!(result["tracks"][0]["clips"][0]["timelineEndTick"],2000123);
        assert_eq!(document["captionTracks"][0]["clips"].as_array().unwrap().len(),1);
        document["captionTracks"][0]["locked"]=json!(true);
        assert_eq!(send(json!({"composition":document,"captionIds":["cue"]})).await.0,StatusCode::BAD_REQUEST);
        assert_eq!(send(json!({"composition":document,"captionIds":[]})).await.0,StatusCode::BAD_REQUEST);
    }
}
