use serde::{Deserialize, Serialize};
use chrono::{NaiveDateTime, NaiveDate};
use std::net::SocketAddr;
use std::env;
use std::collections::HashMap;
use dotenv::dotenv;
use axum::{
    extract::Path,
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Html},
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
        .route("/inflation", get(inflation))
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


async fn inflation(Extension(pool): Extension<PgPool>) -> Html<String> {
    let query = "
    SELECT gtin, ARRAY_AGG(price ORDER BY scraped), ARRAY_AGG(scraped ORDER BY scraped)
    FROM product
    GROUP BY gtin
    HAVING COUNT(*) >= 2";
    let result: Result<Vec<(i32, Vec<f64>, Vec<NaiveDateTime>)>, sqlx::Error> = sqlx::query_as(query).fetch_all(&pool).await;
    let result = result.unwrap();
    let dts_to_check  = vec![
        NaiveDate::from_ymd_opt(2023, 8, 1).unwrap().and_hms_opt(0, 0, 0).unwrap(),
        NaiveDate::from_ymd_opt(2023, 9, 1).unwrap().and_hms_opt(0, 0, 0).unwrap(),
        NaiveDate::from_ymd_opt(2023, 9, 10).unwrap().and_hms_opt(0, 0, 0).unwrap(),
        NaiveDate::from_ymd_opt(2023, 9, 20).unwrap().and_hms_opt(0, 0, 0).unwrap(),
        NaiveDate::from_ymd_opt(2023, 9, 30).unwrap().and_hms_opt(0, 0, 0).unwrap()
    ];
    let mut final_table = Vec::new();
    for dt in dts_to_check {
        let mut relevant_prices = Vec::new();
        for (gtin, prices, scraped) in &result {
            let idx = match scraped.binary_search(&dt) {
                Ok(x) => x,
                Err(x) => x.saturating_sub(1)
            };
            if prices[0] == 0.0 {
                continue
            }
            relevant_prices.push(prices[idx] / prices[0]);
            // println!("{gtin}{prices:?}{scraped:?}");
        }
        // println!("{:?}{}", relevant_prices.iter().sum::<f64>(), relevant_prices.len());
        final_table.push((dt, relevant_prices.iter().sum::<f64>() / relevant_prices.len() as f64));
    }
    let final_table: Vec<String> = final_table.iter().map(|(dt, val)| format!("<tr><td>{dt}</td><td>{val}</td></tr>")).collect();
    let output_html = final_table.join("");
    Html(format!("<table>{output_html}</table>"))
}


async fn product(Path(product_id): Path<i32>, Extension(pool): Extension<PgPool>) -> impl IntoResponse {

    let result: Result<(i32, String, i64, String, String, f64, i32, String, f64, String, String), sqlx::Error> = sqlx::query_as(
        "SELECT gtin, name, sku, image, description, rating, review_count, brand, price, url, availability
        FROM product
        WHERE gtin = $1
        ORDER BY scraped DESC"
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
            "SELECT * FROM (
                SELECT DISTINCT ON (gtin) gtin, name, sku, image, description, rating, review_count, brand, price, url, availability
                FROM product
                WHERE name ILIKE $1
                ORDER BY gtin, scraped DESC
            ) t1
            ORDER BY {sort} ASC
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
