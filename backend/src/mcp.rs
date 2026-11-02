//! Opt-in, loopback-only MCP bridge to the editor actually visible in a browser.
//! A browser claims commands before executing them; expiration never replays edits.
use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

const TOOL_NAMES: &[&str] = &[
    "command_list",
    "command_run",
    "command_batch",
    "get_project",
    "begin_agent_session",
    "end_agent_session",
    "export_captions",
    "merge_captions",
    "restyle_captions",
    "get_keyframes",
    "freeze_frame",
    "get_color_track",
    "analyze_clip",
    "measure_shots",
    "compare_frames",
    "auto_split_by_shot",
    "mark_shots",
    "apply_shot_cuts",
    "get_history",
    "jump_history",
    "apply_batch",
    "dry_run",
    "create_checkpoint",
    "list_checkpoints",
    "delete_checkpoint",
    "restore_checkpoint",
    "set_history_lock",
    "undo",
    "redo",
];
struct Pending {
    command: Value,
    expires: Instant,
    response: oneshot::Sender<Value>,
    claimed: bool,
}
#[derive(Default)]
struct Inbox {
    queue: VecDeque<String>,
    pending: HashMap<String, Pending>,
    editor: Option<(String, Instant)>,
    released_actors: Vec<String>,
}
struct ClientSession {
    seen: Instant,
    client: String,
    version: String,
    connected_at: u64,
    last_activity_at: u64,
}
fn timestamp_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(u64::MAX as u128) as u64
}
fn client_field(value: &Value, limit: usize, fallback: &str) -> String {
    value.as_str().filter(|text| !text.trim().is_empty()).unwrap_or(fallback).chars().filter(|ch| !ch.is_control()).take(limit).collect()
}
async fn connection_snapshot(bridge: &Bridge) -> Vec<Value> {
    let sessions = bridge.sessions.lock().await;
    let mut entries: Vec<_> = sessions.iter().filter(|(_, session)| session.seen.elapsed() < Duration::from_secs(1800))
        .map(|(id, session)| json!({"id":id,"client":session.client,"version":session.version,"connectedAt":session.connected_at,"lastActivityAt":session.last_activity_at})).collect();
    entries.sort_by(|a,b| a["id"].as_str().cmp(&b["id"].as_str()));
    entries
}
#[derive(Clone)]
struct Bridge {
    token: Option<String>,
    origins: Vec<String>,
    inbox: Arc<Mutex<Inbox>>,
    sessions: Arc<Mutex<HashMap<String, ClientSession>>>,
}

pub fn router(token: Option<String>, origins: Vec<String>) -> Router {
    let bridge = Bridge {
        token: token.filter(|token| token.len() >= 32),
        origins,
        inbox: Arc::default(),
        sessions: Arc::default(),
    };
    Router::new()
        .route("/mcp", post(rpc).get(no_stream).delete(end_session))
        .route("/api/agent/poll", post(poll))
        .route("/api/agent/heartbeat", post(heartbeat))
        .route("/api/agent/result", post(complete).layer(axum::extract::DefaultBodyLimit::max(crate::db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024)))
        .route("/api/agent/status", get(status))
        .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024))
        .with_state(bridge)
}

fn authorize(
    bridge: &Bridge,
    headers: &HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
) -> Result<(), StatusCode> {
    if !peer.is_some_and(|peer| peer.0.ip().is_loopback()) {
        return Err(StatusCode::FORBIDDEN);
    }
    let token = bridge
        .token
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let expected = format!("Bearer {token}");
    let supplied = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    // Constant-time compare equal-length bearer credentials.
    if supplied.len() != expected.len()
        || supplied
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            != 0
    {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if let Some(origin) = headers.get("origin") {
        let origin = origin.to_str().map_err(|_| StatusCode::FORBIDDEN)?;
        if !bridge.origins.iter().any(|allowed| allowed == origin) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    if let Some(host) = headers.get("host") {
        let host = host.to_str().map_err(|_| StatusCode::FORBIDDEN)?;
        let url = url::Url::parse(&format!("http://{host}")).map_err(|_| StatusCode::FORBIDDEN)?;
        if !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(())
}

async fn no_stream(
    State(bridge): State<Bridge>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
) -> StatusCode {
    authorize(&bridge, &headers, peer)
        .err()
        .unwrap_or(StatusCode::METHOD_NOT_ALLOWED)
}
async fn status(
    State(bridge): State<Bridge>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
) -> Result<Json<Value>, StatusCode> {
    authorize(&bridge, &headers, peer)?;
    let inbox = bridge.inbox.lock().await;
    Ok(Json(
        json!({"connected": inbox.editor.as_ref().is_some_and(|(_,time)| time.elapsed() < Duration::from_secs(5))}),
    ))
}

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0", "id":id,"error":{"code":code,"message":message}})
}
async fn rpc(
    State(bridge): State<Bridge>,
    mut headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Json(mut request): Json<Value>,
) -> Result<Response, StatusCode> {
    authorize(&bridge, &headers, peer)?;
    if request["jsonrpc"] != "2.0"
        || !request.is_object()
        || (request
            .get("id")
            .is_some_and(|id| !id.is_string() && !id.is_number() && !id.is_null()))
    {
        return Ok(Json(error(Value::Null, -32600, "Invalid request")).into_response());
    }
    let method = request["method"].as_str().unwrap_or_default().to_owned();
    if request.get("id").is_none_or(Value::is_null) {
        return if method.starts_with("notifications/") {
            Ok(StatusCode::ACCEPTED.into_response())
        } else {
            Ok(Json(error(Value::Null, -32600, "Request id required")).into_response())
        };
    }
    if let Some(session) = headers.get("mcp-session-id") {
        let session = session
            .to_str()
            .map_err(|_| StatusCode::BAD_REQUEST)?
            .to_owned();
        let mut sessions = bridge.sessions.lock().await;
        sessions.retain(|_, time| time.seen.elapsed() < Duration::from_secs(1800));
        let time = sessions.get_mut(&session).ok_or(StatusCode::NOT_FOUND)?;
        time.seen = Instant::now();
        time.last_activity_at = timestamp_ms();
        headers.insert(
            "x-agent-id",
            session.parse().map_err(|_| StatusCode::BAD_REQUEST)?,
        );
    }
    if method == "resources/list" {
        return Ok(Json(json!({"jsonrpc":"2.0","id":request["id"],"result":{"resources":[
            {"uri":"project://document","name":"Current composition","mimeType":"application/json"},
            {"uri":"project://history","name":"History and checkpoints","mimeType":"application/json"}
        ]}})).into_response());
    }
    let uri = if method == "resources/read" {
        let uri = request["params"]["uri"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let tool = match uri.as_str() {
            "project://document" => "get_project",
            "project://history" => "get_history",
            _ => {
                return Ok(
                    Json(error(request["id"].clone(), -32602, "Unknown resource")).into_response(),
                )
            }
        };
        request["method"] = json!("tools/call");
        request["params"] = json!({"name":tool});
        Some(uri)
    } else {
        None
    };
    let client_info = request["params"]["clientInfo"].clone();
    let (status, Json(mut body)) =
        dispatch_rpc(State(bridge.clone()), headers, peer, Json(request)).await?;
    if let Some(uri) = uri {
        if body["result"]["isError"] == true {
            body = error(body["id"].clone(), -32000, "Visible editor unavailable");
        } else if let Some(text) = body["result"]["content"][0]["text"].as_str() {
            body["result"] =
                json!({"contents":[{"uri":uri,"mimeType":"application/json","text":text}]});
        }
    }
    let mut response = (status, Json(body.clone())).into_response();
    if method == "initialize" && body.get("result").is_some() {
        let mut sessions = bridge.sessions.lock().await;
        sessions.retain(|_, time| time.seen.elapsed() < Duration::from_secs(1800));
        if sessions.len() >= 128 {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        let session = Uuid::new_v4().to_string();
        let now = timestamp_ms();
        sessions.insert(session.clone(), ClientSession { seen: Instant::now(), client: client_field(&client_info["name"],128,"MCP client"), version: client_field(&client_info["version"],64,""), connected_at:now, last_activity_at:now });
        response.headers_mut().insert(
            "mcp-session-id",
            session.parse().expect("UUID is a valid header"),
        );
    }
    Ok(response)
}

async fn end_session(
    State(bridge): State<Bridge>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
) -> Result<StatusCode, StatusCode> {
    authorize(&bridge, &headers, peer)?;
    let session = headers
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    if bridge.sessions.lock().await.remove(session).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    let mut inbox = bridge.inbox.lock().await;
    inbox
        .pending
        .retain(|_, pending| pending.command["actor"].as_str() != Some(session));
    if inbox.released_actors.len() < 128 {
        inbox.released_actors.push(session.into());
    }
    cleanup(&mut inbox);
    Ok(StatusCode::NO_CONTENT)
}

async fn dispatch_rpc(
    State(bridge): State<Bridge>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Json(request): Json<Value>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    authorize(&bridge, &headers, peer)?;
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || !request.is_object() {
        return Ok((StatusCode::OK, Json(error(id, -32600, "Invalid request"))));
    }
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if id.is_null() {
        return if method == "notifications/initialized" {
            Ok((StatusCode::ACCEPTED, Json(Value::Null)))
        } else {
            Ok((
                StatusCode::OK,
                Json(error(id, -32600, "Request id required")),
            ))
        };
    }
    let params = &request["params"];
    let result = match method {
        "initialize" => {
            json!({"protocolVersion":"2025-03-26", "capabilities":{"tools":{},"resources":{}},"serverInfo":{"name":"video-kadr","version":"0.1.0"},"instructions":"Enable the Agent panel in the open composition. Read get_project before editing; ordinary compositions use revision as expectedRevision; nested projects use projectHash as expectedProjectHash for supported batch and caption edits; nativeBatch in get_project lists native capabilities. Editor tool times are integer microseconds. command_list/command_run/command_batch expose a partial Filmcraft export catalog: source time is 254016000000 ticks/second, with decimal strings for exact IDs/ticks. Source exports require explicit document/library bindings; preset and queue mutations are outside composition Undo. apply_batch is atomic; dry_run validates without committing."})
        }
        "ping" => json!({}),
        "tools/list" => {
            json!({"tools": TOOL_NAMES.iter().map(|name| tool_schema(name)).collect::<Vec<_>>()})
        }
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or_default();
            if !TOOL_NAMES.contains(&name) {
                return Ok((StatusCode::OK, Json(error(id, -32602, "Unknown tool"))));
            }
            if matches!(name,"begin_agent_session"|"end_agent_session") && !headers.contains_key("mcp-session-id") {
                return Ok((StatusCode::OK,Json(error(id,-32602,"Initialize an MCP session before managing agent work"))));
            }
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            if !arguments.is_object() {
                return Ok((
                    StatusCode::OK,
                    Json(error(id, -32602, "Arguments must be an object")),
                ));
            }
            let actor = headers
                .get("x-agent-id")
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty() && value.len() <= 128)
                .unwrap_or("mcp-client");
            let ticket = Uuid::new_v4().to_string();
            let (tx, rx) = oneshot::channel();
            {
                let mut inbox = bridge.inbox.lock().await;
                cleanup(&mut inbox);
                if !inbox
                    .editor
                    .as_ref()
                    .is_some_and(|(_, time)| time.elapsed() < Duration::from_secs(5))
                {
                    return Ok((
                        StatusCode::OK,
                        Json(
                            json!({"jsonrpc":"2.0","id":id,"result":tool_result(json!({"error":"Open the composition and connect its Agent panel"}), true)}),
                        ),
                    ));
                }
                if inbox.pending.len() >= 32 {
                    return Err(StatusCode::TOO_MANY_REQUESTS);
                }
                let command_timeout = command_timeout_for_tool(name);
                let expires_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
                    + command_timeout.as_millis();
                inbox.pending.insert(ticket.clone(), Pending { command: json!({"id":ticket,"actor":actor,"tool":name,"args":arguments,"expiresAt":expires_at}), expires: Instant::now() + command_timeout, response: tx, claimed:false });
                inbox.queue.push_back(ticket.clone());
            }
            match tokio::time::timeout(command_timeout_for_tool(name), rx).await {
                Ok(Ok(response)) => tool_result(
                    response
                        .get("result")
                        .cloned()
                        .unwrap_or_else(|| response.clone()),
                    response.get("error").is_some() || name == "command_batch" && response["result"]["failed"].as_u64().is_some_and(|failed| failed > 0),
                ),
                _ => {
                    bridge.inbox.lock().await.pending.remove(&ticket);
                    tool_result(
                        json!({"error":"Editor did not confirm completion; inspect project before retrying", "operationId":ticket}),
                        true,
                    )
                }
            }
        }
        _ => return Ok((StatusCode::OK, Json(error(id, -32601, "Method not found")))),
    };
    // tools/call early errors above are still JSON-RPC responses.
    Ok((
        StatusCode::OK,
        Json(json!({"jsonrpc":"2.0","id":id,"result":result})),
    ))
}
fn tool_result(result: Value, failed: bool) -> Value {
    json!({"content":[{"type":"text","text":result.to_string()}],"isError":failed})
}
fn tool_schema(name: &str) -> Value {
    if name == "command_batch" {
        return json!({"name":name,"description":"Run available Filmcraft export commands sequentially in Rust: {steps:[{id,params?}],stop_on_error?:true}. Returns {completed,failed,results:[{ok,result|error}]}; failures mark the MCP result as an error. Completed steps remain applied; this is not atomic apply_batch and creates no composition Undo. The editor deadline applies to all steps; accepted exports may continue after timeout, but later steps stop. Scope is the partial native export catalog from command_list.","annotations":{"readOnlyHint":false,"destructiveHint":true,"idempotentHint":false,"openWorldHint":false},"inputSchema":{"type":"object","properties":{"steps":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string","minLength":1},"params":{"type":["object","null"]}},"required":["id"],"additionalProperties":false}},"stop_on_error":{"type":["boolean","null"]}},"required":["steps"],"additionalProperties":false}});
    }

    if name == "command_list" {
        return json!({"name":name,"description":"Discover the transferred Filmcraft export preset and queue commands with id, label, parameter docs and enabled state. This is a partial catalog of eighteen native export commands, not all source editor commands. Requires the connected editor's authenticated owner.","annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false},"inputSchema":{"type":"object","properties":{"filter":{"type":["string","null"]},"enabled_only":{"type":["boolean","null"]}},"additionalProperties":false}});
    }
    if name == "command_run" {
        return json!({"name":name,"description":"Run an available Filmcraft command from command_list using {id,params?}. Supported scope is native export formats, presets, Quick Export and the serial export queue. Queue add requires explicit source document/sequence/library bindings; files use owned outputs and portable preset JSON. Preset/queue mutations persist outside composition Undo. Completion timeout is ambiguous: inspect state before retrying; an accepted export keeps running.","annotations":{"readOnlyHint":false,"destructiveHint":true,"idempotentHint":false,"openWorldHint":false},"inputSchema":{"type":"object","properties":{"id":{"type":"string","minLength":1},"params":{"type":["object","null"]}},"required":["id"],"additionalProperties":false}});
    }

    let mut properties = match name {
        "jump_history" => json!({"entryId":{"type":"string","minLength":1,"maxLength":128},"side":{"type":"string","enum":["before","after"],"default":"after"},"expectedRevision":{"type":"integer","minimum":0},"expectedProjectHash":{"type":"string","pattern":"^[0-9a-f]{64}$"}}),
        "begin_agent_session" => json!({"reason":{"type":"string","minLength":1,"maxLength":512}}),
        "end_agent_session" => json!({"force":{"type":"boolean"}}),
        "restyle_captions" => json!({"expectedRevision":{"type":"integer","minimum":0},"layer_ids":{"type":["array","null"],"minItems":1,"maxItems":512,"items":{"type":"string","minLength":1}},"font_family":{"type":["string","null"],"enum":["Noto Sans","Arial Unicode MS","DejaVu Sans","Arial",null]},"font_size_px":{"type":["number","null"],"minimum":1,"maximum":512},"color":{"type":["object","null"],"properties":{"r":{"type":"integer","minimum":0,"maximum":255},"g":{"type":"integer","minimum":0,"maximum":255},"b":{"type":"integer","minimum":0,"maximum":255},"a":{"type":"integer","minimum":0,"maximum":255}},"required":["r","g","b","a"],"additionalProperties":false},"outline_width":{"type":["number","null"],"minimum":0,"maximum":100}}),
        "merge_captions" => json!({"expectedRevision":{"type":"integer","minimum":0},"layer_ids":{"type":"array","minItems":2,"maxItems":512,"items":{"type":"string","minLength":1}}}),
        "export_captions" => json!({"format":{"type":"string","enum":["srt","vtt"]},"track_id":{"type":["string","null"],"minLength":1,"maxLength":128},"composition_id":{"type":["string","null"],"minLength":1,"maxLength":128}}),
        "get_color_track" => json!({"clipId":{"type":"string"},"composition_id":{"type":"string","description":"Native nested composition scope; defaults to root"}}),
        "freeze_frame" => json!({"expectedProjectHash":{"type":"string","pattern":"^[0-9a-f]{64}$"},"composition_id":{"type":"string","minLength":1,"maxLength":128},"clipId":{"type":"string","minLength":1,"maxLength":128},"time_us":{"type":"integer","minimum":0,"maximum":9007199254740991u64}}),
        "get_keyframes" => json!({"clipId":{"type":"string"},"property":{"type":"string","minLength":1,"maxLength":128},"envelope":{"type":"string","enum":["visual","audio","mask","path","effect"]},"maskId":{"type":"string"},"effectIndex":{"type":"integer","minimum":0,"maximum":1023},"effectKind":{"type":"string","minLength":1,"maxLength":128}}),
        "analyze_clip" | "measure_shots" | "auto_split_by_shot" | "mark_shots"
        | "apply_shot_cuts" => {
            json!({"clipId":{"type":"string"},"expectedRevision":{"type":"integer"},"threshold":{"type":"number","minimum":0.05,"maximum":1},"minShotTicks":{"type":"integer","minimum":1,"maximum":60000000},"disabledCuts":{"type":"array","items":{"type":"integer"}},"discardedRowIds":{"type":"array","items":{"type":"string"}},"stats":{"type":"boolean"}})
        }
        "compare_frames" => {
            json!({"clipId":{"type":"string"},"leftTick":{"type":"integer","minimum":0},"rightTick":{"type":"integer","minimum":0}})
        }
        "apply_batch" | "dry_run" => {
            json!({"expectedRevision":{"type":"integer","minimum":0},"operations":{"type":"array","minItems":1,"maxItems":100,"items":{"oneOf":operation_schemas()}}})
        }
        "create_checkpoint" => json!({"label":{"type":"string","minLength":1,"maxLength":128}}),
        "delete_checkpoint" | "restore_checkpoint" => json!({"id":{"type":"string"}}),
        "set_history_lock" => {
            json!({"locked":{"type":"boolean"},"reason":{"type":"string","maxLength":256}})
        }
        _ => json!({}),
    };
    if matches!(name,"merge_captions"|"restyle_captions") {
        properties["expectedProjectHash"]=json!({"type":"string","pattern":"^[0-9a-f]{64}$","description":"Open nested project's get_project.projectHash. Ordinary compositions use expectedRevision instead."});
        if name=="merge_captions" { properties["composition_id"]=json!({"type":["string","null"],"minLength":1,"maxLength":128,"description":"Optional nested composition scope for ambiguous clip IDs."}); }
    }
    if name=="get_keyframes" { properties["composition_id"]=json!({"type":"string","minLength":1,"maxLength":128,"description":"Native composition scope; defaults to root. Native supported envelopes are in get_project.nativeBatch.keyframeEnvelopes."}); }
    if matches!(name,"apply_batch"|"dry_run") { properties["expectedProjectHash"]=json!({"type":"string","pattern":"^[0-9a-f]{64}$","description":"Current nested project's get_project.projectHash. Ordinary compositions use expectedRevision."});properties["composition_id"]=json!({"type":"string","minLength":1,"maxLength":128,"description":"Nested batch scope; defaults to root independently of the editor's current view."}); }
    if name=="restore_checkpoint" { properties["expectedProjectHash"]=json!({"type":"string","pattern":"^[0-9a-f]{64}$","description":"Required for an open nested project: current get_project.projectHash."}); }
    let required = match name {
        "jump_history" => vec!["entryId"],
        "begin_agent_session" => vec!["reason"],
        "restyle_captions" => vec![],
        "merge_captions" => vec!["layer_ids"],
        "export_captions" => vec!["format"],
        "get_color_track" => vec!["clipId"],
        "get_keyframes" => vec!["clipId","property"],
        "freeze_frame" => vec!["expectedProjectHash","clipId","time_us"],
        "analyze_clip" | "measure_shots" => vec!["clipId"],
        "auto_split_by_shot" | "mark_shots" | "apply_shot_cuts" => {
            vec!["clipId", "expectedRevision"]
        }
        "compare_frames" => vec!["clipId", "leftTick", "rightTick"],
        "apply_batch" | "dry_run" => vec!["operations"],
        "create_checkpoint" => vec!["label"],
        "delete_checkpoint" | "restore_checkpoint" => vec!["id"],
        "set_history_lock" => vec!["locked"],
        _ => vec![],
    };
    if matches!(name,"apply_batch"|"dry_run") { return json!({"name":name,"description":"Atomic batch or read-only rehearsal on the visible editor. Ordinary: expectedRevision and full advertised authoring operations. Nested: expectedProjectHash, optional composition_id (default root), and only nativeBatch.operations/patch fields advertised by get_project; unsupported native operations refuse atomically. Nested uses owned helpers plus Rust validation and commits once; existing wire effects/animation remain retained. Native update_track is native-only; per-layer locks and escapeLink are not native equivalents. Times are integer microseconds.","inputSchema":{"type":"object","properties":properties,"required":required,"oneOf":[{"required":["expectedRevision"]},{"required":["expectedProjectHash"]}],"additionalProperties":false}}); }
    if name=="begin_agent_session" { return json!({"name":name,"description":"Begin an owned work session for this project opening. Creates one Pre-agent checkpoint and enters agent view once. Same connection repeats idempotently without changing its view; another owner refuses. Requires initialized MCP session. Unrecorded.","inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}}); }
    if name=="end_agent_session" { return json!({"name":name,"description":"End your work session and release only its owner's history lock; force:true may end another owner's session. Keeps view, checkpoints and logs; does not cancel tasks or disconnect MCP. Requires initialized MCP session. Unrecorded.","inputSchema":{"type":"object","properties":properties,"additionalProperties":false}}); }
    if name=="list_checkpoints" { return json!({"name":name,"description":"List current editor-context checkpoints oldest first as id, label, actor and created_at. No document snapshots; read-only.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}); }
    if name == "restyle_captions" { return json!({"name":name,"description":"Restyle caption-role Text lanes of the ordinary composition or every authored composition of an open nested project, or explicit layer_ids. One Undo, no timing/text edits, normal titles excluded. Omitted/null style fields stay unchanged; send at least one style field. Zero outline removes it; positive outline retains existing color or uses black. Requires expectedRevision for ordinary compositions or expectedProjectHash from get_project for nested projects.","inputSchema":{"type":"object","properties":properties,"required":required,"oneOf":[{"required":["expectedRevision"]},{"required":["expectedProjectHash"]}],"additionalProperties":false}}); }
    if name == "jump_history" {return json!({"name":name,"description":"Jump before/after a retained get_history entry ID using real Undo/Redo. IDs survive Undo/Redo; pruned or discarded branch IDs refuse. Creates no new entry. Requires current ordinary expectedRevision or native expectedProjectHash, respects history locks; native base jump may close a project opened through history.","inputSchema":{"type":"object","properties":properties,"required":required,"oneOf":[{"required":["expectedRevision"]},{"required":["expectedProjectHash"]}],"additionalProperties":false}});}
    if name == "merge_captions" { return json!({"name":name,"description":"Merge captions from one caption-role lane into the earliest cue. Union span, newline-joined text in timeline order, earliest style retained. Refuses locked targets, other lanes, ordinary titles and collisions with unselected cues. One Undo. Requires expectedRevision for ordinary compositions or expectedProjectHash from nested get_project.","inputSchema":{"type":"object","properties":properties,"required":required,"oneOf":[{"required":["expectedRevision"]},{"required":["expectedProjectHash"]}],"additionalProperties":false}}); }
    if name == "export_captions" { return json!({"name":name,"description":"Export caption-role Text lanes in stable chronological order as SRT or WebVTT. Optional composition_id selects a nested composition (default root); track_id selects a caption lane in that composition. Ordinary title tracks and mismatched scopes refuse. Returns format, cues and body. Read-only; includes hidden and locked lanes and preserves overlapping cue times.","inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}}); }
    if name == "freeze_frame" { return json!({"name":name,"description":"Insert a captured one-second FreezeFrame into an authenticated saved native project. Requires current get_project.projectHash and an unchanged saved revision/document. Optional composition_id defaults root; time_us is absolute composition time. Publishes a fresh still, splits/ripples the video lane and creates one local Undo entry. This is a server-persisted mutation, not a dry-run/batch operation. Unsupported native effects/playback profiles refuse. If the server commits after the editor context changes, local state is retained and the caller must reload before another save.","inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}}); }
    if name == "get_keyframes" { return json!({"name":name,"description":"Read scalar animation before editing: ordinary revision or native projectHash (optional native composition_id defaults root), constant value or keyframes with native tick/timeBase, layer-local t_local_us, timeline t_us, effective tangents and outgoing segment. Exact preset_id is omitted for hand-tuned segments and the last key. envelope defaults to visual; mask requires maskId. Native effect channels require effectIndex plus effectKind and use authored animation parameters. Read-only.","inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}}); }
    if name == "get_color_track" { return json!({"name":name,"description":"Read text or color-layer animation (native nested scope defaults to root and returns projectHash; packed native timing retains fractional microseconds and nativeTrack): constant RGBA bytes or keyframes with authored tick/timeBase, layer-local t_local_us, timeline t_us, resolved temporal tangents and outgoing segment. Read-only. set_color_key uses composition playhead ticks; delete_color_key uses exact authored key ticks, including negative retained keys after trimming.","inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}}); }
    json!({"name":name,"description":format!("{name}: operate on the visible Video Kadr composition. Batch operations use existing editor commands and validation; integer times are microseconds. Shot parameters and frame-comparison ticks are source-absolute; rows include composition-clock times. Shot editing requires expectedRevision and commits once. analyze_clip caches a whole-source floor scan; stats opt-in measures reviewed spans."),"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})
}
fn operation_schemas() -> Vec<Value> {
    let specs: &[(&str, &[&str])] = &[
        ("slip_layer", &["clipId", "sourceDeltaTicks"]),
        ("set_color_key", &["clipId", "playheadTicks", "value"]),
        ("delete_color_key", &["clipId", "tick"]),
        ("edit_color_key", &["clipId", "tick"]),
        ("move_color_keys", &["edits"]),
        ("set_color_preset", &["clipId", "preset"]),
        ("set_color_interpolation", &["clipId", "interpolation"]),
        ("set_keyframe_extrapolation", &["clipId", "property", "side", "extrapolation"]),
        ("set_color_extrapolation", &["clipId", "side", "extrapolation"]),
        ("set_color_tangent_mode", &["clipId", "tick", "mode"]),
        ("edit_color_tangents", &["clipId", "tick"]),
        ("convert_motion_path", &["clipId", "nodeIdPrefix", "options"]),
        ("edit_motion_path", &["clipId", "edit"]),
        ("arrive_at_path_node", &["clipId", "index", "playheadTicks"]),
        ("insert_path_node_near", &["clipId", "point", "nodeId", "maxDistance"]),
        ("set_path_progress", &["clipId", "playheadTicks", "value"]),
        ("switch_position_mode", &["clipId", "positionMode", "nodeId"]),
        ("create_link", &["id", "memberIds"]),
        ("remove_link", &["id"]),
        ("duplicate_layer", &["clipId", "id"]),
        ("paste_layers", &["subjects"]),
        ("add_track", &["track"]),
        ("update_track", &["trackId", "patch"]),
        ("delete_track", &["trackId"]),
        ("reorder_track", &["trackId", "index"]),
        ("add_layer", &["trackId", "clip"]),
        ("update_layer", &["clipId", "patch"]),
        ("set_keyframe_preset", &["clipId", "property", "ticks", "preset"]),
        ("set_keyframe_tangent_mode", &["clipId", "property", "ticks", "mode"]),
        ("edit_keyframe_tangents", &["clipId", "property", "tick"]),
        ("edit_keyframe", &["clipId", "property", "tick"]),
        ("move_keyframe", &["clipId", "property", "tick", "newTick"]),
        ("upsert_keyframe", &["clipId", "property", "tick", "value"]),
        ("delete_keyframe", &["clipId", "property", "tick"]),
        ("update_canvas", &["patch"]),
        ("delete_layer", &["clipId"]),
        ("set_layers_enabled", &["clipIds", "enabled"]),
        ("set_layer_locked", &["clipId", "locked"]),
        ("ripple_delete_layers", &["clipIds"]),
        ("ripple_delete_gap", &["trackId", "startTicks", "endTicks"]),
        ("move_layer", &["clipId", "trackId", "startTicks"]),
        ("move_layers", &["clipIds", "trackId", "startTicks"]),
        (
            "move_layer_placements",
            &["placements", "anchorId", "startTicks"],
        ),
        ("move_layers_to_new_track", &["clipIds", "trackId"]),
        ("trim_layer", &["clipId", "startTicks", "endTicks"]),
        ("split_layer", &["clipId", "atTicks", "rightClipId"]),
        ("add_marker", &["marker"]),
        ("remove_marker", &["markerId"]),
        ("add_transition", &["trackId", "transition"]),
        ("remove_transition", &["trackId", "transitionId"]),
    ];
    specs.iter().map(|(name, fields)| {
        let mut properties = serde_json::Map::new();
        properties.insert("op".into(), json!({"type":"string","const":name}));
        for field in *fields {
            let schema = match *field {
                "options" if *name == "convert_motion_path" => json!({"type":"object","properties":{"startFrame":{"type":"integer","minimum":0},"endFrame":{"type":"integer","minimum":1},"tolerancePx":{"type":"number","exclusiveMinimum":0},"everyFrames":{"type":"integer","minimum":1},"xyMode":{"type":"string","enum":["editable","bake"]}},"required":["startFrame","endFrame","tolerancePx","everyFrames","xyMode"],"additionalProperties":false,"description":"Native nested XY/path conversion through Rust fitter; clip-local frame range, canvas rational FPS; rejects results outside tolerance. Editable uses every frame, bake uses everyFrames."}),
                "edit" if *name == "edit_motion_path" => {
                    let point=json!({"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"],"additionalProperties":false});
                    let variants: &[(&str,&[&str])] = &[("translate",&["delta"]),("remove",&["index"]),("toggle_segment",&["index"]),("solve",&[]),("edit",&["index","part","value"]),("mode",&["index","mode"]),("insert",&["segment","t","id"]),("append",&["id","point"])];
                    let edits:Vec<Value>=variants.iter().map(|(operation,fields)|{
                        let mut props=serde_json::Map::new();props.insert("operation".into(),json!({"const":operation}));
                        for field in *fields {props.insert((*field).into(),match *field {
                            "delta"|"value"|"point"=>point.clone(),
                            "index"|"segment"=>json!({"type":"integer","minimum":0}),
                            "t"=>json!({"type":"number","exclusiveMinimum":0,"exclusiveMaximum":1}),
                            "part"=>json!({"type":"string","enum":["point","in_handle","out_handle"]}),
                            "mode"=>json!({"type":"string","enum":["corner","smooth","auto"]}),
                            _=>json!({"type":"string","minLength":1})
                        });}
                        let required:Vec<&str>=std::iter::once("operation").chain(fields.iter().copied()).collect();
                        json!({"type":"object","properties":props,"required":required,"additionalProperties":false})
                    }).collect();
                    json!({"oneOf":edits,"description":"Native nested projects: Rust motion path edit; one atomic batch/Undo. Geometry stays native."})
                },
                "point" if *name == "insert_path_node_near" => json!({"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"],"additionalProperties":false}),
                "maxDistance" => json!({"type":"number","minimum":0}),
                "sourceDeltaTicks" => json!({"type":"integer","description":"Signed source-time microseconds for slip. Video/audio only; timeline placement and local animation stay fixed; source bounds clamp. Freeze and speed ramps refuse."}),
                "placements" => json!({"type":"array","minItems":1,"maxItems":512,"items":{"type":"object","properties":{"clipId":{"type":"string","minLength":1},"trackId":{"type":"string","minLength":1}},"required":["clipId","trackId"],"additionalProperties":false},"description":"Exact selected subjects with individual destination lanes; anchorId determines their common timeline shift. Links do not expand this set."}),
                "edits" => json!({"type":"array","minItems":1,"maxItems":4096,"items":{"type":"object","properties":{"clipId":{"type":"string","minLength":1},"tick":{"type":"number"},"newTick":{"type":"number"}},"required":["clipId","tick","newTick"],"additionalProperties":false},"description":"Atomic color key clock edits across text/color layers; authored track ticks or native signed fractional microseconds. Swaps supported; duplicate subjects and final collisions refuse."}),
                "playheadTicks" => json!({"type":"integer","minimum":0,"description":"Composition playhead ticks; must fall inside the clip."}),
                "positionMode" => json!({"type":"string","enum":["xy","path"],"description":"Exact static XY to one-node path or one-node path to XY. Animated XY/multi-node conversion requires fitter."}),
                "value" if *name == "set_color_key" => json!({"type":"object","properties":{"r":{"type":"integer","minimum":0,"maximum":255},"g":{"type":"integer","minimum":0,"maximum":255},"b":{"type":"integer","minimum":0,"maximum":255},"a":{"type":"integer","minimum":0,"maximum":255}},"required":["r","g","b","a"],"additionalProperties":false}),
                "interpolation" => json!({"oneOf":[
                    {"type":"string","enum":["hold","linear","ease_in","ease_out","ease_in_out","ease_in_out_cubic","bounce_in","bounce_out","bounce_in_out"]},
                    {"type":"object","properties":{"kind":{"const":"bezier"},"p1":{"type":"array","minItems":2,"maxItems":2,"prefixItems":[{"type":"number","minimum":0,"maximum":1},{"type":"number"}]},"p2":{"type":"array","minItems":2,"maxItems":2,"prefixItems":[{"type":"number","minimum":0,"maximum":1},{"type":"number"}]}},"required":["kind","p1","p2"],"additionalProperties":false},
                    {"type":"object","properties":{"kind":{"const":"elastic"},"dir":{"type":"string","enum":["in","out","in_out"]},"amplitude":{"type":"number","minimum":1},"period":{"type":"number","exclusiveMinimum":0}},"required":["kind","dir","amplitude","period"],"additionalProperties":false}
                ]}),
                "value" => json!({"type":"number"}),
                "side" if matches!(*name, "set_color_extrapolation" | "set_keyframe_extrapolation") => json!({"type":"string","enum":["before","after"]}),
                "extrapolation" => json!({"type":"string","enum":["hold","continue","loop","ping_pong","offset"]}),
                "tick" if matches!(*name, "delete_color_key" | "edit_color_key" | "set_color_tangent_mode" | "edit_color_tangents") => json!({"type":"number","description":"Exact color key tick; native packed tracks use signed, possibly fractional microseconds."}),
                "tick" if matches!(*name, "delete_color_key" | "set_color_tangent_mode" | "edit_color_tangents" | "edit_color_key") => json!({"type":"integer","description":"Exact key tick in the authored color track timeBase; may be negative after split/trim."}),
                "tick" | "newTick" => json!({"type":"integer","description":"Exact authored track tick. Native rebased tracks retain signed off-clip keys; use get_keyframes timeBase, never quantize to project FPS."}),
                "mode" => json!({"type":"string","enum":["auto","free"],"description":"Auto solves selected key tangents and converts adjacent segments to spline; Free retains solved coordinates."}),
                "property" => json!({"type":"string","enum":["x","y","scaleX","scaleY","rotationDegrees","opacity","gain","pan","width","height","progress"]}),
                "ticks" => json!({"type":"array","minItems":1,"uniqueItems":true,"items":{"type":"integer"},"description":"Existing exact clip-local key ticks in the animation track timeBase, including signed rebased native keys."}),
                "preset" => json!({"type":"string","enum":["ease","ease_in","ease_out","ease_in_out","ease_in_sine","ease_out_sine","ease_in_out_sine","ease_in_quad","ease_out_quad","ease_in_out_quad","ease_in_cubic","ease_out_cubic","ease_in_out_cubic","ease_in_quart","ease_out_quart","ease_in_out_quart","ease_in_quint","ease_out_quint","ease_in_out_quint","ease_in_expo","ease_out_expo","ease_in_out_expo","ease_in_circ","ease_out_circ","ease_in_out_circ","ease_in_back","ease_out_back","ease_in_out_back","linear","hold","ease_in_elastic","ease_out_elastic","ease_in_out_elastic","ease_in_bounce","ease_out_bounce","ease_in_out_bounce"],"description":"Bakes selected outgoing segments and arriving controls as Free. Linked scale axes update together."}),
                "track" => json!({"type":"object","required":["id","kind","name","locked","clips"],"description":"Complete CompositionTrack. kind video/audio/image/text. Visual tracks require hidden:boolean; video requires muted:boolean; audio requires muted/solo:boolean; clips is an array."}),
                "clip" => json!({"type":"object","required":["id","kind","timelineStartTicks"],"description":"Complete CompositionClip. Text: durationTicks,text,x,y,opacity,style{fontSizePx,color:#RRGGBBAA,align:left/center/right}. Source clips: sourceId,sourceInTicks,sourceOutTicks and type-specific parameters; read existing project clips for their shape."}),
                "marker" => json!({"type":"object","required":["id","tick","label"],"properties":{"id":{"type":"string"},"tick":{"type":"integer","minimum":0},"label":{"type":"string"},"color":{"type":"string"}}}),
                "transition" | "patch" => json!({"type":"object","description":"Existing editor domain fields; the complete resulting document is validated. Layer patches cannot change id/kind."}),
                "enabled" | "locked" => json!({"type":"boolean"}),
                "index" | "startTicks" | "endTicks" | "atTicks" => json!({"type":"integer","minimum":0}),
                "subjects" => json!({"type":"array","minItems":1,"maxItems":512,"items":{"type":"object","properties":{"clipId":{"type":"string"},"id":{"type":"string"}},"required":["clipId","id"],"additionalProperties":false}}),
                "clipIds" => json!({"type":"array","minItems":1,"maxItems":512,"uniqueItems":true,"items":{"type":"string","minLength":1}}),
                "memberIds" => json!({"type":"array","minItems":2,"maxItems":512,"uniqueItems":true,"items":{"type":"string","minLength":1}}),
                _ => json!({"type":"string","minLength":1}),
            };
            properties.insert((*field).into(), schema);
        }
        if *name == "edit_color_key" {
            properties.insert("newTick".into(), json!({"type":"number","description":"Exact color-track tick; native packed tracks retain signed fractional microseconds."}));
            properties.insert("value".into(), json!({"type":"object","properties":{"r":{"type":"integer","minimum":0,"maximum":255},"g":{"type":"integer","minimum":0,"maximum":255},"b":{"type":"integer","minimum":0,"maximum":255},"a":{"type":"integer","minimum":0,"maximum":255}},"required":["r","g","b","a"],"additionalProperties":false}));
        }
        if matches!(*name, "set_color_preset" | "set_color_interpolation") { properties.insert("tick".into(), json!({"type":"number","description":"Optional exact color key tick, including fractional native microseconds. Omit for authored inherited default or all native packed segments."})); }
        if *name == "edit_color_tangents" {
            properties.insert("side".into(), json!({"type":"string","enum":["incoming","outgoing"]}));
            properties.insert("point".into(), json!({"type":"object","required":["x","y"],"properties":{"x":{"type":"number","minimum":0,"maximum":1},"y":{"type":"number"}},"additionalProperties":false}));
            properties.insert("continuity".into(), json!({"type":"string","enum":["smooth","broken"]}));
        }
        if *name == "add_track" { properties.insert("index".into(), json!({"type":"integer","minimum":0})); }
        if matches!(*name, "set_keyframe_extrapolation" | "set_keyframe_preset" | "set_keyframe_tangent_mode" | "edit_keyframe_tangents" | "move_keyframe" | "upsert_keyframe" | "delete_keyframe" | "edit_keyframe") {
            properties.insert("envelope".into(), json!({"type":"string","enum":["visual","audio","mask","path","effect"],"description":"Path targets progress. Defaults to visual. Audio targets gain/pan; mask requires maskId. Native effect channels require exact effectIndex and effectKind; original source-owned curves remain separate."}));
            properties.insert("maskId".into(), json!({"type":"string","minLength":1}));
            properties.insert("effectIndex".into(), json!({"type":"integer","minimum":0,"maximum":1023}));
            properties.insert("effectKind".into(), json!({"type":"string","minLength":1,"maxLength":128}));
        }
        if matches!(*name, "upsert_keyframe" | "edit_keyframe") { properties.insert("preset".into(), json!({"type":"string","minLength":1,"description":"Optional canonical easing preset id, applied to the leaving segment as Free controls."})); }
        if matches!(*name, "edit_keyframe_tangents" | "edit_keyframe") {
            for side in ["incoming", "outgoing"] {
                properties.insert(side.into(), json!({"type":"object","required":["x","y"],"properties":{"x":{"type":"number","minimum":0,"maximum":1},"y":{"type":"number"}},"additionalProperties":false}));
            }
            properties.insert("continuity".into(), json!({"type":"string","enum":["smooth","broken"]}));
        }
        if *name == "edit_keyframe" {
            properties.insert("newTick".into(), json!({"type":"integer","description":"Exact authored track tick; native rebased keys may be negative."}));
            properties.insert("value".into(), json!({"type":"number"}));
        }
        if *name == "create_link" { properties.insert("reassign".into(), json!({"type":"boolean"})); }
        if matches!(*name, "move_layer" | "split_layer" | "trim_layer" | "duplicate_layer") { properties.insert("escapeLink".into(), json!({"type":"boolean"})); }
        if matches!(*name, "duplicate_layer" | "paste_layers") {
            properties.insert("trackId".into(), json!({"type":"string","minLength":1}));
            if *name == "duplicate_layer" { properties.insert("linkId".into(), json!({"type":"string","minLength":1})); }
            properties.insert("startTicks".into(), json!({"type":"integer","minimum":0}));
        }
        if *name == "move_layers_to_new_track" {properties.insert("anchor".into(),json!({"type":"object","properties":{"clipId":{"type":"string","minLength":1},"startTicks":{"type":"integer","minimum":0}},"required":["clipId","startTicks"],"additionalProperties":false,"description":"Optional selected anchor and landing time; omission preserves every layer's original clock."}));}
        if *name == "paste_layers" { properties.insert("linkIds".into(), json!({"type":"array","items":{"type":"string"},"uniqueItems":true})); }
        let required = std::iter::once("op").chain(fields.iter().copied()).collect::<Vec<_>>();
        json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
    }).collect()
}

fn cleanup(inbox: &mut Inbox) {
    inbox
        .pending
        .retain(|_, pending| pending.expires > Instant::now() && !pending.response.is_closed());
    inbox.queue.retain(|id| inbox.pending.contains_key(id));
}
fn command_timeout_for_tool(name: &str) -> Duration {
    if [
        "command_run",
        "command_batch",
        "analyze_clip",
        "measure_shots",
        "compare_frames",
        "auto_split_by_shot",
        "mark_shots",
        "apply_shot_cuts",
        "apply_batch",
        "dry_run",
    ]
    .contains(&name)
    {
        Duration::from_secs(240)
    } else {
        Duration::from_secs(15)
    }
}
async fn heartbeat(
    State(bridge): State<Bridge>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Json(body): Json<Value>,
) -> Result<Response, StatusCode> {
    authorize(&bridge, &headers, peer)?;
    let editor = body["editorId"].as_str().ok_or(StatusCode::BAD_REQUEST)?;
    let mut inbox = bridge.inbox.lock().await;
    let Some((id, time)) = inbox.editor.as_mut() else {
        return Err(StatusCode::CONFLICT);
    };
    if id != editor {
        return Err(StatusCode::CONFLICT);
    }
    *time = Instant::now();
    let released = std::mem::take(&mut inbox.released_actors);
    drop(inbox);
    if body["includeConnections"] == true {
        return Ok(Json(json!({"connections":connection_snapshot(&bridge).await,"releasedActors":released})).into_response());
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    if !released.is_empty() {
        response.headers_mut().insert("x-agent-released", serde_json::to_string(&released).expect("actor IDs serialize").parse().map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?);
    }
    Ok(response)
}
async fn poll(
    State(bridge): State<Bridge>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    authorize(&bridge, &headers, peer)?;
    let editor_id = body["editorId"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 128)
        .ok_or(StatusCode::BAD_REQUEST)?;
    let mut inbox = bridge.inbox.lock().await;
    cleanup(&mut inbox);
    if inbox
        .editor
        .as_ref()
        .is_some_and(|(id, time)| id != editor_id && time.elapsed() < Duration::from_secs(5))
    {
        return Err(StatusCode::CONFLICT);
    }
    if inbox.editor.as_ref().is_some_and(|(id, _)| id != editor_id) {
        inbox.pending.clear();
        inbox.queue.clear();
    }
    inbox.editor = Some((editor_id.into(), Instant::now()));
    let command = inbox
        .queue
        .pop_front()
        .and_then(|id| inbox.pending.get_mut(&id))
        .map(|pending| {
            pending.claimed = true;
            pending.command.clone()
        });
    let released_actors = std::mem::take(&mut inbox.released_actors);
    drop(inbox);
    let connections=connection_snapshot(&bridge).await;
    Ok(Json(
        json!({"command":command,"releasedActors":released_actors,"connections":connections}),
    ))
}
async fn complete(
    State(bridge): State<Bridge>,
    headers: HeaderMap,
    peer: Option<ConnectInfo<SocketAddr>>,
    Json(body): Json<Value>,
) -> Result<StatusCode, StatusCode> {
    authorize(&bridge, &headers, peer)?;
    let id = body["id"].as_str().ok_or(StatusCode::BAD_REQUEST)?;
    let mut inbox = bridge.inbox.lock().await;
    if !inbox
        .editor
        .as_ref()
        .is_some_and(|(id, _)| Some(id.as_str()) == body["editorId"].as_str())
    {
        return Err(StatusCode::CONFLICT);
    }
    let pending = inbox.pending.remove(id).ok_or(StatusCode::GONE)?;
    if !pending.claimed {
        return Err(StatusCode::CONFLICT);
    }
    let _ = pending.response.send(body);
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    const TOKEN: &str = "0123456789abcdef0123456789abcdef";
    fn app() -> Router {
        router(Some(TOKEN.into()), vec!["http://localhost:5173".into()])
    }
    fn request(path: &str, body: Value) -> Request<Body> {
        let mut request = Request::builder()
            .method("POST")
            .uri(path)
            .header("authorization", format!("Bearer {TOKEN}"))
            .header("content-type", "application/json")
            .header("host", "localhost:8080")
            .body(Body::from(body.to_string()))
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(
            "127.0.0.1:43210".parse::<SocketAddr>().unwrap(),
        ));
        request
    }
    async fn json_body(response: axum::response::Response) -> Value {
        serde_json::from_slice(
            &to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap()
    }
    #[tokio::test]
    async fn bearer_peer_origin_and_host_are_all_enforced() {
        for kind in ["token", "peer", "origin", "host"] {
            let mut req = request("/mcp", json!({"jsonrpc":"2.0","id":1,"method":"ping"}));
            match kind {
                "token" => {
                    req.headers_mut().remove("authorization");
                }
                "peer" => {
                    req.extensions_mut().insert(ConnectInfo(
                        "192.168.1.2:1234".parse::<SocketAddr>().unwrap(),
                    ));
                }
                "origin" => {
                    req.headers_mut()
                        .insert("origin", "https://evil.example".parse().unwrap());
                }
                _ => {
                    req.headers_mut()
                        .insert("host", "evil.example".parse().unwrap());
                }
            }
            let status = app().oneshot(req).await.unwrap().status();
            assert!(status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED);
        }
    }
    #[tokio::test]
    async fn disabled_bridge_fails_closed_and_catalog_is_discoverable() {
        let disabled = router(None, vec![])
            .oneshot(request("/mcp", json!({})))
            .await
            .unwrap();
        assert_eq!(disabled.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = json_body(
            app()
                .oneshot(request(
                    "/mcp",
                    json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(
            body["result"]["tools"].as_array().unwrap().len(),
            TOOL_NAMES.len()
        );
        let catalog = body["result"]["tools"].as_array().unwrap();
        let color = catalog.iter().find(|tool| tool["name"] == "get_color_track").unwrap();
        assert_eq!(color["inputSchema"]["required"], json!(["clipId"]));
        let batch = catalog.iter().find(|tool| tool["name"] == "apply_batch").unwrap();
        let operations = batch["inputSchema"]["properties"]["operations"]["items"]["oneOf"].as_array().unwrap();
        let set = operations.iter().find(|op| op["properties"]["op"]["const"] == "set_color_key").unwrap();
        assert_eq!(set["properties"]["value"]["required"], json!(["r", "g", "b", "a"]));
        assert_eq!(set["properties"]["value"]["additionalProperties"], false);
        for channel in ["r", "g", "b", "a"] {
            assert_eq!(set["properties"]["value"]["properties"][channel]["maximum"], 255);
        }
        let delete = operations.iter().find(|op| op["properties"]["op"]["const"] == "delete_color_key").unwrap();
        assert!(delete["properties"]["tick"].get("minimum").is_none());
        let offline = json_body(app().oneshot(request("/mcp", json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_project"}}))).await.unwrap()).await;
        assert_eq!(offline["id"], 2);
        assert_eq!(offline["result"]["isError"], true);
    }
    #[tokio::test]
    async fn tool_calls_reach_only_one_editor_and_results_return_as_mcp_content() {
        let app = app();
        let response = app
            .clone()
            .oneshot(request("/api/agent/poll", json!({"editorId":"editor-a"})))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            app.clone()
                .oneshot(request("/api/agent/poll", json!({"editorId":"editor-b"})))
                .await
                .unwrap()
                .status(),
            StatusCode::CONFLICT
        );
        let rpc_app = app.clone();
        let handle = tokio::spawn(async move {
            rpc_app.oneshot(request("/mcp", json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"get_project"}}))).await.unwrap()
        });
        let mut command = Value::Null;
        for _ in 0..100 {
            let body = json_body(
                app.clone()
                    .oneshot(request("/api/agent/poll", json!({"editorId":"editor-a"})))
                    .await
                    .unwrap(),
            )
            .await;
            if !body["command"].is_null() {
                command = body["command"].clone();
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        assert_eq!(command["tool"], "get_project");
        let repeated = json_body(
            app.clone()
                .oneshot(request("/api/agent/poll", json!({"editorId":"editor-a"})))
                .await
                .unwrap(),
        )
        .await;
        assert!(repeated["command"].is_null());
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "/api/agent/result",
                    json!({"editorId":"editor-b","id":command["id"],"result":{}})
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::CONFLICT
        );
        let completed = app
            .clone()
            .oneshot(request(
                "/api/agent/result",
                json!({"editorId":"editor-a","id":command["id"],"result":{"revision":42}}),
            ))
            .await
            .unwrap();
        assert_eq!(completed.status(), StatusCode::NO_CONTENT);
        let result = json_body(handle.await.unwrap()).await;
        assert_eq!(result["id"], 5);
        assert_eq!(result["result"]["isError"], false);
        assert!(result["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("42"));
        assert_eq!(
            app.oneshot(request(
                "/api/agent/result",
                json!({"editorId":"editor-a","id":command["id"],"result":{}})
            ))
            .await
            .unwrap()
            .status(),
            StatusCode::GONE
        );
    }
    #[tokio::test]
    async fn sessions_are_distinct_revocable_and_resources_are_discoverable() {
        let app = app();
        let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}});
        let first = app
            .clone()
            .oneshot(request("/mcp", initialize.clone()))
            .await
            .unwrap();
        let first_id = first.headers()["mcp-session-id"].clone();
        let second = app
            .clone()
            .oneshot(request("/mcp", initialize))
            .await
            .unwrap();
        assert_ne!(first_id, second.headers()["mcp-session-id"]);
        let mut resource = request(
            "/mcp",
            json!({"jsonrpc":"2.0","id":2,"method":"resources/list"}),
        );
        resource
            .headers_mut()
            .insert("mcp-session-id", first_id.clone());
        let body = json_body(app.clone().oneshot(resource).await.unwrap()).await;
        assert_eq!(body["result"]["resources"].as_array().unwrap().len(), 2);
        let mut delete = request("/mcp", json!({}));
        *delete.method_mut() = axum::http::Method::DELETE;
        delete
            .headers_mut()
            .insert("mcp-session-id", first_id.clone());
        assert_eq!(
            app.clone().oneshot(delete).await.unwrap().status(),
            StatusCode::NO_CONTENT
        );
        let mut revoked = request("/mcp", json!({"jsonrpc":"2.0","id":3,"method":"ping"}));
        revoked.headers_mut().insert("mcp-session-id", first_id);
        assert_eq!(
            app.oneshot(revoked).await.unwrap().status(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn abandoned_and_expired_commands_are_never_replayed() {
        let mut inbox = Inbox::default();
        let (response, _rx) = oneshot::channel();
        inbox.pending.insert(
            "expired".into(),
            Pending {
                command: json!({}),
                expires: Instant::now() - Duration::from_secs(1),
                response,
                claimed: false,
            },
        );
        inbox.queue.push_back("expired".into());
        cleanup(&mut inbox);
        assert!(inbox.pending.is_empty());
        assert!(inbox.queue.is_empty());
    }
}
