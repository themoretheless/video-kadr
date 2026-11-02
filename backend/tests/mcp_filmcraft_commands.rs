mod support;
use axum::{
    body::{to_bytes, Body},
    extract::ConnectInfo,
    http::Request,
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
const TOKEN: &str = "filmcraft-command-mcp-token-0123456789";
fn request(path: &str, body: Value) -> Request<Body> {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("authorization", format!("Bearer {TOKEN}"))
        .header("host", "localhost:8080")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:43210".parse::<std::net::SocketAddr>().unwrap(),
    ));
    request
}
async fn body(response: axum::response::Response) -> Value {
    assert_eq!(response.status(), 200);
    serde_json::from_slice(
        &to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}
async fn native(app: &Router, owner: Option<&str>, command: &str, params: Value) -> (u16, Value) {
    native_deadlined(app, owner, command, params, None).await
}
async fn native_deadlined(
    app: &Router,
    owner: Option<&str>,
    command: &str,
    params: Value,
    deadline: Option<u64>,
) -> (u16, Value) {
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/compositions/filmcraft/command")
        .header("content-type", "application/json");
    if let Some(owner) = owner {
        request = request.header("authorization", format!("Bearer {owner}"));
    }
    let response = app
        .clone()
        .oneshot(
            request
                .body(Body::from(
                    json!({"command":command,"params":params,"deadlineUnixMs":deadline})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    (
        response.status().as_u16(),
        serde_json::from_slice(
            &to_bytes(response.into_body(), 2 * 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap(),
    )
}
async fn forward(
    bridge: &Router,
    native_app: &Router,
    owner: Option<&str>,
    tool: &str,
    args: Value,
) -> Value {
    let rpc_app = bridge.clone();
    let tool = tool.to_owned();
    let pending = tokio::spawn(async move {
        rpc_app.oneshot(request("/mcp",json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":tool,"arguments":args}}))).await.unwrap()
    });
    let command = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let poll = body(
                bridge
                    .clone()
                    .oneshot(request("/api/agent/poll", json!({"editorId":"editor"})))
                    .await
                    .unwrap(),
            )
            .await;
            if poll["command"].is_object() {
                break poll["command"].clone();
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    let (id, params) = if command["tool"] == "command_list" {
        ("command.list", command["args"].clone())
    } else if command["tool"] == "command_batch" {
        ("command.batch", command["args"].clone())
    } else {
        (
            command["args"]["id"].as_str().unwrap(),
            command["args"].get("params").cloned().unwrap_or(json!({})),
        )
    };
    let (status, result) =
        native_deadlined(native_app, owner, id, params, command["expiresAt"].as_u64()).await;
    let mut ack = json!({"editorId":"editor","id":command["id"]});
    if status == 200 {
        ack["result"] = result;
    } else {
        ack["error"] = result["error"].clone();
    }
    assert_eq!(
        bridge
            .clone()
            .oneshot(request("/api/agent/result", ack))
            .await
            .unwrap()
            .status(),
        204
    );
    body(pending.await.unwrap()).await
}
#[tokio::test]
async fn source_discovery_and_run_cross_the_claimed_bridge_to_owner_native_services() {
    let (state, _dir) = support::make_state(false, false).await;
    let owner = state
        .db
        .register_auth_user("mcp-command-owner", "test password")
        .await
        .unwrap()
        .unwrap();
    let native_app = support::router(state.clone());
    let bridge = video_kadr_backend::mcp::router(Some(TOKEN.into()), vec![]);
    let initialized = body(
        bridge
            .clone()
            .oneshot(request(
                "/mcp",
                json!({"jsonrpc":"2.0","id":0,"method":"initialize"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert!(initialized["result"]["instructions"]
        .as_str()
        .unwrap()
        .contains("254016000000 ticks/second"));
    let tools = body(
        bridge
            .clone()
            .oneshot(request(
                "/mcp",
                json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    let tools = tools["result"]["tools"].as_array().unwrap();
    let batch_tool = tools
        .iter()
        .find(|tool| tool["name"] == "command_batch")
        .unwrap();
    assert_eq!(batch_tool["inputSchema"]["required"], json!(["steps"]));
    assert_eq!(
        batch_tool["inputSchema"]["properties"]["steps"]["items"]["required"],
        json!(["id"])
    );
    let list = tools
        .iter()
        .find(|tool| tool["name"] == "command_list")
        .unwrap();
    assert_eq!(list["annotations"]["readOnlyHint"], true);
    assert_eq!(list["inputSchema"]["additionalProperties"], false);
    let run = tools
        .iter()
        .find(|tool| tool["name"] == "command_run")
        .unwrap();
    assert_eq!(run["inputSchema"]["required"], json!(["id"]));
    assert!(run["description"]
        .as_str()
        .unwrap()
        .contains("outside composition Undo"));
    body(
        bridge
            .clone()
            .oneshot(request("/api/agent/poll", json!({"editorId":"editor"})))
            .await
            .unwrap(),
    )
    .await;
    let discovered = forward(
        &bridge,
        &native_app,
        Some(&owner.token),
        "command_list",
        json!({"filter":"PRESETS","enabled_only":true}),
    )
    .await;
    assert_eq!(discovered["result"]["isError"], false);
    let commands: Value =
        serde_json::from_str(discovered["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(commands.as_array().unwrap().len(), 7);
    assert!(commands
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["enabled"] == true && item["partial"] == true));
    let all = native(&native_app, Some(&owner.token), "command.list", json!({})).await;
    assert_eq!(all.0, 200);
    assert_eq!(all.1.as_array().unwrap().len(), 47);
    assert!(all.1.as_array().unwrap().iter().any(|row| row["id"] == "clip.rename"));
    assert_eq!(
        native(
            &native_app,
            Some(&owner.token),
            "command.list",
            json!({"filtr":"queue"})
        )
        .await
        .0,
        400
    );
    let formats = forward(
        &bridge,
        &native_app,
        Some(&owner.token),
        "command_run",
        json!({"id":"export.formats"}),
    )
    .await;
    assert_eq!(formats["result"]["isError"], false);
    let formats: Value =
        serde_json::from_str(formats["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(formats.as_array().unwrap().len(), 14);
    assert_eq!(
        formats
            .as_array()
            .unwrap()
            .iter()
            .filter(|format| format["available"] == true)
            .count(),
        if cfg!(target_os = "macos") { 14 } else { 13 }
    );
    let effects=forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":"effects.list","params":{"kind":"AudioTransition","detail":true}})).await;
    assert_eq!(effects["result"]["isError"],false,"{effects}");
    let effects:Value=serde_json::from_str(effects["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(effects.as_array().unwrap().len(),3);assert!(effects[0]["paramInfo"].is_array());
    let rack_oracle:Value=serde_json::from_str(include_str!("fixtures/filmcraft-effect-edit-oracle.json")).unwrap();
    let mut rack_document=json!({"format":"filmcraft.project","schema_version":12,"project":rack_oracle["project"]});
    rack_document["project"]["items"][rack_oracle["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"][0]["items"][0]["effects"]=rack_oracle["cases"][0]["before"].clone();
    let reset=forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":"effects.reset","params":{"document":serde_json::to_string(&rack_document).unwrap(),"sequence":rack_oracle["sequence"],"clip":rack_oracle["clip"],"index":0}})).await;
    assert_eq!(reset["result"]["isError"],false,"{reset}");
    let reset:Value=serde_json::from_str(reset["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(reset["documentText"].is_string());
    let applied=forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":"effects.apply","params":{"document":serde_json::to_string(&rack_document).unwrap(),"sequence":rack_oracle["sequence"],"clip":rack_oracle["clip"],"effect":"Gaussian Blur"}})).await;
    assert_eq!(applied["result"]["isError"],false,"{applied}");
    let set_param=forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":"effects.setParam","params":{"document":serde_json::to_string(&rack_document).unwrap(),"sequence":rack_oracle["sequence"],"clip":rack_oracle["clip"],"effect":"motion","param":"scale","value":75.}})).await;
    assert_eq!(set_param["result"]["isError"],false,"{set_param}");
    let key_oracle: Value=serde_json::from_str(include_str!("fixtures/filmcraft-keyframe-edit-oracle.json")).unwrap();
    for command in ["effects.deleteKeyframe","effects.moveKeyframe","effects.setInterpolation","effects.setKeyframe"] {
        let case=key_oracle["cases"].as_array().unwrap().iter().find(|c|c["command"]==command&&c["ok"]==true).unwrap();
        let mut document=json!({"format":"filmcraft.project","schema_version":12,"project":key_oracle["project"]});
        document["project"]["items"][key_oracle["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"][0]["items"][0]["effects"]=json!([key_oracle["instances"][case["instance"].as_u64().unwrap() as usize]]);
        let mut params=case["params"].clone();params["sequence"]=key_oracle["sequence"].clone();params["document"]=json!(serde_json::to_string(&document).unwrap());
        let response=forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":command,"params":params})).await;
        assert_eq!(response["result"]["isError"],false,"{command}: {response}");
    }
    let animation: Value=serde_json::from_str(include_str!("fixtures/filmcraft-animation-oracle.json")).unwrap();
    for command in ["effects.toggleAnimation","effects.addKeyframe"] {
        let case=animation["cases"].as_array().unwrap().iter().find(|c|c["command"]==command&&c["ok"]==true).unwrap();
        let mut document=json!({"format":"filmcraft.project","schema_version":12,"project":animation["project"]});
        let item=&mut document["project"]["items"][animation["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"][0]["items"][0];
        for(k,v)in case["clipChanges"].as_object().unwrap(){item[k]=v.clone();}
        for k in case["clipRemovals"].as_array().unwrap(){item.as_object_mut().unwrap().remove(k.as_str().unwrap());}
        item["effects"]=json!([animation["instances"][case["instance"].as_u64().unwrap() as usize]]);
        let mut params=case["params"].clone();params["sequence"]=animation["sequence"].clone();params["context"]=json!({"playhead":case["playhead"]});params["document"]=json!(serde_json::to_string(&document).unwrap());
        let response=forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":command,"params":params})).await;
        assert_eq!(response["result"]["isError"],false,"{command}: {response}");
    }
    let mask_oracle: Value=serde_json::from_str(include_str!("fixtures/filmcraft-mask-command-oracle.json")).unwrap();
    for command in ["masks.add","masks.remove","masks.set","masks.moveVertex","masks.translate","masks.addVertex","masks.removeVertex","masks.toggleVertexSmooth","masks.list","masks.select"] {
        let case=mask_oracle["cases"].as_array().unwrap().iter().find(|c|c["command"]==command&&c["ok"]==true).unwrap();
        let mut document=json!({"format":"filmcraft.project","schema_version":12,"project":mask_oracle["project"]});
        document["project"]["items"][mask_oracle["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"][0]["items"][0]["effects"]=json!([mask_oracle["instances"][case["instance"].as_u64().unwrap() as usize]]);
        let mut params=case["params"].clone();params["sequence"]=mask_oracle["sequence"].clone();params["context"]=case["context"].clone();params["document"]=json!(serde_json::to_string(&document).unwrap());
        let response=forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":command,"params":params})).await;
        assert_eq!(response["result"]["isError"],false,"{command}: {response}");
    }
    let tracking_oracle:Value=serde_json::from_str(include_str!("fixtures/filmcraft-mask-track-batch-oracle.json")).unwrap();
    let mut document=json!({"format":"filmcraft.project","schema_version":12,"project":tracking_oracle["project"]});
    document["project"]["items"][tracking_oracle["sequence"].to_string()]["kind"]["Sequence"]["video_tracks"][0]["items"][0]=tracking_oracle["cases"][0]["clip"].clone();
    document["project"]["items"][tracking_oracle["source"].to_string()]["kind"]["Media"]["media"]=json!({"Generator":{"ColorMatte":{"color":[0.2,0.4,0.8,1.]}}});
    let tracked=forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":"masks.track","params":{"document":serde_json::to_string(&document).unwrap(),"sequence":tracking_oracle["sequence"],"clip":tracking_oracle["cases"][0]["clip"]["id"],"effect":"opacity","mask":0,"frames":3,"wait":true}})).await;
    assert_eq!(tracked["result"]["isError"],false,"{tracked}");let tracked:Value=serde_json::from_str(tracked["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    for command in ["jobs.list","jobs.cancel"]{let result=forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":command,"params":{"job":tracked["job"],"snapshot":true}})).await;assert_eq!(result["result"]["isError"],false,"{command}: {result}");}
    let saved = forward(
        &bridge,
        &native_app,
        Some(&owner.token),
        "command_run",
        json!({"id":"export.presets.save","params":{"name":"Via MCP","format":"wav"}}),
    )
    .await;
    assert_eq!(saved["result"]["isError"], false);
    assert_eq!(
        native(
            &native_app,
            Some(&owner.token),
            "export.presets.get",
            json!({"name":"Via MCP"})
        )
        .await
        .0,
        200
    );
    let refused = forward(
        &bridge,
        &native_app,
        None,
        "command_run",
        json!({"id":"export.presets.delete","params":{"name":"Via MCP"}}),
    )
    .await;
    assert_eq!(refused["result"]["isError"], true);
    assert_eq!(
        native(
            &native_app,
            Some(&owner.token),
            "export.presets.get",
            json!({"name":"Via MCP"})
        )
        .await
        .0,
        200
    );
    let batch = forward(&bridge, &native_app, Some(&owner.token), "command_batch", json!({"steps":[{"id":"export.presets.save","params":{"name":"Batch first","format":"wav"}},{"id":"unknown.command"},{"id":"export.presets.save","params":{"name":"Batch after","format":"wav"}}]})).await;
    assert_eq!(batch["result"]["isError"], true);
    let batch: Value =
        serde_json::from_str(batch["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(batch["completed"], 1);
    assert_eq!(batch["failed"], 1);
    assert_eq!(batch["results"].as_array().unwrap().len(), 2);
    assert_eq!(
        native(
            &native_app,
            Some(&owner.token),
            "export.presets.get",
            json!({"name":"Batch first"})
        )
        .await
        .0,
        200
    );
    assert_eq!(
        native(
            &native_app,
            Some(&owner.token),
            "export.presets.get",
            json!({"name":"Batch after"})
        )
        .await
        .0,
        400
    );
    let continued = forward(&bridge, &native_app, Some(&owner.token), "command_batch", json!({"stop_on_error":false,"steps":[{"id":"unknown.command"},{"id":"export.presets.save","params":{"name":"Batch after","format":"wav"}}]})).await;
    assert_eq!(continued["result"]["isError"], true);
    let continued: Value =
        serde_json::from_str(continued["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(continued["completed"], 1);
    assert_eq!(continued["failed"], 1);
    assert_eq!(
        native(
            &native_app,
            Some(&owner.token),
            "export.presets.get",
            json!({"name":"Batch after"})
        )
        .await
        .0,
        200
    );
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-source-masks.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    let sequence = &mut document["project"]["items"]["6"]["kind"]["Sequence"];
    sequence["settings"]["frame_rate"] = json!({"num":24,"den":1});
    sequence["settings"]["width"] = json!(64);
    sequence["settings"]["height"] = json!(48);
    for tracks in [
        "video_tracks",
        "audio_tracks",
        "submix_tracks",
        "caption_tracks",
    ] {
        sequence[tracks] = json!([]);
    }
    let document = serde_json::to_string(&document).unwrap();
    let inspected=forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":"project.inspect","params":{"document":document}})).await;
    assert_eq!(inspected["result"]["isError"],false,"{inspected}");
    let inspected:Value=serde_json::from_str(inspected["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inspected["name"],"Linked trim");
    let sequence=forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":"sequence.inspect","params":{"document":document,"item":"6"}})).await;
    assert_eq!(sequence["result"]["isError"],false,"{sequence}");
    let sequence:Value=serde_json::from_str(sequence["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(sequence["id"],6);assert_eq!(sequence["duration"],0);
    let source=forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":"source.inspect","params":{"document":document,"context":{"sourceItem":"1","sourcePlayhead":"17"}}})).await;
    assert_eq!(source["result"]["isError"],false,"{source}");
    let source:Value=serde_json::from_str(source["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(source["item"],1);assert_eq!(source["playhead"],17);


    let exported = forward(&bridge, &native_app, Some(&owner.token), "command_run", json!({"id":"export.queue.add","params":{"document":document,"sequence":6,"sources":{},"preset":"Via MCP","range":{"startTicks":"0","endTicks":"21168000000"},"start":true,"wait":true}})).await;
    assert_eq!(exported["result"]["isError"], false, "{exported}");
    let exported: Value =
        serde_json::from_str(exported["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(exported["items"][0]["status"], "done");
    let job = state
        .get_job(exported["items"][0]["jobId"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    let result = job.result.unwrap();
    let file = state
        .outputs_dir()
        .join(result["filename"].as_str().unwrap());
    let probe = tokio::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(file)
        .output()
        .await
        .unwrap();
    assert!(probe.status.success());
    let probe: Value = serde_json::from_slice(&probe.stdout).unwrap();
    assert_eq!(probe["streams"][0]["duration_ts"], 4000);
    let quick = forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":"export.quick","params":{"document":document,"sequence":6,"preset":"Via MCP","range":{"startFrame":0,"endFrame":2},"wait":true}})).await;
    assert_eq!(quick["result"]["isError"], false, "{quick}");
    let quick: Value =
        serde_json::from_str(quick["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(quick["status"], "done");
    assert_eq!(quick["preset"], "Via MCP");
    assert_eq!(quick["job"], quick["jobId"]);
    assert!((quick["result"]["duration"].as_f64().unwrap() - 4000.0 / 48000.0).abs() < 1e-12);
    let listed=forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":"jobs.list","params":{}})).await;assert_eq!(listed["result"]["isError"],false,"{listed}");let listed:Value=serde_json::from_str(listed["result"]["content"][0]["text"].as_str().unwrap()).unwrap();assert!(listed.as_array().unwrap().iter().any(|row|row["id"]==quick["job"]&&row["kind"]=="export"&&row["finished"]==true));
    let cancelled=forward(&bridge,&native_app,Some(&owner.token),"command_run",json!({"id":"jobs.cancel","params":{"job":quick["job"]}})).await;assert_eq!(cancelled["result"]["isError"],false,"{cancelled}");
    let outsider=state.db.register_auth_user("export-job-outsider","test password").await.unwrap().unwrap();assert_eq!(native(&native_app,Some(&outsider.token),"jobs.cancel",json!({"job":quick["job"]})).await.0,404);
    state.begin_shutdown();
    assert!(
        state
            .wait_for_tasks(std::time::Duration::from_secs(5))
            .await
    );
}
