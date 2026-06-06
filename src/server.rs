use std::sync::Arc;

use anyhow::Context;
use axum::http::{header, HeaderValue, Method};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::config::{load_config, load_dotenv};
use crate::auth::{db, webauthn};
use crate::routes;
use crate::AppState;

pub async fn run() -> anyhow::Result<()> {
    init_tracing();
    load_dotenv(".env");
    let cfg = load_config();

    let pool = db::make_pool(&cfg.database_url).await?;
    let webauthn = webauthn::build(&cfg)?;

    let cors = build_cors(&cfg.origin)?;
    let port = cfg.port;

    let state = AppState {
        pool,
        config: Arc::new(cfg),
        webauthn: Arc::new(webauthn),
    };

    let app = routes::router(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("binding to {addr}"))?;

    println!("rust-auth-server listening on http://localhost:{port}");
    axum::serve(listener, app)
        .await
        .context("running HTTP server")
}

fn build_cors(origin: &str) -> anyhow::Result<CorsLayer> {
    let origin: HeaderValue = origin.parse().context("ORIGIN is not a valid header value")?;
    Ok(CorsLayer::new()
        .allow_origin(origin)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .allow_credentials(true))
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info"));
    let _ = fmt().with_env_filter(filter).try_init();
}
