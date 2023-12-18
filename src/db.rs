use sqlx::{
    postgres::PgPoolOptions,
    Pool,
    Postgres
};


pub async fn db_conn() -> Pool<Postgres>{
    let conn_string = "postgres://user:password@localhost:5432/mydb";
    let pool = PgPoolOptions::new()
    .max_connections(5)
    .connect(conn_string)
    .await.unwrap();
    return pool
}