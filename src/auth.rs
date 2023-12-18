
use async_trait::async_trait;
use axum::{
    response::{
        Redirect,
        IntoResponse,
        Response
    },
    http::StatusCode,
    Form,
    extract::Query,
    response::Html,
};
use axum_login::{AuthUser, AuthnBackend, UserId};
use password_auth::{verify_password, generate_hash};
use serde::{Serialize, Deserialize};
use sqlx::{FromRow, PgPool};
use askama::Template;

use crate::db::db_conn;

#[derive(Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    id: i64,
    email: String,
    password: String,
}

// Here we've implemented `Debug` manually to avoid accidentally logging the
// password hash.
impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("email", &self.email)
            .field("password", &"[redacted]")
            .finish()
    }
}

impl AuthUser for User {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.password.as_bytes() // We use the password hash as the auth
                                 // hash--what this means
                                 // is when the user changes their password the
                                 // auth session becomes invalid.
    }
}

// This allows us to extract the authentication fields from forms. We use this
// to authenticate requests with the backend.
#[derive(Debug, Clone, Deserialize)]
pub struct Credentials {
    pub email: String,
    pub password: String,
    pub next: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Backend {
    db: PgPool,
}

impl Backend {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }
}

#[async_trait]
impl AuthnBackend for Backend {
    type User = User;
    type Credentials = Credentials;
    type Error = sqlx::Error;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let user: Option<Self::User> = sqlx::query_as("SELECT * FROM users WHERE email = $1 ")
            .bind(creds.email)
            .fetch_optional(&self.db)
            .await?;
        Ok(user.filter(|user| {
            verify_password(creds.password, &user.password)
                .ok()
                .is_some() // We're using password-based authentication--this
                           // works by comparing our form input with an argon2
                           // password hash.
        }))
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        let user = sqlx::query_as("SELECT * FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&self.db)
            .await?;

        Ok(user)
    }
}

// We use a type alias for convenience.
//
// Note that we've supplied our concrete backend here.
type AuthSession = axum_login::AuthSession<Backend>;




#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    message: Option<String>,
    next: Option<String>,
}

#[derive(Template)]
#[template(path = "register.html")]
struct RegisterTemplate {
    message: Option<String>,
    next: Option<String>,
}

// This allows us to extract the "next" field from the query string. We use this
// to redirect after log in.
#[derive(Debug, Deserialize)]
pub struct NextUrl {
    next: Option<String>,
}


pub async fn post_login(mut auth_session: AuthSession, Form(creds): Form<Credentials>) -> impl IntoResponse {
    let user = match auth_session.authenticate(creds.clone()).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return Response::builder().body(LoginTemplate {
                message: Some("Invalid credentials.".to_string()),
                next: creds.next,
            }
            .render().unwrap()).unwrap().into_response()
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if auth_session.login(&user).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    if let Some(ref next) = creds.next {
        Redirect::to(next).into_response()
    } else {
        Redirect::to("/").into_response()
    }
}

pub async fn get_login(Query(NextUrl { next }): Query<NextUrl>) -> Html<String> {
    Html(LoginTemplate {
        message: None,
        next,
    }.render().unwrap())
}

pub async fn get_logout(mut auth_session: AuthSession) -> impl IntoResponse {
    match auth_session.logout() {
        Ok(_) => Redirect::to("/login").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn get_register(Query(NextUrl { next }): Query<NextUrl>) -> impl IntoResponse {
    Html(RegisterTemplate {
        message: None,
        next,
    }.render().unwrap())
}


pub async fn post_register(mut auth_session: AuthSession, Form(creds): Form<Credentials>) -> impl IntoResponse {
    let password_hash = generate_hash(creds.password);
    let pool = db_conn().await;
    let query_result = sqlx::query("INSERT INTO users (email, password) VALUES ($1, $2)")
        .bind(creds.email)
        .bind(password_hash)
        .execute(&pool).await;
    if query_result.is_err() {
        println!("{:?}", query_result.err());
        return Response::builder().body(RegisterTemplate {
            message: Some("Invalid credentials.".to_string()),
            next: creds.next,
        }
        .render().unwrap()).unwrap().into_response()
    }

    if let Some(ref next) = creds.next {
        Redirect::to(next).into_response()
    } else {
        Redirect::to("/").into_response()
    }
}