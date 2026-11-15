use serde_json::{json, Value};
use video_kadr_backend::{
    domain::{
        filmcraft_generator_video::source_frame, filmcraft_matte_color::edit,
        filmcraft_project_file::SourceProjectDocument,
    },
    error::ApiJson,
    handlers::filmcraft_project::import,
};

#[tokio::test]
async fn matte_color_updates_shared_source_and_survives_import() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/filmcraft-video-source.json")).unwrap();
    let mut document = fixture["cases"][0]["document"].clone();
    document["project"]["items"]["1"]["kind"]["Media"]["media"] =
        json!({"Generator":{"ColorMatte":{"color":[1.0,0.0,0.0,1.0]}}});
    let result =
        edit(json!({"document":document,"color":"#00ff0080","context":{"projectSelection":["1"]}}))
            .unwrap();
    assert_eq!(result["result"], json!({"item":1,"color":"#00ff0080"}));
    assert_eq!(
        result["document"]["project"]["items"]["6"],
        document["project"]["items"]["6"]
    );
    let text = result["documentText"].as_str().unwrap();
    let source = SourceProjectDocument::read(text.as_bytes()).unwrap();
    assert_eq!(
        &source_frame(&source, 1, 0, 1.0, || false)
            .unwrap()
            .surface
            .rgba[..4],
        &[0, 255, 0, 128]
    );
    let request = serde_json::from_value(json!({"document":text})).unwrap();
    let imported = import(ApiJson(request)).await.unwrap().0;
    let item = imported["projectItems"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == "1")
        .unwrap();
    assert_eq!(item["matteColor"], "#00ff0080");
    assert!(edit(json!({"document":document,"item":"2","color":"#00ff00"})).is_err());
    assert!(edit(json!({"document":document,"item":"1","color":"green"})).is_err());
}
