pub mod config;
pub mod health;
pub mod instances;
pub mod output;
pub mod pairing;
pub mod settings;
pub mod system;

use axum::{middleware, Router};

use crate::{auth, state::AppState};

pub fn api_router(state: AppState) -> Router<AppState> {
    let regular_api = Router::new()
        .merge(instances::router())
        .merge(config::router())
        .merge(output::router())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_api_bearer_header,
        ));

    let master_api = Router::new()
        .merge(settings::router())
        .merge(system::router())
        .merge(pairing::master_router())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_master_bearer_header,
        ));

    let public_api = Router::new().merge(pairing::public_router());

    Router::new()
        .merge(public_api)
        .merge(regular_api)
        .merge(master_api)
}
