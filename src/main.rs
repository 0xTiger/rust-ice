use dotenv::dotenv;
use axum::{
    http::StatusCode,
    routing::post,
    Router,
};

mod db;
mod auth;
use auth::{
    post_login,
    Backend,
};
use db::db_conn;

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
        .layer(HandleErrorLayer::new(|_: BoxError| async {StatusCode::BAD_REQUEST}))
        .layer(AuthManagerLayerBuilder::new(backend, session_layer).build());

    let app = Router::new()
        .route("/login", post(post_login))
        .layer(auth_service);

    tracing::debug!("listening");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app.into_make_service()).await.unwrap();
}