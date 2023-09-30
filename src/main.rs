use serde::{Deserialize, Serialize};
use chrono::{NaiveDateTime, NaiveDate, Days};
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


async fn inflation(Query(params): Query<HashMap<String, String>>, Extension(pool): Extension<PgPool>) -> Html<String> {
    let query = "
    SELECT gtin, ARRAY_AGG(price ORDER BY scraped), ARRAY_AGG(scraped ORDER BY scraped)
    FROM product
    GROUP BY gtin
    HAVING COUNT(*) >= 2";
    let result: Result<Vec<(i32, Vec<f64>, Vec<NaiveDateTime>)>, sqlx::Error> = sqlx::query_as(query).fetch_all(&pool).await;
    let result = result.unwrap();
    let timeframe_default = &"day".to_owned();
    let timeframe = params.get("timeframe").unwrap_or(timeframe_default).as_str();

    let timeframe = match timeframe {
        "day" => 1,
        "week" => 7,
        "month" => 30,
        _ => 1
    };
    let dts_to_check: Vec<NaiveDateTime> = (0..50)
        .into_iter()
        .map(|n| NaiveDate::from_ymd_opt(2023, 8, 1).unwrap().and_hms_opt(0, 0, 0).unwrap() + Days::new(n*timeframe)).collect();
    let mut final_table = Vec::new();
    for dt in dts_to_check {
        let mut relevant_prices = Vec::new();
        for (_gtin, prices, scraped) in &result {
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
    let final_table: Vec<String> = final_table.iter().map(|(dt, val)| (dt.date(), val)).map(|(dt, val)| format!("<tr><td>{dt}</td><td>{val:.3}</td></tr>")).collect();
    let output_html = final_table.join("");
    let header = r#"
        <script src="https://unpkg.com/htmx.org@1.9.6"></script>
        <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.2/dist/css/bootstrap.min.css\" rel=\"stylesheet\" integrity=\"sha384-T3c6CoIi6uLrA9TneNEoa7RxnatzjcDSCmG1MXxSR1GAsXEV/Dwwykc2MPK8M2HN" crossorigin="anonymous">
    "#;
    let dropdown = r##"
    <select name="timeframe" hx-get="/inflation" hx-target="#inflation-table" hx-indicator=".htmx-indicator">
        <option value="day">Day</option>
        <option value="week">Week</option>
        <option value="month">Month</option>
    </select>
    "##;
    if params.get("timeframe").is_none() {
        Html(format!("{header}{dropdown}<table class=\"table table-sm\" id=\"inflation-table\">{output_html}</table>"))
    } else {
        Html(format!("<table class=\"table table-sm\" id=\"inflation-table\">{output_html}</table>"))
    }
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
