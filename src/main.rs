use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use tower_http::services::ServeDir;

#[derive(Serialize, FromRow)]
struct Post {
    id: i32,
    user: String,
    image_url: String,
    caption: String,
    likes: i64,
    liked_by_me: bool,
}

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct UserInfo {
    username: String,
}

#[derive(Deserialize)]
struct NewPost {
    image_url: String,
    caption: String,
}

async fn feed(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<Vec<Post>>, StatusCode> {
    // Logged-out visitors can still read the feed; they just never have a "liked" heart.
    let me: Option<i32> = current_user_id(&pool, &headers).await.ok();

    let posts = sqlx::query_as::<_, Post>(
        r#"
        SELECT p.id,
               u.username AS "user",
               p.image_url,
               p.caption,
               (SELECT COUNT(*) FROM post_likes l WHERE l.post_id = p.id) AS likes,
               EXISTS (SELECT 1 FROM post_likes l
                       WHERE l.post_id = p.id AND l.user_id = $1) AS liked_by_me
        FROM posts p
        JOIN users u ON u.id = p.user_id
        ORDER BY p.id DESC
        "#,
    )
    .bind(me)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("feed query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(posts))
}

// Creates a session row and returns the Set-Cookie header value.
async fn start_session(pool: &PgPool, user_id: i32) -> Result<String, StatusCode> {
    let token = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO sessions (token, user_id) VALUES ($1, $2)")
        .bind(&token)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|e| {
            eprintln!("session insert failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(format!(
        "session={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age=604800"
    ))
}

async fn signup(
    State(pool): State<PgPool>,
    Json(c): Json<Credentials>,
) -> Result<impl IntoResponse, StatusCode> {
    let name_ok = c.username.len() >= 3
        && c.username.len() <= 30
        && c.username.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    if !name_ok || c.password.len() < 8 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(c.password.as_bytes(), &salt)
        .map_err(|e| {
            eprintln!("hashing failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .to_string();

    let row: Result<(i32,), sqlx::Error> = sqlx::query_as(
        "INSERT INTO users (username, password_hash) VALUES ($1, $2) RETURNING id",
    )
    .bind(&c.username)
    .bind(&hash)
    .fetch_one(&pool)
    .await;

    let user_id = match row {
        Ok((id,)) => id,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(StatusCode::CONFLICT) // username taken
        }
        Err(e) => {
            eprintln!("signup failed: {e}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let cookie = start_session(&pool, user_id).await?;
    Ok((
        StatusCode::CREATED,
        [(header::SET_COOKIE, cookie)],
        Json(UserInfo { username: c.username }),
    ))
}

async fn login(
    State(pool): State<PgPool>,
    Json(c): Json<Credentials>,
) -> Result<impl IntoResponse, StatusCode> {
    let row: Option<(i32, Option<String>)> =
        sqlx::query_as("SELECT id, password_hash FROM users WHERE username = $1")
            .bind(&c.username)
            .fetch_optional(&pool)
            .await
            .map_err(|e| {
                eprintln!("login query failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    // Same 401 for "no such user" and "wrong password", so attackers can't tell which.
    let (user_id, stored_hash) = match row {
        Some((id, Some(h))) => (id, h),
        _ => return Err(StatusCode::UNAUTHORIZED),
    };
    let parsed = PasswordHash::new(&stored_hash).map_err(|_| StatusCode::UNAUTHORIZED)?;
    Argon2::default()
        .verify_password(c.password.as_bytes(), &parsed)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let cookie = start_session(&pool, user_id).await?;
    Ok((
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(UserInfo { username: c.username }),
    ))
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|part| {
        part.trim().strip_prefix("session=").map(|v| v.to_string())
    })
}

async fn me(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<UserInfo>, StatusCode> {
    let token = session_token(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT u.username FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token = $1 AND s.created_at > now() - interval '7 days'",
    )
    .bind(&token)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        eprintln!("me query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match row {
        Some((username,)) => Ok(Json(UserInfo { username })),
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn logout(State(pool): State<PgPool>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(token) = session_token(&headers) {
        let _ = sqlx::query("DELETE FROM sessions WHERE token = $1")
            .bind(token)
            .execute(&pool)
            .await;
    }
    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, "session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0")],
    )
}

async fn current_user_id(pool: &PgPool, headers: &HeaderMap) -> Result<i32, StatusCode> {
    let token = session_token(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT user_id FROM sessions WHERE token = $1 AND created_at > now() - interval '7 days'",
    )
    .bind(&token)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        eprintln!("session lookup failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    row.map(|(id,)| id).ok_or(StatusCode::UNAUTHORIZED)
}

async fn create_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(p): Json<NewPost>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let url = p.image_url.trim();
    let caption = p.caption.trim();
    let url_ok = url.starts_with("https://") || url.starts_with("http://");
    if !url_ok || url.len() > 500 || caption.len() > 500 {
        return Err(StatusCode::BAD_REQUEST);
    }
    sqlx::query("INSERT INTO posts (user_id, image_url, caption) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(url)
        .bind(caption)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("create post failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::CREATED)
}

async fn like_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(post_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    // The primary key (user_id, post_id) means liking twice does nothing.
    sqlx::query(
        "INSERT INTO post_likes (user_id, post_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(post_id)
    .execute(&pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_foreign_key_violation() => StatusCode::NOT_FOUND,
        _ => {
            eprintln!("like failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unlike_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(post_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    sqlx::query("DELETE FROM post_likes WHERE user_id = $1 AND post_id = $2")
        .bind(user_id)
        .bind(post_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("unlike failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("could not connect to the database (is Docker running?)");

    let app = Router::new()
        .route("/api/feed", get(feed))
        .route("/api/signup", post(signup))
        .route("/api/login", post(login))
        .route("/api/me", get(me))
        .route("/api/logout", post(logout))
        .route("/api/posts", post(create_post))
        .route("/api/posts/{id}/like", post(like_post).delete(unlike_post))
        .fallback_service(ServeDir::new("static"))
        .with_state(pool);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("Running on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}