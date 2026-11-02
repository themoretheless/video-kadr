use axum::{body::{to_bytes,Body},extract::ConnectInfo,http::Request};
use serde_json::{json,Value};
use tower::ServiceExt;
#[tokio::test]
async fn native_batches_advertise_hash_scope_and_atomic_rehearsal_without_requiring_ordinary_revision(){
 let token="native-batch-catalog-token-0123456789";
 let app=video_kadr_backend::mcp::router(Some(token.into()),vec![]);
 let mut request=Request::builder().method("POST").uri("/mcp").header("authorization",format!("Bearer {token}")).header("host","localhost:8080").header("content-type","application/json").body(Body::from(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string())).unwrap();
 request.extensions_mut().insert(ConnectInfo("127.0.0.1:43210".parse::<std::net::SocketAddr>().unwrap()));
 let response=app.oneshot(request).await.unwrap();assert_eq!(response.status(),200);
 let body:Value=serde_json::from_slice(&to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap();
 for name in ["apply_batch","dry_run"]{
  let tool=body["result"]["tools"].as_array().unwrap().iter().find(|tool|tool["name"]==name).unwrap();
  assert_eq!(tool["inputSchema"]["required"],json!(["operations"]));
  assert_eq!(tool["inputSchema"]["oneOf"],json!([{"required":["expectedRevision"]},{"required":["expectedProjectHash"]}]));
  assert_eq!(tool["inputSchema"]["properties"]["expectedProjectHash"]["pattern"],"^[0-9a-f]{64}$");
  assert_eq!(tool["inputSchema"]["properties"]["composition_id"]["type"],"string");
  assert!(tool["description"].as_str().unwrap().contains("nativeBatch"));
  let operations=tool["inputSchema"]["properties"]["operations"]["items"]["oneOf"].as_array().unwrap();
  let color_delete=operations.iter().find(|op|op["properties"]["op"]["const"]=="delete_color_key").unwrap();assert_eq!(color_delete["properties"]["tick"]["type"],"number");
  let color_edit=operations.iter().find(|op|op["properties"]["op"]["const"]=="edit_color_key").unwrap();assert_eq!(color_edit["properties"]["tick"]["type"],"number");assert_eq!(color_edit["properties"]["newTick"]["type"],"number");
  let colors_moved=operations.iter().find(|op|op["properties"]["op"]["const"]=="move_color_keys").unwrap();assert_eq!(colors_moved["properties"]["edits"]["items"]["properties"]["tick"]["type"],"number");assert_eq!(colors_moved["properties"]["edits"]["items"]["properties"]["newTick"]["type"],"number");
  for name in ["set_color_preset","set_color_interpolation"]{let curve=operations.iter().find(|op|op["properties"]["op"]["const"]==name).unwrap();assert_eq!(curve["properties"]["tick"]["type"],"number");}
  let near=operations.iter().find(|op|op["properties"]["op"]["const"]=="insert_path_node_near").unwrap();assert_eq!(near["properties"]["point"]["required"],json!(["x","y"]));assert_eq!(near["properties"]["maxDistance"]["minimum"],0);
  let arrival=operations.iter().find(|op|op["properties"]["op"]["const"]=="arrive_at_path_node").unwrap();assert_eq!(arrival["required"],json!(["op","clipId","index","playheadTicks"]));assert_eq!(arrival["properties"]["index"]["minimum"],0);
  let path_edit=operations.iter().find(|op|op["properties"]["op"]["const"]=="edit_motion_path").unwrap();assert_eq!(path_edit["properties"]["edit"]["oneOf"].as_array().unwrap().len(),8);let append=path_edit["properties"]["edit"]["oneOf"].as_array().unwrap().iter().find(|edit|edit["properties"]["operation"]["const"]=="append").unwrap();assert_eq!(append["required"],json!(["operation","id","point"]));assert_eq!(append["properties"]["point"]["required"],json!(["x","y"]));
  let convert=operations.iter().find(|op|op["properties"]["op"]["const"]=="convert_motion_path").unwrap();assert_eq!(convert["properties"]["options"]["additionalProperties"],false);assert_eq!(convert["properties"]["options"]["required"].as_array().unwrap().len(),5);
  let scalar_extrap=operations.iter().find(|op|op["properties"]["op"]["const"]=="set_keyframe_extrapolation").unwrap();assert_eq!(scalar_extrap["properties"]["side"]["enum"],json!(["before","after"]));assert_eq!(scalar_extrap["properties"]["envelope"]["enum"],json!(["visual","audio","mask","path","effect"]));
  assert_eq!(scalar_extrap["properties"]["effectIndex"]["minimum"],0);assert_eq!(scalar_extrap["properties"]["effectKind"]["type"],"string");
  let placements=operations.iter().find(|op|op["properties"]["op"]["const"]=="move_layer_placements").unwrap();assert_eq!(placements["properties"]["placements"]["type"],"array");assert_eq!(placements["properties"]["placements"]["items"]["required"],json!(["clipId","trackId"]));
  let raise=operations.iter().find(|op|op["properties"]["op"]["const"]=="move_layers_to_new_track").unwrap();assert_eq!(raise["properties"]["anchor"]["required"],json!(["clipId","startTicks"]));
  let slip=operations.iter().find(|op|op["properties"]["op"]["const"]=="slip_layer").unwrap();assert_eq!(slip["properties"]["sourceDeltaTicks"]["type"],"integer");assert!(slip["properties"]["sourceDeltaTicks"].get("minimum").is_none());
  let moved=operations.iter().find(|op|op["properties"]["op"]["const"]=="move_keyframe").unwrap();
  assert!(moved["properties"]["tick"].get("minimum").is_none());assert!(moved["properties"]["newTick"].get("minimum").is_none());
  let edited=operations.iter().find(|op|op["properties"]["op"]["const"]=="edit_keyframe").unwrap();assert!(edited["properties"]["newTick"].get("minimum").is_none());
  assert!(tool["inputSchema"]["properties"]["operations"]["items"]["oneOf"].as_array().unwrap().iter().any(|op|op["properties"]["op"]["const"]=="update_track"));
 }
 let freeze=body["result"]["tools"].as_array().unwrap().iter().find(|tool|tool["name"]=="freeze_frame").unwrap();
 assert_eq!(freeze["inputSchema"]["required"],json!(["expectedProjectHash","clipId","time_us"]));
 assert_eq!(freeze["inputSchema"]["additionalProperties"],false);
 assert_eq!(freeze["inputSchema"]["properties"]["time_us"]["maximum"],9007199254740991u64);
 assert!(freeze["description"].as_str().unwrap().contains("server-persisted mutation"));
 let jump=body["result"]["tools"].as_array().unwrap().iter().find(|tool|tool["name"]=="jump_history").unwrap();assert_eq!(jump["inputSchema"]["oneOf"],json!([{"required":["expectedRevision"]},{"required":["expectedProjectHash"]}]));
 let color=body["result"]["tools"].as_array().unwrap().iter().find(|tool|tool["name"]=="get_color_track").unwrap();
 assert_eq!(color["inputSchema"]["properties"]["composition_id"]["type"],"string");
 let read=body["result"]["tools"].as_array().unwrap().iter().find(|tool|tool["name"]=="get_keyframes").unwrap();
 assert_eq!(read["inputSchema"]["properties"]["composition_id"]["type"],"string");
}
