use axum::{body::{to_bytes, Body}, extract::ConnectInfo, http::Request};
use serde_json::{json, Value};
use tower::ServiceExt;

#[tokio::test]
async fn caption_export_is_advertised_with_source_format_and_lane_arguments() {
    let token = "caption-export-test-token-0123456789";
    let app = video_kadr_backend::mcp::router(Some(token.into()), vec![]);
    let mut request = Request::builder().method("POST").uri("/mcp")
        .header("authorization", format!("Bearer {token}"))
        .header("host", "localhost:8080").header("content-type", "application/json")
        .body(Body::from(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string())).unwrap();
    request.extensions_mut().insert(ConnectInfo("127.0.0.1:43210".parse::<std::net::SocketAddr>().unwrap()));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200);
    let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap()).unwrap();
    let begin=body["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"]=="begin_agent_session").unwrap();
    assert_eq!(begin["inputSchema"]["required"],json!(["reason"]));
    assert!(body["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"]=="end_agent_session"));
    let list = body["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "list_checkpoints").unwrap();
    assert_eq!(list["inputSchema"]["additionalProperties"],false);
    let restore = body["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "restore_checkpoint").unwrap();
    assert_eq!(restore["inputSchema"]["properties"]["expectedProjectHash"]["type"],"string");
    let restyle = body["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "restyle_captions").unwrap();
    assert_eq!(restyle["inputSchema"]["required"], json!([]));
    assert_eq!(restyle["inputSchema"]["properties"]["layer_ids"]["minItems"], 1);
    assert_eq!(restyle["inputSchema"]["properties"]["outline_width"]["minimum"], 0);
    let merge = body["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "merge_captions").unwrap();
    assert_eq!(merge["inputSchema"]["required"], json!(["layer_ids"]));
    assert_eq!(merge["inputSchema"]["properties"]["layer_ids"]["minItems"], 2);
    assert_eq!(merge["inputSchema"]["oneOf"],json!([{"required":["expectedRevision"]},{"required":["expectedProjectHash"]}]));
    assert_eq!(restyle["inputSchema"]["oneOf"],merge["inputSchema"]["oneOf"]);
    let tool = body["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "export_captions").unwrap();
    assert_eq!(tool["inputSchema"]["required"], json!(["format"]));
    assert_eq!(tool["inputSchema"]["properties"]["format"]["enum"], json!(["srt", "vtt"]));
    assert_eq!(tool["inputSchema"]["properties"]["track_id"]["type"], json!(["string", "null"]));
    assert_eq!(tool["inputSchema"]["properties"]["composition_id"]["type"], json!(["string", "null"]));
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
}
