//! Library crate for the video-kadr backend.
//!
//! `main.rs` is a thin wrapper around this crate. Exposing the modules and the
//! router builder as a library lets the integration tests in `tests/` drive the
//! real HTTP API with `tower::ServiceExt::oneshot`, without binding a socket.

pub mod analysis;
pub mod artifacts;
pub mod backup;
pub mod capabilities;
pub mod config;
pub mod db;
pub mod domain;
pub mod error;
pub mod handlers;
pub mod http;
pub mod ingest;
pub mod jobs;
pub mod library;
pub mod luts;
pub mod mcp;
pub mod messages;
pub mod model;
pub mod object_storage;
#[cfg(feature = "spec-contracts")]
pub mod packaging;
pub mod ports;
pub mod privacy;
pub mod process_control;
pub mod project_archive;
pub mod reliability;
#[cfg(feature = "spec-contracts")]
pub mod render;
pub mod runtime;
pub mod services;
pub mod state;
pub mod stock_catalog;
pub mod telemetry;
pub mod tools;
pub mod youtube;

use axum::extract::DefaultBodyLimit;
use std::sync::Arc;

use axum::http::{header, HeaderValue, Method};
use axum::routing::{delete, get, patch, post};
use axum::Router;
use tower_http::cors::{AllowOrigin, CorsLayer};

use config::CorsOrigins;
use http::ports::{
    RuntimeSystemPort, SqliteAuthPort, SqliteCompositionProjectPort, SqliteProjectPort,
    SqliteProjectReviewPort, SqliteSpacePort,
};
use state::AppState;

/// Build the application router. `max_upload` caps the `/api/upload` body size.
///
/// Static files are served only from `sources/`, `outputs/`, and the isolated
/// proxy media directory with HTTP range support. Proxy manifests and the
/// SQLite DB live outside these mounts and are intentionally unreachable.
pub fn build_router(state: AppState, max_upload: usize) -> Router {
    build_router_with_cors(state, max_upload, &CorsOrigins::default())
}

pub fn build_router_with_cors(
    state: AppState,
    max_upload: usize,
    cors_origins: &CorsOrigins,
) -> Router {
    build_router_with_mcp(state, max_upload, cors_origins, None)
}

pub fn build_router_with_mcp(
    state: AppState,
    max_upload: usize,
    cors_origins: &CorsOrigins,
    mcp_token: Option<String>,
) -> Router {
    let system_port = Arc::new(RuntimeSystemPort::new(state.tools.clone()));
    let auth_port = Arc::new(SqliteAuthPort::new(state.db.clone()));
    let project_port = Arc::new(SqliteProjectPort::new(state.db.clone()));
    let composition_project_port = Arc::new(SqliteCompositionProjectPort::new(
        state.db.clone(),
        state.library.clone(),
    ));
    let project_review_port = Arc::new(SqliteProjectReviewPort::new(state.db.clone()));
    let space_port = Arc::new(SqliteSpacePort::new(
        state.db.clone(),
        state.library.clone(),
    ));
    let source_files = Router::new()
        .route(
            "/files/sources/:filename",
            get(handlers::source_file_handler),
        )
        .route("/files/outputs/:filename", get(handlers::output_file_handler))
        .with_state(state.clone());
    let metrics = Router::new()
        .route("/metrics", get(handlers::metrics_handler))
        .with_state(state.clone());
    let core_api = Router::new()
        .route("/import", post(handlers::import_handler))
        .route(
            "/upload",
            post(handlers::upload_handler).layer(DefaultBodyLimit::max(max_upload)),
        )
        .route(
            "/luts",
            get(handlers::lut_list_handler)
                .post(handlers::lut_upload_handler)
                .layer(DefaultBodyLimit::max(handlers::MAX_LUT_BODY_BYTES)),
        )
        .route("/luts/:id", get(handlers::lut_get_handler))
        .route("/edit", post(handlers::edit_handler))
        .route("/composition-projects/:id/concat-video-cutout/smart-region/operations/:operation", get(handlers::concat_video_operation::lookup))
        .route("/composition-projects/:id/concat-video-cutout/smart-region/operations/:operation/cancel", post(handlers::concat_video_operation::cancel).layer(DefaultBodyLimit::max(1024)))
        .route("/composition-projects/:id/concat-video-cutout/smart-region/attach", post(handlers::concat_video_smart::attach).layer(DefaultBodyLimit::max(16 * 1024)))
        .route("/composition-projects/:id/concat-video-cutout/smart-region", post(handlers::concat_video_smart::start).layer(DefaultBodyLimit::max(handlers::concat_video_smart::MAX_START_BODY_BYTES)))
        .route("/composition-projects/:id/concat-video-cutout/binding", post(handlers::concat_video_binding::handler).layer(DefaultBodyLimit::max(handlers::concat_video_binding::MAX_BODY_BYTES)))
        .route("/composition-projects/:id/concat-video-cutout/preview", post(handlers::concat_video_preview::handler).layer(DefaultBodyLimit::max(handlers::concat_video_binding::MAX_BODY_BYTES)))
        .route("/composition-projects/:id/concat-video-cutout/cancel", post(handlers::concat_video_cancel::handler).layer(DefaultBodyLimit::max(16 * 1024)))
        .route("/composition-projects/:id/concat-video-cutout/analyse", post(handlers::concat_video_analyse::handler).layer(DefaultBodyLimit::max(16 * 1024)))
        .route("/composition-projects/:id/concat-cutout", post(handlers::concat_cutout_analyse::handler).layer(DefaultBodyLimit::max(4 * 1024 * 1024)))
        .route("/compositions/concat-project/export", post(handlers::concat_project_import::export_handler).layer(DefaultBodyLimit::max(32*1024*1024)))
        .route("/compositions/concat-project/import", post(handlers::concat_project_import::import_handler).layer(DefaultBodyLimit::max(4 * 1024 * 1024 + crate::domain::concat_project_fonts::MAX_FONT_JSON_BYTES + 1024*1024)))
        .route("/compositions/concat-effects/upgrade", post(handlers::concat_effect_upgrade::upgrade_handler).layer(DefaultBodyLimit::max(512 * 1024)))
        .route(
            "/compositions/render",
            post(handlers::composition_render_handler),
        )
        .route(
            "/compositions/motion-path",
            post(handlers::motion_path_handler),
        )
        .route("/composition-projects/:id/freeze-frame",post(handlers::concat_freeze::store).layer(DefaultBodyLimit::max(8*1024)))
        .route("/compositions/concat-retime-edit",post(handlers::nested_composition::concat_retime_edit_handler).layer(DefaultBodyLimit::max(512*1024)))
        .route(
            "/compositions/nested-move-clips",
            post(handlers::nested_composition::nested_move_clips_handler).layer(
                DefaultBodyLimit::max(db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024),
            ),
        )
        .route(
            "/speech/reference-clip-plan",
            post(handlers::speech::reference_clip_plan).layer(DefaultBodyLimit::max(
                db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024,
            )),
        )
        .route(
            "/speech/clip-plan",
            post(handlers::speech::clip_plan).layer(DefaultBodyLimit::max(
                db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024,
            )),
        )
        .route(
            "/speech/align",
            post(handlers::speech::align).layer(DefaultBodyLimit::max(
                db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 4 * 1024 * 1024,
            )),
        )
        .route("/speech/models", get(handlers::speech::models))
        .route(
            "/speech/narration-assets",
            axum::routing::put(handlers::tts_assets::store)
                .layer(DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .route(
            "/speech/tts-references/source",
            post(handlers::tts_reference_sources::create),
        )
        .route(
            "/speech/tts-references",
            post(handlers::tts_references::upload)
                .get(handlers::tts_references::list)
                .delete(handlers::tts_references::remove)
                .layer(DefaultBodyLimit::max(8 * 1024 * 1024)),
        )
        .route(
            "/cutout/jobs/:id/asset",
            post(handlers::cutout_assets::store).layer(DefaultBodyLimit::max(4096)),
        )
        .route("/cutout/status", get(handlers::cutout_jobs::status))
        .route(
            "/sam/jobs/:id/asset",
            post(handlers::sam_assets::store).layer(DefaultBodyLimit::max(4096)),
        )
        .route("/sam/status", get(handlers::sam_jobs::status))
        .route(
            "/sam/jobs",
            post(handlers::sam_jobs::submit)
                .get(handlers::sam_jobs::list)
                .layer(DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route("/sam/jobs/:id/mask", get(handlers::sam_jobs::mask))
        .route("/sam/jobs/:id/video", get(handlers::sam_jobs::video))
        .route(
            "/sam/jobs/:id/video-asset",
            post(handlers::sam_video_assets::store).layer(DefaultBodyLimit::max(4096)),
        )
        .route("/sam/jobs/:id/points", get(handlers::sam_jobs::points))
        .route("/selfie/status", get(handlers::selfie_jobs::status))
        .route(
            "/selfie/jobs",
            post(handlers::selfie_jobs::submit)
                .get(handlers::selfie_jobs::list)
                .layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/selfie/jobs/:id/original",
            get(handlers::selfie_jobs::original),
        )
        .route("/selfie/jobs/:id/mask", get(handlers::selfie_jobs::mask))
        .route("/selfie/jobs/:id/image", get(handlers::selfie_jobs::image))
        .route(
            "/selfie/jobs/:id/asset",
            post(handlers::cutout_assets::store).layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/selfie/jobs/:id/corrections",
            post(handlers::cutout_edit_jobs::submit).layer(DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route(
            "/cutout/jobs",
            post(handlers::cutout_jobs::submit)
                .get(handlers::cutout_jobs::list)
                .layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/composition-projects/:id/cutout/:clip",
            get(handlers::cutout_edit_jobs::project_settings),
        )
        .route(
            "/composition-projects/:id/cutout/:clip/original",
            get(handlers::cutout_edit_jobs::project_original),
        )
        .route(
            "/composition-projects/:id/cutout/corrections",
            post(handlers::cutout_edit_jobs::submit_project)
                .layer(DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route(
            "/cutout/jobs/:id/corrections",
            post(handlers::cutout_edit_jobs::submit).layer(DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route(
            "/cutout/jobs/:id/correction",
            get(handlers::cutout_edit_jobs::settings),
        )
        .route(
            "/cutout/jobs/:id/original",
            get(handlers::cutout_jobs::original),
        )
        .route("/cutout/jobs/:id/mask", get(handlers::cutout_jobs::mask))
        .route("/cutout/jobs/:id/image", get(handlers::cutout_jobs::image))
        .route("/enhance/status", get(handlers::enhance_jobs::status))
        .route("/cutout/models", get(handlers::cutout_models::list))
        .route(
            "/cutout/models/select",
            post(handlers::cutout_models::select).layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/cutout/models/remove",
            post(handlers::cutout_models::remove).layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/cutout/models/downloads",
            get(handlers::cutout_model_downloads::list)
                .post(handlers::cutout_model_downloads::submit)
                .layer(DefaultBodyLimit::max(4096)),
        )
        .route("/sam/models", get(handlers::sam_models::list))
        .route(
            "/sam/models/select",
            post(handlers::sam_models::select).layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/sam/models/remove",
            post(handlers::sam_models::remove).layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/sam/models/downloads",
            get(handlers::sam_model_downloads::list)
                .post(handlers::sam_model_downloads::submit)
                .layer(DefaultBodyLimit::max(4096)),
        )
        .route("/enhance/models", get(handlers::enhance_models::list))
        .route(
            "/enhance/models/select",
            post(handlers::enhance_models::select).layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/enhance/models/remove",
            post(handlers::enhance_models::remove).layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/enhance/models/downloads",
            get(handlers::enhance_model_downloads::list)
                .post(handlers::enhance_model_downloads::submit)
                .layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/composition-projects/:id/video-cutout/:clip",
            get(handlers::person_video_project::settings),
        )
        .route(
            "/composition-projects/:id/video-cutout/corrections",
            post(handlers::person_video_project::submit)
                .layer(DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route("/person/status", get(handlers::person_video_jobs::status))
        .route(
            "/person/video-jobs",
            post(handlers::person_video_jobs::submit)
                .get(handlers::person_video_jobs::list)
                .layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/person/video-jobs/:id/asset",
            post(handlers::person_video_assets::store).layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/person/video-jobs/:id/automatic-mask-video",
            get(handlers::person_video_jobs::automatic_video),
        )
        .route(
            "/person/video-jobs/:id/corrections",
            post(handlers::person_video_jobs::submit_correction)
                .get(handlers::person_video_jobs::correction_recipe)
                .layer(DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route(
            "/person/video-jobs/:id/masks/:frame",
            get(handlers::person_video_jobs::mask_frame),
        )
        .route(
            "/person/video-jobs/:id/source/:frame",
            get(handlers::person_video_jobs::source_frame),
        )
        .route(
            "/person/video-jobs/:id/regions",
            post(handlers::person_video_project::add_region),
        )
        .route(
            "/person/video-jobs/:id/regions/:region/:frame",
            get(handlers::person_video_region_frames::frame),
        )
        .route(
            "/person/video-jobs/:id/masks",
            get(handlers::person_video_jobs::mask_stream),
        )
        .route(
            "/person/video-jobs/:id/video",
            get(handlers::person_video_jobs::video),
        )
        .route(
            "/enhance/video-jobs",
            post(handlers::enhance_video_jobs::submit)
                .get(handlers::enhance_video_jobs::list)
                .layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/enhance/video-jobs/:id/asset",
            post(handlers::enhance_video_assets::store).layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/enhance/video-jobs/:id/video",
            get(handlers::enhance_video_jobs::video),
        )
        .route(
            "/enhance/jobs",
            post(handlers::enhance_jobs::submit)
                .get(handlers::enhance_jobs::list)
                .layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/enhance/jobs/:id/asset",
            post(handlers::enhance_assets::store).layer(DefaultBodyLimit::max(4096)),
        )
        .route(
            "/enhance/jobs/:id/image",
            get(handlers::enhance_jobs::image),
        )
        .route(
            "/speech/tts-jobs",
            get(handlers::tts_jobs::list)
                .post(handlers::tts_jobs::submit)
                .layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route("/speech/tts-jobs/:id/audio", get(handlers::tts_jobs::audio))
        .route(
            "/speech/tts-model-downloads",
            get(handlers::tts_model_downloads::list).post(handlers::tts_model_downloads::submit),
        )
        .route(
            "/speech/tts-models",
            get(handlers::tts_models::list).delete(handlers::tts_models::remove),
        )
        .route(
            "/speech/tts-models/selection",
            axum::routing::put(handlers::tts_models::select),
        )
        .route("/speech/voices", get(handlers::tts::voices))
        .route(
            "/speech/synthesize",
            post(handlers::tts::synthesize).layer(DefaultBodyLimit::max(16 * 1024)),
        )
        .route(
            "/speech/model-catalog",
            get(handlers::speech_model_downloads::catalog_handler),
        )
        .route(
            "/speech/model-downloads",
            post(handlers::speech_model_downloads::submit)
                .get(handlers::speech_model_downloads::list),
        )
        .route(
            "/speech/model-library",
            get(handlers::speech_models::list)
                .put(handlers::speech_models::import)
                .layer(DefaultBodyLimit::max(4_000_000_000usize)),
        )
        .route(
            "/speech/model-removal",
            post(handlers::speech_models::remove),
        )
        .route(
            "/speech/model-selection",
            post(handlers::speech_models::select),
        )
        .route(
            "/speech/jobs",
            post(handlers::speech_jobs::submit).get(handlers::speech_jobs::list),
        )
        .route("/speech/transcribe", post(handlers::speech::transcribe))
        .route(
            "/compositions/nested-frame",
            post(handlers::nested_composition::nested_frame_handler).layer(DefaultBodyLimit::max(
                db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024,
            )),
        )
        .route(
            "/compositions/nested-group",
            post(handlers::nested_composition::nested_group_handler).layer(DefaultBodyLimit::max(
                db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024,
            )),
        )
        .route(
            "/compositions/nested-split",
            post(handlers::nested_composition::nested_split_handler).layer(DefaultBodyLimit::max(
                db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024,
            )),
        )
        .route(
            "/compositions/nested-preview",
            post(handlers::nested_composition::nested_preview_handler).layer(
                DefaultBodyLimit::max(db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024),
            ),
        )
        .route(
            "/compositions/position-conversion",
            post(handlers::position_conversion::position_conversion_handler),
        )
        .route(
            "/compositions/nested-durations",
            post(handlers::nested_composition::nested_durations_handler).layer(
                DefaultBodyLimit::max(db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024),
            ),
        )
        .route(
            "/compositions/nested-export",
            post(handlers::nested_export::nested_export_handler).layer(DefaultBodyLimit::max(
                db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024,
            )),
        )
        .route(
            "/compositions/scopes-rgba",
            post(handlers::scopes::scopes_binary_handler)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/scopes",
            post(handlers::scopes::scopes_handler).layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/color-match",
            post(handlers::color_match::color_match_handler).layer(DefaultBodyLimit::max(
                db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024,
            )),
        )
        .route(
            "/compositions/lut-export",
            post(handlers::color_lut::export_lut_handler)
                .layer(DefaultBodyLimit::max(96 * 1024 * 1024)),
        )
        .route(
            "/compositions/project-lut-remove",
            post(handlers::color_lut::remove_project_lut_handler)
                .layer(DefaultBodyLimit::max(96 * 1024 * 1024)),
        )
        .route(
            "/compositions/project-lut-update",
            post(handlers::color_lut::update_project_lut_handler)
                .layer(DefaultBodyLimit::max(96 * 1024 * 1024)),
        )
        .route(
            "/compositions/project-lut-import",
            post(handlers::color_lut::import_project_lut_handler)
                .layer(DefaultBodyLimit::max(96 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/rate-stretch",
            post(handlers::filmcraft_project::stretch)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/roll",
            post(handlers::filmcraft_project::roll).layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/slide",
            post(handlers::filmcraft_project::slide).layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/copy",
            post(handlers::filmcraft_project::copy_clips)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/paste",
            post(handlers::filmcraft_project::paste_clips)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/trim-sessions/create",
            post(handlers::filmcraft_project::create_trim_session)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/trim-sessions/read",
            post(handlers::filmcraft_project::read_trim_session)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/trim-sessions/command",
            post(handlers::filmcraft_project::command_trim_session)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/trim-sessions/close",
            post(handlers::filmcraft_project::close_trim_session)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/trim-points",
            post(handlers::filmcraft_project::trim_points)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/to-playhead",
            post(handlers::filmcraft_project::to_playhead)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/nearest-edit-points",
            post(handlers::filmcraft_project::nearest_edit_points)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/analysis-jobs",
            post(handlers::stabilization_jobs::submit)
                .get(handlers::stabilization_jobs::list)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/video-jobs",
            post(handlers::filmcraft_video_jobs::submit).get(handlers::filmcraft_video_jobs::list).layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/export-presets",
            get(handlers::filmcraft_export_presets::list)
                .post(handlers::filmcraft_export_presets::command)
                .layer(DefaultBodyLimit::max(1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/quick-export",
            post(handlers::filmcraft_quick_export::submit)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/command",
            post(handlers::filmcraft_commands::execute)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/export-queue",
            get(handlers::filmcraft_export_queue::list)
                .post(handlers::filmcraft_export_queue::command)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/export-frame",
            post(handlers::filmcraft_project::export_frame)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/preview-frame",
            post(handlers::filmcraft_project::preview_frame)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/trim-monitor",
            post(handlers::filmcraft_project::trim_monitor)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/batch-transitions",
            post(handlers::filmcraft_project::batch_transitions)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/set-transition",
            post(handlers::filmcraft_project::set_transition)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/transitions",
            get(handlers::filmcraft_project::transition_catalog),
        )
        .route(
            "/compositions/filmcraft/apply-transition",
            post(handlers::filmcraft_project::apply_transition)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/add-transition",
            post(handlers::filmcraft_project::add_transition)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/place-clips",
            post(handlers::filmcraft_project::place_clips)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/remove-range",
            post(handlers::filmcraft_project::remove_range)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/razor",
            post(handlers::filmcraft_project::razor).layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/close-gap",
            post(handlers::filmcraft_project::close_gap)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/delete",
            post(handlers::filmcraft_project::delete)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/slip",
            post(handlers::filmcraft_project::slip).layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/trim",
            post(handlers::filmcraft_project::trim).layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route("/compositions/filmcraft/parameters",post(handlers::filmcraft_project::parameters).layer(DefaultBodyLimit::max(32 * 1024 * 1024)))
        .route("/compositions/filmcraft/edit-parameter",post(handlers::filmcraft_project::edit_parameter).layer(DefaultBodyLimit::max(32 * 1024 * 1024)))
        .route("/compositions/filmcraft/edit-mask",post(handlers::filmcraft_project::edit_mask).layer(DefaultBodyLimit::max(32 * 1024 * 1024)))
        .route("/compositions/filmcraft/track-mask",post(handlers::filmcraft_project::track_mask).layer(DefaultBodyLimit::max(32 * 1024 * 1024)))
        .route("/compositions/filmcraft/mask-monitor",post(handlers::filmcraft_project::mask_monitor).layer(DefaultBodyLimit::max(32 * 1024 * 1024)))
        .route(
            "/compositions/filmcraft/import",
            post(handlers::filmcraft_project::import)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/filmcraft/export-audio",
            post(handlers::filmcraft_project::export_audio)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/audio-dsp-catalog",
            get(handlers::audio_dsp::catalog_handler),
        )
        .route(
            "/compositions/audio-preview",
            post(handlers::composition_audio_preview::preview)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route(
            "/compositions/audio-project-response",
            post(handlers::audio_dsp::project_response_handler),
        )
        .route(
            "/compositions/builtin-luts",
            get(handlers::color_lut::builtin_luts_handler),
        )
        .route(
            "/compositions/color-lut",
            post(handlers::color_lut::color_lut_handler)
                .layer(DefaultBodyLimit::max(32 * 1024 * 1024)),
        )
        .route("/compositions/concat-curve",post(handlers::tone_curves::concat_curve_handler).layer(DefaultBodyLimit::max(16*1024)))
        .route(
            "/compositions/tone-curve",
            post(handlers::tone_curves::tone_curve_handler),
        )
        .route(
            "/compositions/captions/upgrade",
            post(handlers::composition_captions::upgrade),
        )
        .route(
            "/compositions/captions/edit",
            post(handlers::caption_edit::handle),
        )
        .route(
            "/compositions/captions/file/read",
            post(handlers::caption_files::read),
        )
        .route(
            "/compositions/captions/file/prepare-import",
            post(handlers::caption_file_import::handle),
        )
        .route(
            "/compositions/captions/file/write",
            post(handlers::caption_files::write),
        )
        .route(
            "/compositions/graphic-templates",
            get(handlers::graphic_templates::catalog),
        )
        .route(
            "/compositions/fonts-used",
            post(handlers::graphic_templates::fonts_used),
        )
        .route(
            "/compositions/replace-fonts",
            post(handlers::graphic_templates::replace_fonts),
        )
        .route("/compositions/project-font-file",post(handlers::graphic_templates::edit_project_font).layer(DefaultBodyLimit::max(db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + crate::domain::concat_project_fonts::MAX_FONT_JSON_BYTES + 64*1024)))
        .route(
            "/compositions/project-fonts-used",
            post(handlers::graphic_templates::project_fonts_used).layer(DefaultBodyLimit::max(db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024)),
        )
        .route(
            "/compositions/project-replace-fonts",
            post(handlers::graphic_templates::replace_project_fonts).layer(DefaultBodyLimit::max(db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024)),
        )
        .route(
            "/compositions/graphic-template-thumbnail",
            post(handlers::graphic_templates::thumbnail),
        )
        .route(
            "/compositions/graphic-template-values",
            post(handlers::graphic_templates::values),
        )
        .route(
            "/compositions/graphic-template-edit",
            post(handlers::graphic_templates::edit),
        )
        .route(
            "/compositions/graphic-template-edit-batch",
            post(handlers::graphic_templates::edit_batch),
        )
        .route(
            "/compositions/graphic-template-file/export",
            post(handlers::graphic_templates::export_file),
        )
        .route(
            "/compositions/graphic-template-file/instantiate",
            post(handlers::graphic_templates::instantiate_file),
        )
        .route(
            "/compositions/graphic-template-file/read",
            post(handlers::graphic_templates::read_file),
        )
        .route(
            "/compositions/graphic-template-file/write",
            post(handlers::graphic_templates::write_file),
        )
        .route(
            "/compositions/graphic-template-library",
            get(handlers::graphic_template_library::list),
        )
        .route(
            "/compositions/graphic-template-library/install",
            post(handlers::graphic_template_library::install),
        )
        .route(
            "/compositions/graphic-template-library/remove",
            post(handlers::graphic_template_library::remove),
        )
        .route(
            "/compositions/graphic-clock",
            post(handlers::graphic_clock::map_clock),
        )
        .route(
            "/compositions/graphic-pin-capture",
            post(handlers::graphic_pin::capture),
        )
        .route(
            "/compositions/graphic-text-edit",
            post(handlers::graphic_text_edit::graphic_text_edit_handler),
        )
        .route(
            "/compositions/animated-color",
            post(handlers::animated_color::animated_color_handler),
        )
        .route("/jobs/failed", get(handlers::failed_jobs_handler))
        .route("/jobs/registry", get(handlers::job_registry_handler))
        .route("/jobs/:id", get(handlers::job_status_handler))
        .route("/jobs/:id/cancel", post(handlers::cancel_handler))
        .route("/jobs/:id/retry", post(handlers::retry_job_handler))
        .route("/jobs/:id/discard", post(handlers::discard_job_handler))
        .route("/library/:id/insta360", get(handlers::insta360::metadata))
        .route("/library/:id/insta360/gyro", post(handlers::insta360::gyro))
        .route("/library/:id/insta360/profile", get(handlers::insta360::profile))
        .route("/composition-projects/:id/spherical-mask-track", post(handlers::spherical_tracking::handler).layer(DefaultBodyLimit::max(db::MAX_EXTENDED_COMPOSITION_PROJECT_DOCUMENT_BYTES + 64 * 1024)))
        .route("/library", get(handlers::library_list_handler))
        .route("/library/search", get(handlers::library_search_handler))
        .route("/stock/search", get(handlers::stock_search_handler))
        .route(
            "/publish/youtube/status",
            get(handlers::youtube_status_handler),
        )
        .route(
            "/publish/youtube/connect",
            post(handlers::youtube_connect_handler).delete(handlers::youtube_disconnect_handler),
        )
        .route(
            "/publish/youtube/callback",
            get(handlers::youtube_callback_handler),
        )
        .route("/publish/youtube", post(handlers::youtube_publish_handler))
        .route(
            "/library/:id/thumbnail",
            get(handlers::library_thumbnail_handler),
        )
        .route(
            "/library/:id/thumbnail/:key",
            get(handlers::library_thumbnail_version_handler),
        )
        .route(
            "/library/:id/filmstrip",
            get(handlers::library_filmstrip_handler),
        )
        .route(
            "/library/:id/filmstrip/:key",
            get(handlers::library_filmstrip_version_handler),
        )
        .route("/library/:id", delete(handlers::library_delete_handler))
        .route(
            "/library/:id/shots",
            post(handlers::library_shot_scan_handler),
        )
        .route(
            "/library/:id/shots/measure",
            post(handlers::library_shot_measure_handler),
        )
        .route(
            "/library/:id/compare-frames",
            post(handlers::library_compare_frames_handler),
        )
        .route(
            "/library/:id/analysis-frame",
            get(handlers::library_analysis_frame_handler),
        )
        .route("/library/:id/audio-streams", get(handlers::library_audio_streams_handler))
        .route(
            "/library/:id/cropdetect",
            post(handlers::library_cropdetect_handler),
        )
        .route(
            "/library/:id/proxies",
            get(handlers::proxy_list_handler).post(handlers::proxy_create_handler),
        )
        .route(
            "/library/:id/proxies/:key",
            delete(handlers::proxy_delete_handler),
        )
        .route(
            "/library/:id/proxies/:key/content",
            get(handlers::proxy_content_handler),
        )
        .route(
            "/library/:id/metadata",
            patch(handlers::library_metadata_patch_handler)
                .put(handlers::library_metadata_put_handler),
        )
        .route(
            "/composition-projects/import",
            post(handlers::composition_project_archive_import_handler).layer(
                DefaultBodyLimit::max(project_archive::MAX_ARCHIVE_MULTIPART_BYTES),
            ),
        )
        .route(
            "/composition-projects/:id/archive",
            get(handlers::composition_project_archive_export_handler),
        )
        .with_state(state);
    let api = core_api
        .merge(http::system_router(system_port))
        .merge(http::auth_router(auth_port))
        .merge(http::project_router(project_port))
        .merge(http::composition_project_router(composition_project_port))
        .merge(http::project_review_router(project_review_port))
        .merge(http::space_router(space_port))
        .fallback(handlers::api_not_found_handler)
        .method_not_allowed_fallback(handlers::method_not_allowed_handler);

    let router = Router::new()
        .merge(mcp::router(
            mcp_token,
            cors_origins.iter().map(str::to_owned).collect(),
        ))
        .merge(metrics)
        .merge(source_files)
        .nest("/api", api);
    http::policy::apply_public_layers(router, cors_layer(cors_origins))
}

fn cors_layer(origins: &CorsOrigins) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(cors_header_values(origins)))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            telemetry::REQUEST_ID_HEADER,
            header::HeaderName::from_static("x-space-id"),
            header::HeaderName::from_static("x-frame-width"),
            header::HeaderName::from_static("x-frame-height"),
            header::HeaderName::from_static("x-scope-options"),
        ])
        .expose_headers([telemetry::REQUEST_ID_HEADER, header::CONTENT_DISPOSITION])
}

fn cors_header_values(origins: &CorsOrigins) -> Vec<HeaderValue> {
    origins
        .iter()
        .map(|origin| {
            origin
                .parse()
                .expect("CorsOrigins guarantees valid ASCII HTTP origins")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cors_origins_use_local_dev_defaults() {
        let origins = cors_header_values(&CorsOrigins::default());
        assert!(origins.contains(&HeaderValue::from_static("http://localhost:5173")));
        assert!(origins.contains(&HeaderValue::from_static("http://127.0.0.1:8088")));
        assert!(!origins.contains(&HeaderValue::from_static("*")));
    }
}
