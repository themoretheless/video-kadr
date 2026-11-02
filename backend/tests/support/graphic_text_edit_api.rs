use super::*;
#[tokio::test]
async fn original_character_editing_http_commands() {
    let (state, _dir) = make_state(true, true).await;
    let app = router(state);
    let f: Value = serde_json::from_str(include_str!(
        "../fixtures/filmcraft-graphic-text-editing.json"
    ))
    .unwrap();
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 192);
    for (i, c) in cases.iter().enumerate() {
        let kind = c["operation"].as_str().unwrap();
        let operation = match kind {
            "apply" => json!({"kind":kind,"start":c["start"],"end":c["end"],"style":c["style"]}),
            "clear" => json!({"kind":kind,"start":c["start"],"end":c["end"]}),
            "adjust" => json!({"kind":kind,"newText":c["newText"]}),
            _ => json!({"kind":kind}),
        };
        let response = app
            .clone()
            .oneshot(post_json(
                "/api/compositions/graphic-text-edit",
                json!({"text":c["text"],"runs":c["runs"],"operation":operation}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "case{i}");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        let actual: Vec<video_kadr_backend::domain::graphic_text_runs::CharacterStyleRun> =
            serde_json::from_value(body["runs"].clone()).unwrap();
        let expected: Vec<video_kadr_backend::domain::graphic_text_runs::CharacterStyleRun> =
            serde_json::from_value(c["expected"].clone()).unwrap();
        assert_eq!(actual, expected, "case{i}");
    }
    for operation in [
        json!({"kind":"apply","start":0,"end":1,"style":{"fill":[1.,0.,0.,2.]}}),
        json!({"kind":"apply","start":-1,"end":1,"style":{}}),
        json!({"kind":"apply","start":0,"end":1,"style":{"unknown":true}}),
    ] {
        let response = app
            .clone()
            .oneshot(post_json(
                "/api/compositions/graphic-text-edit",
                json!({"text":"Б🙂","runs":[],"operation":operation}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
