use std::{
    time::Instant,
    collections::HashMap
};
use serde::Serialize;
use chrono::{NaiveDateTime, NaiveDate};
use dotenv::dotenv;
use axum::{
    extract::Path,
    extract::Query,
    extract::Request,
    http::StatusCode,
    http,
    response::{IntoResponse, Html, Response},
    routing::get,
    routing::post,
    Json,
    Router,
    Extension,
    middleware::{self, Next},
};
use sqlx::{
    Error,
    PgPool,
    Pool,
    Postgres,
};
use askama::Template;

mod db;
mod auth;
use auth::{
    get_login,
    post_login,
    get_logout,
    get_register,
    post_register,
    Backend,
};
use db::{
    db_conn,
    Product,
    DebugInfo,
    ApiKey,
    CreditsPeriod
};
use uuid::Uuid;

#[derive(Serialize)]
struct JStatus {
    detail: bool,
}

use axum::{error_handling::HandleErrorLayer, BoxError};
use axum_login::{
    login_required,
    tower_sessions::{Expiry, MemoryStore, SessionManagerLayer},
    AuthManagerLayerBuilder, AuthnBackend,
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


    let api_routes = Router::new()
        .route("/products/:product_id", get(product))
        .route("/products/search", get(search))
        .route_layer(middleware::from_fn(verify_header_api_key))
        .route("/ping", get(ping));
    let static_routes = Router::new()
        .route("/styles", get(styles))
        .route("/logo", get(logo));
    let authed_routes = Router::new()
        .route("/debug-dashboard", get(debug_dashboard))
        .route_layer(login_required!(Backend, login_url = "/login"))
        .route("/login", post(post_login))
        .route("/login", get(get_login))
        .route("/register", post(post_register))
        .route("/register", get(get_register))
        .route("/logout", get(get_logout))
        .layer(auth_service);
    let app = Router::new()
        .nest("/api", api_routes)
        .nest("/static", static_routes)
        .route("/", get(root))
        .route("/inflation", get(inflation))
        .route("/inflation-viz", get(inflation_viz))
        .route("/search-pretty-results", get(search_pretty_results))
        .route("/search", get(search_pretty_page))
        .merge(authed_routes)
        .layer(Extension(pool));

    let addr = "0.0.0.0:3000";
    tracing::debug!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app.into_make_service()).await.unwrap();
}


#[derive(Template)]
#[template(path="landing.html")]
struct LandingTemplate {}


async fn root() -> Html<String> {
    let inflation_template = LandingTemplate {};
    Html(inflation_template.render().unwrap())
}


async fn ping() -> (StatusCode, Json<JStatus>) {
    (StatusCode::OK, Json(JStatus { detail: true }))
}

#[derive(Template)]
#[template(path="inflation.html")]
struct InflationTemplate {}

async fn inflation() -> Html<String> {
    let inflation_template = InflationTemplate {};
    Html(inflation_template.render().unwrap())
}

async fn styles() -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/css")
        .body(include_str!("../templates/styles.css").to_owned())
        .unwrap() 
}


async fn logo() -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "image/svg+xml")
        .body(include_str!("../templates/shopping-cart-empty-side-view-svgrepo-com.svg").to_owned())
        .unwrap() 
}


async fn calc_inflation_rate2(pool: Pool<Postgres>, namefilter: Option<&String>) -> Vec<(NaiveDateTime, f64)> {
    let now = Instant::now();

    let query = "
    SELECT DATE_TRUNC('day', to_date), 1 + AVG(increase / years) / 365 FROM
        (
        SELECT
            seller, sku,
            price,
            price / LAG(price) OVER (PARTITION BY seller, sku ORDER BY scraped) - 1 AS increase,
            LAG(scraped) OVER (PARTITION BY seller, sku ORDER BY scraped) from_date,
            scraped AS to_date,
            EXTRACT(epoch FROM 
                    scraped - LAG(scraped) OVER (PARTITION BY seller, sku ORDER BY scraped)
            ) / (60 * 60 * 24 * 365.25) AS years
        
        FROM product
        WHERE price > 0 AND name ~* $1
        ORDER BY seller, sku, scraped
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
    let namefilter = params.get("q");
    let is_table = params.get("table").is_some();
    let inflation_data = calc_inflation_rate2(pool, namefilter).await;
    let final_table: String = inflation_data
        .iter()
        .map(|(dt, val)| format!("<tr><td>{}</td><td>{:.3}</td></tr>", dt.date(), val))
        .collect::<Vec<String>>()
        .join("\n");
    let table_html = format!(r#"<table class="table table-sm">{final_table}</table>"#);

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

    let output_html = if is_table {table_html} else {chart_html};
    Html(format!(r#"<div id="inflation-viz">{output_html}</div>"#))
}


async fn product(Path(product_id): Path<i32>, Extension(pool): Extension<PgPool>) -> impl IntoResponse {

    let result: Result<Product, sqlx::Error> = sqlx::query_as(
        "SELECT gtin, name, sku, image, description, rating, review_count, brand, price, url, availability, seller
        FROM product
        WHERE gtin = $1
        ORDER BY scraped DESC"
    )
    .bind(product_id) 
    .fetch_one(&pool).await;

    match result {
        Err(Error::RowNotFound) => {StatusCode::NOT_FOUND.into_response()}
        Err(value) => {panic!("{}", value)}
        Ok(product) => {Json(product).into_response()}
    }
}


async fn search_for_product(query: String, sort: &String, pool: PgPool) -> Result<Vec<Product>, sqlx::Error>{
    let result: Result<Vec<Product>, sqlx::Error> = sqlx::query_as(
        format!(
            "SELECT * FROM (
                SELECT DISTINCT ON (seller, sku) gtin, name, sku, image, description, rating, review_count, brand, price, url, availability, seller
                FROM product
                WHERE name ILIKE $1
                ORDER BY seller, sku, scraped DESC
            ) t1
            ORDER BY {sort} ASC
            LIMIT 10"
        ).as_str()
    )
    .bind(query).bind(sort) 
    .fetch_all(&pool).await;
    return result
}


async fn search(Query(params): Query<HashMap<String, String>>, Extension(pool): Extension<PgPool>) -> impl IntoResponse {
    let query = format!("%{}%", params.get("query").unwrap());
    let default_sort = &"name".to_string();
    let sort = params.get("sort").unwrap_or(default_sort); // SANITIZE THIS!!
    let result = search_for_product(query, sort, pool).await;

    match result {
        Err(Error::RowNotFound) => {StatusCode::NOT_FOUND.into_response()}
        Err(value) => {panic!("{}", value)}
        Ok(rows) => {Json(rows).into_response()}
    }
}


async fn search_pretty_results(Query(params): Query<HashMap<String, String>>, Extension(pool): Extension<PgPool>) -> Html<String> {
    let mut query = format!("%{}%", params.get("query").unwrap());
    if params.get("query").unwrap() == "" {
        query = format!("%pasta%")
    }
    let result = search_for_product(query, &"name".to_string(), pool).await.unwrap();

    let results_html: String = result.iter()
        .map(|product| {
            let name = &product.name;
            let price = &product.price;
            let brand = &product.brand;
            let rating = &product.rating.unwrap_or(0.0);
            let image = &product.image;
            let color = match product.seller.as_str() {
                "asda" => "green",
                "sainsburys" => "orange",
                "tesco" => "blue",
                _ => "black"
            };
            format!(r#"<tr><td><img src="{image}" width=24 height=24></td><td>{name}</td><td style="color: {color};">£{price:.2}</td><td>{brand}</td><td>{rating:.2?}</td></tr>"#)
        })
        .collect::<Vec<String>>()
        .join("\n");
    let output_html = format!(r#"
    <table class="table" id="search-results">
    {results_html}
    </table>"#);
    return Html(output_html)
}


#[derive(Template)]
#[template(path="search.html")]
struct SearchTemplate {}


async fn search_pretty_page() -> Html<String> {
    let search_template = SearchTemplate {};
    return Html(search_template.render().unwrap())
}

#[derive(Template)]
#[template(path="debug_dashboard.html")]
struct DebugDashboardTemplate {
    total: i64,
    outdated: i64,
    unique: i64,
    notyetscraped: i64,
}


async fn debug_dashboard(Extension(pool): Extension<PgPool>) -> Html<String> {
    let result: (sqlx::types::Json<DebugInfo>,) = sqlx::query_as(
        "SELECT json_build_object(
            'total', (SELECT COUNT(*) FROM product),
            'unique', (SELECT COUNT(DISTINCT (seller, sku)) FROM product),
            'outdated', (SELECT COUNT(*) FROM productscrapestatus WHERE last_scraped < NOW() - INTERVAL '7 Days'),
            'notyetscraped', (SELECT COUNT(*) FROM productscrapestatus WHERE last_scraped IS NULL)
        )"
    ).fetch_one(&pool).await.unwrap();


    let debug_info = result.0.0;
    let debug_dashboard_template = DebugDashboardTemplate {
        total:  debug_info.total,
        outdated:  debug_info.outdated,
        unique:  debug_info.unique,
        notyetscraped:  debug_info.notyetscraped
    };
    return Html(debug_dashboard_template.render().unwrap())
}



async fn verify_header_api_key(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    let auth_header = req.headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .and_then(|api_key| Uuid::try_parse(api_key).ok());

    let auth_header = if let Some(auth_header) = auth_header {
        auth_header
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let pool = db_conn().await;
    let credits_period: Option<CreditsPeriod> = sqlx::query_as(r#"
        UPDATE credits_period SET credits_used = credits_used + 1
        FROM api_key
        WHERE credits_period.users_id = api_key.users_id AND key = $1
        RETURNING credits_period.*
        "#)
        .bind(auth_header)
        .fetch_optional(&pool)
        .await.unwrap();
    if let Some(credits_period) = credits_period {
        println!("{}/{}", credits_period.credits_used, credits_period.credits_allocated);
        if credits_period.credits_used <= credits_period.credits_allocated {
            Ok(next.run(req).await)
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }


    // if let Some(current_user) = current_user_for_api_key(auth_header).await {
    //     // insert the current user into a request extension so the handler can
    //     // extract it
    //     req.extensions_mut().insert(current_user);
    //     Ok(next.run(req).await)
    // } else {
    //     Err(StatusCode::UNAUTHORIZED)
    // }
}