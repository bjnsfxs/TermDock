mod auth;
mod config;
mod db;
mod error;
mod models;
mod routes;
mod state;
mod ws;

// process module is stubbed in this skeleton. Implement per Plan.md milestones.
mod process;

use axum::{
    http::{header, Method},
    routing::get,
    Router,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::time::{Duration, MissedTickBehavior};
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{config::DaemonConfig, state::AppState};

#[tokio::main]
async fn main() -> Result<(), crate::error::AppError> {
    // Logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // default to info; override with RUST_LOG=debug
                "ai_cli_manager_daemon=info,tower_http=info".into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load or create config + data directory
    let cfg = DaemonConfig::load_or_create()?;
    tracing::info!(
        bind_address = %cfg.bind_address,
        port = cfg.port,
        data_dir = %cfg.data_dir.display(),
        web_dir = %cfg.web_dir.display(),
        "config loaded"
    );

    // Init DB
    let pool = db::init_sqlite(&cfg.data_dir).await?;

    // Init state
    let state = AppState::new(cfg, pool);
    {
        let process = state.process.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(1));
            ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                process.sample_metrics_once().await;
            }
        });
    }

    let app = build_app(state.clone());

    // Bind
    let cfg = state.config_read();
    let addr: SocketAddr = format!("{}:{}", cfg.bind_address, cfg.port)
        .parse()
        .map_err(|e| crate::error::AppError::internal(format!("invalid bind addr: {e}")))?;

    tracing::info!(%addr, "listening");
    tracing::info!(ui_url = %format!("http://{addr}/"), "web UI endpoint");

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| crate::error::AppError::internal(format!("bind failed: {e}")))?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(|e| crate::error::AppError::internal(format!("server error: {e}")))?;

    Ok(())
}

fn build_app(state: AppState) -> Router {
    let api = routes::api_router(state.clone());
    let ws_router = ws::router(state.clone());
    let web_dir = state.config_read().web_dir;
    let static_files = ServeDir::new(web_dir.clone())
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(web_dir.join("index.html")));

    // CORS: required for browser clients (PWA) calling the daemon from another origin.
    // MVP: allow any origin on LAN; tighten later if needed.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    Router::new()
        .route("/health", get(routes::health::health))
        .nest("/api/v1", api)
        .nest("/ws/v1", ws_router)
        .fallback_service(static_files)
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use std::path::PathBuf;
    use tower::ServiceExt;
    use uuid::Uuid;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn serves_spa_routes_without_breaking_api_auth() {
        let root = unique_temp_dir("aicli-static-test");
        let data_dir = root.join("data");
        let web_dir = root.join("web");
        std::fs::create_dir_all(&data_dir).expect("create data dir");
        std::fs::create_dir_all(&web_dir).expect("create web dir");
        std::fs::write(
            web_dir.join("index.html"),
            "<!doctype html><html><body>ai-cli-web</body></html>",
        )
        .expect("write index");
        std::fs::write(web_dir.join("asset.txt"), "asset-body").expect("write asset");

        let cfg = DaemonConfig::for_tests(data_dir.clone(), web_dir.clone(), "test-token".into());
        let db = db::init_sqlite(&cfg.data_dir).await.expect("init sqlite");
        let app = build_app(AppState::new(cfg, db));

        let health = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("health request"),
            )
            .await
            .expect("health response");
        assert_eq!(health.status(), StatusCode::OK);

        let root_page = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .expect("root request"),
            )
            .await
            .expect("root response");
        assert_eq!(root_page.status(), StatusCode::OK);
        let body = to_bytes(root_page.into_body(), usize::MAX)
            .await
            .expect("read root body");
        let body_text = String::from_utf8(body.to_vec()).expect("utf8 root body");
        assert!(body_text.contains("ai-cli-web"));

        let deep_link = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/instances/demo/term")
                    .body(Body::empty())
                    .expect("deep-link request"),
            )
            .await
            .expect("deep-link response");
        assert_eq!(deep_link.status(), StatusCode::OK);
        let deep_body = to_bytes(deep_link.into_body(), usize::MAX)
            .await
            .expect("read deep body");
        let deep_text = String::from_utf8(deep_body.to_vec()).expect("utf8 deep body");
        assert!(deep_text.contains("ai-cli-web"));

        let unauthorized = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/instances")
                    .body(Body::empty())
                    .expect("api request"),
            )
            .await
            .expect("api response");
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let _ = std::fs::remove_dir_all(root);
    }
}
