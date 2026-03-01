pub mod config;
pub mod health;
pub mod instances;
pub mod output;
pub mod settings;

use axum::middleware;
use axum::Router;

use crate::{auth, state::AppState};

pub fn api_router(state: AppState) -> Router<AppState> {
    Router::new()
        .merge(instances::router())
        .merge(config::router())
        .merge(output::router())
        .merge(settings::router())
        // Protect all /api/v1 routes (REST header auth)
        .layer(middleware::from_fn_with_state(
            state,
            auth::require_bearer_header,
        ))
}
