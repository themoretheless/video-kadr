mod support;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use video_kadr_backend::domain::{
    composition::CompositionTrack,
    concat_project_import::{import, ImportRequest},
    flat_surface,
    nested_composition::NestedCompositionProject,
};
fn font(bold: bool) -> Vec<u8> {
    let mut data = if bold {
        include_bytes!("../vendor/concat-text/fonts/Inter-Bold.ttf").to_vec()
    } else {
        include_bytes!("../vendor/concat-text/fonts/Inter-Regular.ttf").to_vec()
    };
    for (old, new) in [
        (b"Inter".to_vec(), b"KadrX".to_vec()),
        (
            "Inter".encode_utf16().flat_map(u16::to_be_bytes).collect(),
            "KadrX".encode_utf16().flat_map(u16::to_be_bytes).collect(),
        ),
    ] {
        let mut at = 0;
        while at + old.len() <= data.len() {
            if data[at..at + old.len()] == old {
                data[at..at + old.len()].copy_from_slice(&new);
                at += old.len();
            } else {
                at += 1;
            }
        }
    }
    data
}
fn source() -> Value {
    json!({"version":1,"media":[],"fonts":[{"family":"KadrX","path":"/never/open/KadrX.ttf"}],"activeTimelineId":"TL","timelines":[{"id":"TL","video":{"width":320,"height":180,"rateNum":10,"rateDen":1},"tracks":[{"id":"titles"}],"clips":[{"id":"title","trackId":"titles","kind":"text","start":0,"duration":2,"text":{"content":"Project fonts","fontFamily":"KadrX","fontSize":0.2,"fontWeight":400,"shadow":false}}]}]})
}
fn request(bold: bool) -> Value {
    json!({"schemaVersion":1,"document":source(),"mediaBindings":{},"fontBindings":{"/never/open/KadrX.ttf":{"data":STANDARD.encode(font(bold))}}})
}
fn convert(value: Value) -> video_kadr_backend::domain::concat_project_import::ImportReport {
    import(serde_json::from_value::<ImportRequest>(value).unwrap()).unwrap()
}
fn render(project: &NestedCompositionProject) -> Vec<u8> {
    project
        .render_surfaces(500000, |_, content, time| {
            Ok(flat_surface::render_generated(content, time, &mut || Ok(()))?.unwrap())
        })
        .unwrap()
        .rgba
}
#[test]
fn custom_font_bytes_are_owned_portable_scoped_and_do_not_open_original_paths() {
    let report = convert(request(false));
    assert_eq!(report.status, "converted", "{:?}", report.diagnostics);
    assert_eq!(report.source_document, source());
    let project = report.project.unwrap();
    assert_eq!(project.fonts.len(), 1);
    let regular = render(&project);
    let bold = convert(request(true)).project.unwrap();
    assert_ne!(regular, render(&bold));
    assert_eq!(render(&project), regular);
    let mut no_font = source();
    no_font["fonts"] = json!([]);
    let fallback = convert(json!({"schemaVersion":1,"document":no_font,"mediaBindings":{}}))
        .project
        .unwrap();
    assert_ne!(regular, render(&fallback));
    assert_eq!(render(&project), regular);
    let bytes = serde_json::to_vec(&project).unwrap();
    let restored: NestedCompositionProject = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(render(&restored), regular);
    let document = serde_json::to_value(&project).unwrap();
    let allowance = video_kadr_backend::db::composition_document_byte_limit(&document).unwrap();
    assert_eq!(
        allowance,
        video_kadr_backend::db::MAX_COMPOSITION_PROJECT_DOCUMENT_BYTES
            + serde_json::to_vec(&document["fonts"]).unwrap().len()
    );
    let inventory =
        video_kadr_backend::domain::font_inventory_status::project_fonts(&project).unwrap();
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].usage.family, "KadrX");
    assert_eq!(inventory[0].usage.style, "Regular");
    assert!(!inventory[0].missing);
    let a = project.clone();
    let b = bold.clone();
    let first = std::thread::spawn(move || render(&a));
    let second = std::thread::spawn(move || render(&b));
    assert_eq!(first.join().unwrap(), regular);
    assert_eq!(second.join().unwrap(), render(&bold));
}
#[test]
fn missing_malformed_or_wrong_family_font_bindings_refuse_without_partial_import() {
    for mode in 0..5 {
        let mut value = request(false);
        match mode {
            0 => value["fontBindings"] = json!({}),
            1 => value["fontBindings"]["/never/open/KadrX.ttf"]["data"] = json!("invalid!"),
            2 => {
                value["fontBindings"]["/never/open/KadrX.ttf"]["data"] =
                    json!(STANDARD.encode([0, 1, 2, 3]))
            }
            3 => value["document"]["fonts"][0]["family"] = json!("Wrong Family"),
            _ => {
                value["fontBindings"]["unknown-path"] = json!({"data":""});
            }
        }
        let original = value["document"].clone();
        let report = convert(value);
        assert!(report.project.is_none());
        assert_eq!(report.source_document, original);
    }
}
#[test]
fn replacing_retained_title_fonts_changes_real_typography_and_respects_locks() {
    use video_kadr_backend::domain::{
        composition::CompositionTrack, composition_font_replace::FontSelection, project_fonts,
    };
    let project = convert(request(false)).project.unwrap();
    let before = render(&project);
    let changed = project_fonts::replace_fonts(
        &project,
        &FontSelection::Family("KadrX".into()),
        &serde_json::from_value(json!({"family":"Hanken Grotesk","style":"Bold Italic"})).unwrap(),
    )
    .unwrap();
    assert_eq!(changed.replaced, 1);
    assert_ne!(render(&changed.project), before);
    assert_eq!(render(&project), before);
    let fonts = project_fonts::fonts_used(&changed.project).unwrap();
    assert_eq!(fonts[0].family, "Hanken Grotesk");
    assert_eq!(fonts[0].style, "Bold Italic");
    let mut locked = project.clone();
    if let CompositionTrack::Text { locked, .. } = &mut locked
        .compositions
        .get_mut(&project.root)
        .unwrap()
        .content
        .tracks[0]
    {
        *locked = true;
    }
    assert!(project_fonts::replace_fonts(
        &locked,
        &FontSelection::Family("KadrX".into()),
        &FontSelection::Family("Hanken Grotesk".into())
    )
    .is_err());
    assert_eq!(render(&locked), before);
}

#[tokio::test]
async fn editor_result_transport_accepts_resource_documents_without_expanding_rpc_requests() {
    use axum::{
        body::Body,
        extract::ConnectInfo,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    let token = "project-font-transport-01234567890123456789";
    let app = video_kadr_backend::mcp::router(Some(token.into()), vec![]);
    let body = json!({"id":"missing-request","result":"x".repeat(2*1024*1024+128)}).to_string();
    for (path, expected) in [
        ("/api/agent/result", StatusCode::CONFLICT),
        ("/mcp", StatusCode::PAYLOAD_TOO_LARGE),
    ] {
        let mut request = Request::builder()
            .method("POST")
            .uri(path)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.clone()))
            .unwrap();
        request.extensions_mut().insert(ConnectInfo(
            "127.0.0.1:4242".parse::<std::net::SocketAddr>().unwrap(),
        ));
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            expected
        );
    }
}

#[test]
fn font_payload_does_not_expand_the_source_document_metadata_budget() {
    let mut value = request(false);
    value["document"]["name"] = json!("x".repeat(4 * 1024 * 1024));
    assert!(import(serde_json::from_value::<ImportRequest>(value).unwrap()).is_err());
}

#[test]
fn explicit_missing_font_fallback_is_reported_and_does_not_leak_another_projects_face() {
    let owned = convert(request(false)).project.unwrap();
    let owned_frame = render(&owned);
    let mut value = request(false);
    value["fontBindings"]["/never/open/KadrX.ttf"] = json!({"fallback":true});
    let report = convert(value);
    assert_eq!(report.status, "converted");
    assert_eq!(report.diagnostics.len(), 1);
    assert!(report.diagnostics[0].contains("системная замена"));
    let project = report.project.unwrap();
    assert!(project.fonts.is_empty());
    assert_ne!(render(&project), owned_frame);
    let status =
        video_kadr_backend::domain::font_inventory_status::project_fonts(&project).unwrap();
    assert!(status[0].missing);
    assert_eq!(render(&owned), owned_frame);
    let mut ambiguous = request(false);
    ambiguous["fontBindings"]["/never/open/KadrX.ttf"]["fallback"] = json!(true);
    assert!(convert(ambiguous).project.is_none());
}

#[test]
fn font_status_does_not_hide_missing_faces_in_a_different_text_engine() {
    let mut project = convert(request(false)).project.unwrap();
    if let CompositionTrack::Text { clips, .. } = &mut project
        .compositions
        .get_mut(&project.root)
        .unwrap()
        .content
        .tracks[0]
    {
        clips[0].effects.push(serde_json::from_value(json!({"kind":"graphic_text","parameters":{"font":"KadrX","fontStyle":"Regular","text":"Other engine"}})).unwrap());
    }
    let status =
        video_kadr_backend::domain::font_inventory_status::project_fonts(&project).unwrap();
    let face = status
        .iter()
        .find(|status| status.usage.family == "KadrX" && status.usage.style == "Regular")
        .unwrap();
    assert_eq!(face.usage.count, 2);
    assert!(face.missing);
}

#[test]
fn adding_and_removing_font_files_preserves_title_references_and_deduplicates_bytes() {
    use video_kadr_backend::domain::concat_project_fonts::{edit, Edit};
    let original = convert(request(false)).project.unwrap();
    let owned = render(&original);
    let removed = edit(&original, Edit::Remove { index: 0 }).unwrap();
    assert!(removed.fonts.is_empty());
    assert_ne!(render(&removed), owned);
    assert_eq!(removed.compositions, original.compositions);
    let restored = edit(
        &removed,
        Edit::Add {
            data: STANDARD.encode(font(false)),
        },
    )
    .unwrap();
    assert_eq!(restored, original);
    assert_eq!(render(&restored), owned);
    assert_eq!(
        edit(
            &restored,
            Edit::Add {
                data: STANDARD.encode(font(false))
            }
        )
        .unwrap(),
        restored
    );
    assert!(edit(&original, Edit::Remove { index: 1 }).is_err());
    assert!(edit(
        &original,
        Edit::Add {
            data: STANDARD.encode([0, 1, 2])
        }
    )
    .is_err());
    assert_eq!(render(&original), owned);
}

#[tokio::test]
async fn font_file_http_route_adds_and_removes_validated_assets() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    let (state, _dir) = support::make_state(false, false).await;
    let app = support::router(state);
    let project = convert(request(false)).project.unwrap();
    let remove = Request::builder()
        .method("POST")
        .uri("/api/compositions/project-font-file")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"project":project,"operation":{"kind":"remove","index":0}}).to_string(),
        ))
        .unwrap();
    let (status, removed, _) = support::send(&app, remove).await;
    assert_eq!(status, StatusCode::OK);
    let removed: NestedCompositionProject = serde_json::from_value(removed).unwrap();
    assert!(removed.fonts.is_empty());
    let add=Request::builder().method("POST").uri("/api/compositions/project-font-file").header("content-type","application/json").body(Body::from(json!({"project":removed,"operation":{"kind":"add","data":STANDARD.encode(font(false))}}).to_string())).unwrap();
    let (status, added, _) = support::send(&app, add).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_value::<NestedCompositionProject>(added).unwrap(),
        project
    );
}
