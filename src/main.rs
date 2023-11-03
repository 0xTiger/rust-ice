use std::time::Instant;
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
    PgPool, Pool, Postgres,
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
        .route("/inflation-viz", get(inflation_viz))
        .route("/product/:product_id", get(product))
        .route("/product/search", get(search))
        .route("/debug-dashboard", get(debug_dashboard))
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


async fn inflation() -> Html<String> {
    let header = r#"
    <!DOCTYPE html>
    <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.2/dist/css/bootstrap.min.css" rel="stylesheet" integrity="sha384-T3c6CoIi6uLrA9TneNEoa7RxnatzjcDSCmG1MXxSR1GAsXEV/Dwwykc2MPK8M2HN" crossorigin="anonymous">
    <script src="https://unpkg.com/htmx.org@1.9.6"></script>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
    <script src="https://cdn.jsdelivr.net/npm/chartjs-adapter-date-fns/dist/chartjs-adapter-date-fns.bundle.min.js"></script>
    "#;
    let dropdown = r##"
    <select name="timeframe" hx-get="/inflation-viz" hx-target="#inflation-viz" hx-swap="outerHTML">
        <option value="day-chart">Day Chart</option>
        <option value="week-chart">Week Chart</option>
        <option value="month-chart">Month Chart</option>
        <option value="day-table">Day Table</option>
        <option value="week-table">Week Table</option>
        <option value="month-table">Month Table</option>
    </select>
    <input type="text" name="q"
    hx-get="/inflation-viz"
    hx-trigger="keyup change delay:500ms"
    hx-target="#inflation-viz"
    placeholder="Search..."
    >
    "##;
    Html(format!(r#"{header}{dropdown}<div id="inflation-viz" hx-get="/inflation-viz?timeframe=day-chart" hx-trigger="load"></div>"#))
}


async fn calc_inflation_rate(pool: Pool<Postgres>, timeframe: u64) -> Vec<(NaiveDateTime, f64)> {

    use std::time::Instant;
    let now = Instant::now();

    let query = "SELECT gtin, price, scraped FROM price_history";
    let result: Result<Vec<(i32, Vec<f64>, Vec<NaiveDateTime>)>, sqlx::Error> = sqlx::query_as(query).fetch_all(&pool).await;
    let result = result.unwrap();
    println!("Query done in: {:.4?}", now.elapsed());

    let dts_to_check: Vec<NaiveDateTime> = (0..100)
        .into_iter()
        .map(|n| NaiveDate::from_ymd_opt(2023, 8, 1).unwrap().and_hms_opt(0, 0, 0).unwrap() + Days::new(n*timeframe)).collect();
    let mut inflation_data = Vec::new();
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
        }
        inflation_data.push((dt, relevant_prices.iter().sum::<f64>() / relevant_prices.len() as f64));
    }
    println!("Total: {:.4?}", now.elapsed());
    inflation_data
}


async fn calc_inflation_rate2(pool: Pool<Postgres>, timeframe: u64, namefilter: Option<&String>) -> Vec<(NaiveDateTime, f64)> {
    let now = Instant::now();

    let query = "
    SELECT DATE_TRUNC('day', to_date), 1 + AVG(increase / years) / 365 FROM
        (
        SELECT
            gtin,
            price,
            price / LAG(price) OVER (PARTITION BY gtin ORDER BY scraped) - 1 AS increase,
            LAG(scraped) OVER (PARTITION BY gtin ORDER BY scraped) from_date,
            scraped AS to_date,
            EXTRACT(epoch FROM 
                    scraped - LAG(scraped) OVER (PARTITION BY gtin ORDER BY scraped)
            ) / (60 * 60 * 24 * 365.25) AS years
        
        FROM product
        WHERE price > 0 AND name ~* $1
        ORDER BY gtin, scraped
        ) t1
    WHERE increase IS NOT NULL AND years > 0
    GROUP BY DATE_TRUNC('day', to_date)
    ORDER BY DATE_TRUNC('day', to_date)
    ";
    let result: Result<Vec<(NaiveDateTime, f64)>, sqlx::Error> = sqlx::query_as(query).bind(namefilter.unwrap_or(&"".to_string())).fetch_all(&pool).await;
    println!("Query done in: {:.4?}", now.elapsed());

    let random_dt = NaiveDate::from_ymd_opt(2023, 8, 1).unwrap().and_hms_opt(0, 0, 0).unwrap();

    let mut inflation_data = result.unwrap();
    inflation_data.insert(0, (random_dt, 1.0));
    inflation_data = inflation_data.into_iter().scan((random_dt, 1.0), |state, x| {
        state.0 = x.0;
        state.1 = state.1 * x.1;
        Some(*state)
    }).collect();


    println!("Total: {:.4?}", now.elapsed());
    inflation_data
}


async fn inflation_viz(Query(params): Query<HashMap<String, String>>, Extension(pool): Extension<PgPool>) -> Html<String> {
    let timeframe_default = &"day-chart".to_owned();
    let timeframe = params.get("timeframe").unwrap_or(timeframe_default).as_str();
    let namefilter = params.get("q");
    let timeframe_parts = timeframe.split("-").collect::<Vec<&str>>();
    let timeframe = match timeframe_parts[0] {
        "day" => 1,
        "week" => 7,
        "month" => 30,
        _ => 1
    };
    let is_chart = timeframe_parts[1] == "chart";
    let inflation_data = calc_inflation_rate2(pool, timeframe, namefilter).await;
    let final_table: Vec<String> = inflation_data
    .iter()
    .map(|(dt, val)| format!("<tr><td>{}</td><td>{:.3}</td></tr>", dt.date(), val))
    .collect();
    let table_html = format!(r#"<table class="table table-sm">{}</table>"#, final_table.join(""));

    let (x, y): (Vec<NaiveDateTime>, Vec<f64>) = inflation_data.into_iter().unzip();
    let x: Vec<String> = x.into_iter().map(|d| d.date().to_string()).collect();
    let chart_html = format!(r#"
    <div class="chart-container" style="position: relative; height: 70vh; width: 100vw;">
        <canvas id="inflation-chart"></canvas>
    </div>

    <script>
    var datasets = [{{
        label: "inflation",
        data: {y:?},
        pointHitRadius: 10,
        pointRadius: 0,
        borderColor: "black",
        backgroundColor: "black"

    }}];
    var labels = {x:?};
    var chart_type = "line";
    var data = {{
        labels: labels,
        datasets: datasets,    
    }};

    var config = {{
        type: chart_type,
        data: data,
        options: {{
            scales: {{
                y: {{
                    grace: "20%",
                }},
                x: {{
                    type: 'time',
                    grid: {{
                        display: false
                    }}
                }}
            }},
            animation: {{
                duration: 0,
            }},
            maintainAspectRatio: false
        }}
    }};
    var InflationChart = new Chart(
        document.getElementById('inflation-chart'),
        config,
    );
    </script>
    "#);

    let output_html = if is_chart {chart_html} else {table_html};
    Html(format!(r#"<div id="inflation-viz">{output_html}</div>"#))
}


async fn product(Path(product_id): Path<i32>, Extension(pool): Extension<PgPool>) -> impl IntoResponse {

    let result: Result<(i32, String, i64, String, String, Option<f64>, i32, String, f64, String, String), sqlx::Error> = sqlx::query_as(
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
    let result: Result<Vec<(i32, String, i64, String, String, Option<f64>, i32, String, f64, String, String)>, sqlx::Error> = sqlx::query_as(
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

async fn debug_dashboard(Extension(pool): Extension<PgPool>) -> Html<String> {
    let result: Result<(sqlx::types::Json<DebugInfo>,), sqlx::Error> = sqlx::query_as(
        "SELECT json_build_object(
            'total', (SELECT COUNT(*) FROM product),
            'unique', (SELECT COUNT(DISTINCT gtin) FROM product),
            'outdated', (SELECT COUNT(*) FROM productscrapestatus WHERE last_scraped < NOW() - INTERVAL '7 Days'),
            'notyetscraped', (SELECT COUNT(*) FROM productscrapestatus WHERE last_scraped IS NULL)
        )"
    ).fetch_one(&pool).await;


    let debug_info = result.unwrap().0;
    let total = debug_info.0.total;
    let outdated = debug_info.0.outdated;
    let unique = debug_info.0.unique;
    let notyetscraped = debug_info.0.notyetscraped;
    let header = r#"
    <!DOCTYPE html>
    <link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.2/dist/css/bootstrap.min.css" rel="stylesheet" integrity="sha384-T3c6CoIi6uLrA9TneNEoa7RxnatzjcDSCmG1MXxSR1GAsXEV/Dwwykc2MPK8M2HN" crossorigin="anonymous">
    <script src="https://unpkg.com/htmx.org@1.9.6"></script>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
    <script src="https://cdn.jsdelivr.net/npm/chartjs-adapter-date-fns/dist/chartjs-adapter-date-fns.bundle.min.js"></script>
    "#;
    let output_html = format!(r#"
    <div hx-get="/debug-dashboard" hx-trigger="every 1s">
    <table class="table">
    <tr><td>Total</td><td>{total}</td><tr>
    <tr><td>Outdated</td><td>{outdated}</td><tr>
    <tr><td>Unique</td><td>{unique}</td><tr>
    <tr><td>Not Yet Scraped</td><td>{notyetscraped}</td><tr>
    </table>
    </div>"#);
    return Html(header.to_owned() + &output_html)
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
    rating: Option<f64>,
    review_count: i32,
    brand: String,
    price: f64,
    url: String,
    availability: String
}

#[derive(Deserialize)]
struct DebugInfo {
    total: i64,
    unique: i64,
    outdated: i64,
    notyetscraped: i64,
}
