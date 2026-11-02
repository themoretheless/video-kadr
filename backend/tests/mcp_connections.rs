use axum::{body::{to_bytes,Body},extract::ConnectInfo,http::{Request,StatusCode},Router};
use serde_json::{json,Value};
use tower::ServiceExt;
const TOKEN:&str="connections-test-token-0123456789012345";
fn request(path:&str,body:Value)->Request<Body>{
 let mut request=Request::builder().method("POST").uri(path).header("authorization",format!("Bearer {TOKEN}")).header("host","localhost:8080").header("content-type","application/json").body(Body::from(body.to_string())).unwrap();
 request.extensions_mut().insert(ConnectInfo("127.0.0.1:43210".parse::<std::net::SocketAddr>().unwrap()));request
}
async fn body(response:axum::response::Response)->Value {serde_json::from_slice(&to_bytes(response.into_body(),2*1024*1024).await.unwrap()).unwrap()}
async fn initialize(app:&Router,name:&str)->String{
 let response=app.clone().oneshot(request("/mcp",json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":name,"version":"2.5"}}}))).await.unwrap();
 assert_eq!(response.status(),StatusCode::OK);response.headers()["mcp-session-id"].to_str().unwrap().to_owned()
}
#[tokio::test]
async fn editor_receives_bounded_session_metadata_and_heartbeat_removal_without_polling(){
 let app=video_kadr_backend::mcp::router(Some(TOKEN.into()),vec![]);
 let a=initialize(&app,&"я".repeat(200)).await;let b=initialize(&app,"Second client").await;
 let response=app.clone().oneshot(request("/api/agent/heartbeat",json!({"editorId":"editor","includeConnections":true}))).await.unwrap();
 assert_eq!(response.status(),StatusCode::CONFLICT);
 let snapshot=body(app.clone().oneshot(request("/api/agent/poll",json!({"editorId":"editor"}))).await.unwrap()).await;
 let clients=snapshot["connections"].as_array().unwrap();assert_eq!(clients.len(),2);
 let first=clients.iter().find(|client|client["id"]==a).unwrap();
 assert_eq!(first["client"].as_str().unwrap().chars().count(),128);assert_eq!(first["version"],"2.5");
 assert!(first["connectedAt"].as_u64().unwrap()>0);assert!(first["lastActivityAt"].as_u64().unwrap()>=first["connectedAt"].as_u64().unwrap());
 let response=app.clone().oneshot(request("/api/agent/heartbeat",json!({"editorId":"editor"}))).await.unwrap();assert_eq!(response.status(),StatusCode::NO_CONTENT);
 let mut delete=request("/mcp",Value::Null);*delete.method_mut()=axum::http::Method::DELETE;delete.headers_mut().insert("mcp-session-id",a.parse().unwrap());
 assert_eq!(app.clone().oneshot(delete).await.unwrap().status(),StatusCode::NO_CONTENT);
 let snapshot=body(app.clone().oneshot(request("/api/agent/heartbeat",json!({"editorId":"editor","includeConnections":true}))).await.unwrap()).await;
 assert_eq!(snapshot["connections"].as_array().unwrap().len(),1);assert_eq!(snapshot["connections"][0]["id"],b);assert_eq!(snapshot["releasedActors"],json!([a]));
 let mut unauthorized=request("/api/agent/heartbeat",json!({"editorId":"editor","includeConnections":true}));unauthorized.headers_mut().remove("authorization");
 assert_eq!(app.oneshot(unauthorized).await.unwrap().status(),StatusCode::UNAUTHORIZED);
}
