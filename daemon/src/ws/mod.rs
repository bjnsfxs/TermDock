pub mod events;
pub mod term;

use axum::{middleware, Router};

use crate::{auth, state::AppState};

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .merge(events::router())
        .merge(term::router())
        // Protect all WS endpoints (header OR ?token= query)
        .layer(middleware::from_fn_with_state(
            state,
            auth::require_bearer_header_or_query,
        ))
}
