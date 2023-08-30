use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use sqlx::postgres::PgPoolOptions;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(root))
        .route("/ping", get(ping))
        .route("/product", get(product));

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}

async fn ping() -> (StatusCode, Json<JStatus>) {
    (StatusCode::OK, Json(JStatus { detail: true }))
}

async fn product() -> (StatusCode, Json<Product>) {
    
    let pool = PgPoolOptions::new()
    .max_connections(5)
    .connect("postgres://postgres:***@localhost:5444/supermarket").await.unwrap();

    let row: (i32, String) = sqlx::query_as("SELECT gtin, name FROM product WHERE gtin = $1")
    .bind(7277397) 
    .fetch_one(&pool).await.unwrap();

    let (gtin, name) = row;
    let prod = Product { id: gtin, name: name };
    (StatusCode::OK, Json(prod))
}

#[derive(Serialize)]
struct JStatus {
    detail: bool,
}


#[derive(Serialize)]
struct Product {
    id: i32,
    name: String,
}
