use std::env;
use dotenv::dotenv;
use axum::{
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::Error;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(root))
        .route("/ping", get(ping))
        .route("/product/:product_id", get(product));

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

async fn product(Path(product_id): Path<i32>) -> impl IntoResponse {
    
    let pg_password: String = env::var("POSTGRES_PASSWORD").expect("$POSTGRES_PASSWORD is not set");
    let pg_user: String = env::var("POSTGRES_USER").expect("$POSTGRES_PASSWORD is not set");

    let pool = PgPoolOptions::new()
    .max_connections(5)
    .connect(format!("postgres://{pg_user}:{pg_password}@localhost:5444/supermarket").as_str())
    .await.unwrap();

    // let row: (i32, String) 
    let result: Result<(i32, String), sqlx::Error>= sqlx::query_as("SELECT gtin, name FROM product WHERE gtin = $1")
    .bind(product_id) 
    .fetch_one(&pool).await;

    // if result.is_err_and(|x| x == Error::RowNotFound) {
    //     (StatusCode::NOT_FOUND, Json(prod))
    // }
    match result {
        Err(Error::RowNotFound) => {StatusCode::NOT_FOUND.into_response()}
        Err(_) => {panic!()}
        Ok(row) => {
            let (gtin, name) = row;
            let prod = Product { id: gtin, name: name };
            Json(prod).into_response()
        }
    }

    

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
