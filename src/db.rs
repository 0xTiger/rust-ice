use serde::{Serialize, Deserialize};
use sqlx::{
    postgres::PgPoolOptions,
    Pool,
    Postgres
};
use std::env;

#[derive(sqlx::FromRow)]
#[derive(Serialize)]
pub struct Product {
    pub gtin: Option<i32>,
    pub name: String,
    pub sku: i64,
    pub image: String,
    pub description: String,
    pub rating: Option<f64>,
    pub review_count: i32,
    pub brand: String,
    pub price: f64,
    pub url: String,
    pub availability: String,
    pub seller: String
}

#[derive(Deserialize)]
pub struct DebugInfo {
    pub total: i64,
    pub unique: i64,
    pub outdated: i64,
    pub notyetscraped: i64,
}

pub async fn db_conn() -> Pool<Postgres>{
    let pg_password: String = env::var("POSTGRES_PASSWORD").expect("$POSTGRES_PASSWORD is not set");
    let pg_user: String = env::var("POSTGRES_USER").expect("$POSTGRES_PASSWORD is not set");

    let pool = PgPoolOptions::new()
    .max_connections(5)
    .connect(format!("postgres://{pg_user}:{pg_password}@localhost:5444/supermarket").as_str())
    .await.unwrap();

    return pool
}