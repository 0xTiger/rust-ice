use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::env;
use std::collections::HashMap;
use dotenv::dotenv;
use axum::{
    extract::Path,
    extract::Query,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json,
    Router,
    Extension,
};
use sqlx::{
    postgres::PgPoolOptions,
    Error,
    PgPool,
};

#[tokio::main]
async fn main() {
    dotenv().ok();
    let pg_password: String = env::var("POSTGRES_PASSWORD").expect("$POSTGRES_PASSWORD is not set");
    let pg_user: String = env::var("POSTGRES_USER").expect("$POSTGRES_PASSWORD is not set");

    let pool = PgPoolOptions::new()
    .max_connections(5)
    .connect(format!("postgres://{pg_user}:{pg_password}@localhost:5444/supermarket").as_str())
    .await.unwrap();

    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(root))
        .route("/ping", get(ping))
        .route("/product/:product_id", get(product))
        .route("/product/search", get(search))
        .layer(Extension(pool));

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn root() -> &'static str {
    "Hello, World!"
}

async fn ping() -> (StatusCode, Json<JStatus>) {
    (StatusCode::OK, Json(JStatus { detail: true }))
}

async fn product(Path(product_id): Path<i32>, Extension(pool): Extension<PgPool>) -> impl IntoResponse {

    let result: Result<(i32, String, i64, String, String, f64, i32, String, f64, String, String), sqlx::Error> = sqlx::query_as(
        "SELECT gtin, name, sku, image, description, rating, review_count, brand, price, url, availability
        FROM product
        WHERE gtin = $1"
    )
    .bind(product_id) 
    .fetch_one(&pool).await;

    match result {
        Err(Error::RowNotFound) => {StatusCode::NOT_FOUND.into_response()}
        Err(value) => {panic!("{}", value)}
        Ok(row) => {
            let (gtin, name, sku, image, description, rating, review_count, brand, price, url, availability) = row;
            let prod = Product { gtin: gtin, name: name, sku: sku, image: image, description: description,
                rating: rating, review_count: review_count, brand: brand,
                price: price, url: url, availability: availability};
            Json(prod).into_response()
        }
    }
}

async fn search(Query(params): Query<HashMap<String, String>>, Extension(pool): Extension<PgPool>) -> impl IntoResponse {

    let query = format!("%{}%", params.get("query").unwrap());
    let default_sort = &"name".to_string();
    let sort = params.get("sort").unwrap_or(default_sort); // SANITIZE THIS!!
    let result: Result<Vec<(i32, String, i64, String, String, f64, i32, String, f64, String, String)>, sqlx::Error> = sqlx::query_as(
        format!(
            "SELECT gtin, name, sku, image, description, rating, review_count, brand, price, url, availability
            FROM product
            WHERE name ILIKE $1
            ORDER BY {sort} DESC
            LIMIT 10"
        ).as_str()
    )
    .bind(query).bind(sort) 
    .fetch_all(&pool).await;

    match result {
        Err(Error::RowNotFound) => {StatusCode::NOT_FOUND.into_response()}
        Err(value) => {panic!("{}", value)}
        Ok(rows) => {

            let mut prods = Vec::new();
            for (gtin, name, sku, image, description, rating, review_count, brand, price, url, availability) in rows {
                let prod = Product { gtin: gtin, name: name, sku: sku, image: image, description: description,
                    rating: rating, review_count: review_count, brand: brand,
                    price: price, url: url, availability: availability};
                prods.push(prod);
            }
            Json(prods).into_response()
        }
    }
}

#[derive(Serialize)]
struct JStatus {
    detail: bool,
}


#[derive(Serialize)]
struct Product {
    gtin: i32,
    name: String,
    sku: i64,
    image: String,
    description: String,
    rating: f64,
    review_count: i32,
    brand: String,
    price: f64,
    url: String,
    availability: String
}
