use serde::Serialize;
use dotenv::dotenv;
use axum::{
    http::StatusCode,
    routing::post,
    Json,
    Router,
    Extension,
};

mod db;
mod auth;
use auth::{
    post_login,
    post_register,
    Backend,
};
use db::db_conn;

#[derive(Serialize)]
struct JStatus {
    detail: bool,
}

use axum::{error_handling::HandleErrorLayer, BoxError};
use axum_login::{
    tower_sessions::{Expiry, MemoryStore, SessionManagerLayer},
    AuthManagerLayerBuilder
};
use time::Duration;
use tower::ServiceBuilder;



#[tokio::main]
async fn main() {
    dotenv().ok();
    let pool = db_conn().await;
    tracing_subscriber::fmt::init();

        // Session layer.
    //
    // This uses `tower-sessions` to establish a layer that will provide the session
    // as a request extension.
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(Duration::days(1)));

    // Auth service.
    //
    // This combines the session layer with our backend to establish the auth
    // service which will provide the auth session as a request extension.
    let backend = Backend::new(pool.clone());
    let auth_service = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|_: BoxError| async {
            StatusCode::BAD_REQUEST
        }))
        .layer(AuthManagerLayerBuilder::new(backend, session_layer).build());


    let app = Router::new()
        .route("/login", post(post_login))
        .route("/register", post(post_register))
        .layer(auth_service)
        .layer(Extension(pool));

    let addr = "0.0.0.0:3000";
    tracing::debug!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app.into_make_service()).await.unwrap();
}

async fn ping() -> (StatusCode, Json<JStatus>) {
    (StatusCode::OK, Json(JStatus { detail: true }))
}