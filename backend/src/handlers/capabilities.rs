use axum::extract::State;
use axum::Json;

use crate::capabilities::Capabilities;
use crate::state::AppState;

pub async fn capabilities_handler(State(state): State<AppState>) -> Json<Capabilities> {
    Json(Capabilities::from_tools(&state.tools))
}
