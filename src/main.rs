use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use tower_http::services::ServeDir;

const RELATIONSHIP_OPTIONS: [&str; 5] = [
    "Single",
    "In a relationship",
    "Engaged",
    "Married",
    "It's complicated",
];

#[derive(Serialize, FromRow)]
struct Post {
    id: i32,
    user: String,
    avatar_url: Option<String>,
    image_url: String,
    caption: String,
    likes: i64,
    liked_by_me: bool,
    comment_count: i64,
}

#[derive(Serialize, FromRow)]
struct Comment {
    id: i32,
    user: String,
    body: String,
}

#[derive(Serialize, FromRow)]
struct GridPost {
    id: i32,
    image_url: String,
}

#[derive(FromRow)]
struct ProfileRow {
    id: i32,
    username: String,
    display_name: Option<String>,
    bio: Option<String>,
    age: Option<i32>,
    location: Option<String>,
    interests: Option<String>,
    relationship_status: Option<String>,
    avatar_url: Option<String>,
    post_count: i64,
    follower_count: i64,
    following_count: i64,
    followed_by_me: bool,
}

#[derive(Serialize)]
struct Profile {
    username: String,
    display_name: Option<String>,
    bio: Option<String>,
    age: Option<i32>,
    location: Option<String>,
    interests: Option<String>,
    relationship_status: Option<String>,
    avatar_url: Option<String>,
    post_count: i64,
    follower_count: i64,
    following_count: i64,
    followed_by_me: bool,
    is_me: bool,
    posts: Vec<GridPost>,
}

#[derive(Serialize)]
struct AvatarInfo {
    avatar_url: String,
}

#[derive(Deserialize)]
struct ProfileUpdate {
    display_name: Option<String>,
    bio: Option<String>,
    age: Option<i32>,
    location: Option<String>,
    interests: Option<String>,
    relationship_status: Option<String>,
}

#[derive(Deserialize)]
struct FeedQuery {
    scope: Option<String>,
}

#[derive(Deserialize)]
struct NewComment {
    body: String,
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

async fn feed(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(q): Query<FeedQuery>,
) -> Result<Json<Vec<Post>>, StatusCode> {
    // Logged-out visitors can still read the feed; they just never have a "liked" heart.
    let me: Option<i32> = current_user_id(&pool, &headers).await.ok();
    // scope=following shows only your own posts and posts from people you follow.
    let following_only = q.scope.as_deref() == Some("following");

    let posts = sqlx::query_as::<_, Post>(
        r#"
        SELECT p.id,
               u.username AS "user",
               u.avatar_url,
               p.image_url,
               p.caption,
               (SELECT COUNT(*) FROM post_likes l WHERE l.post_id = p.id) AS likes,
               EXISTS (SELECT 1 FROM post_likes l
                       WHERE l.post_id = p.id AND l.user_id = $1) AS liked_by_me,
               (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) AS comment_count
        FROM posts p
        JOIN users u ON u.id = p.user_id
        WHERE (NOT $2
               OR p.user_id = $1
               OR p.user_id IN (SELECT followee_id FROM follows WHERE follower_id = $1))
        ORDER BY p.id DESC
        "#,
    )
    .bind(me)
    .bind(following_only)
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

// Works out the real image type from the file's first bytes (never from its name).
// Returns None for anything that isn't a JPEG, PNG, WebP or GIF.
fn image_extension(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if b.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("png")
    } else if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("webp")
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        Some("gif")
    } else {
        None
    }
}

// Saves an image under a random name and returns its URL (like "/uploads/<id>.jpg").
// The extension is chosen by us, so uploads can't overwrite each other or sneak in
// a name like "../something".
async fn save_image(bytes: &[u8], ext: &str) -> Result<String, StatusCode> {
    let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    tokio::fs::create_dir_all("static/uploads").await.map_err(|e| {
        eprintln!("could not create uploads folder: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    tokio::fs::write(format!("static/uploads/{filename}"), bytes)
        .await
        .map_err(|e| {
            eprintln!("could not save upload: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(format!("/uploads/{filename}"))
}

// Deletes an old uploaded file, but only if the URL looks exactly like one we created.
async fn remove_upload(url: &str) {
    if let Some(name) = url.strip_prefix("/uploads/") {
        let safe = !name.is_empty()
            && !name.contains('/')
            && !name.contains('\\')
            && !name.contains("..");
        if safe {
            let _ = tokio::fs::remove_file(format!("static/uploads/{name}")).await;
        }
    }
}

async fn create_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;

    let mut caption = String::new();
    let mut image: Option<(Vec<u8>, &'static str)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        let name = field.name().map(|s| s.to_string());
        match name.as_deref() {
            Some("caption") => {
                caption = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            }
            Some("image") => {
                let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                let ext = image_extension(&bytes).ok_or(StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
                image = Some((bytes.to_vec(), ext));
            }
            _ => {}
        }
    }

    let (bytes, ext) = image.ok_or(StatusCode::BAD_REQUEST)?;
    let caption = caption.trim();
    if caption.len() > 500 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let url = save_image(&bytes, ext).await?;
    sqlx::query("INSERT INTO posts (user_id, image_url, caption) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(&url)
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

async fn list_comments(
    State(pool): State<PgPool>,
    Path(post_id): Path<i32>,
) -> Result<Json<Vec<Comment>>, StatusCode> {
    let rows = sqlx::query_as::<_, Comment>(
        r#"
        SELECT c.id, u.username AS "user", c.body
        FROM comments c
        JOIN users u ON u.id = c.user_id
        WHERE c.post_id = $1
        ORDER BY c.id ASC
        LIMIT 200
        "#,
    )
    .bind(post_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("list comments failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows))
}

async fn add_comment(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(post_id): Path<i32>,
    Json(c): Json<NewComment>,
) -> Result<(StatusCode, Json<Comment>), StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let body = c.body.trim();
    if body.is_empty() || body.chars().count() > 300 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let row = sqlx::query_as::<_, Comment>(
        r#"
        WITH ins AS (
            INSERT INTO comments (post_id, user_id, body)
            VALUES ($1, $2, $3)
            RETURNING id, user_id, body
        )
        SELECT ins.id, u.username AS "user", ins.body
        FROM ins
        JOIN users u ON u.id = ins.user_id
        "#,
    )
    .bind(post_id)
    .bind(user_id)
    .bind(body)
    .fetch_one(&pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_foreign_key_violation() => StatusCode::NOT_FOUND,
        _ => {
            eprintln!("add comment failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    Ok((StatusCode::CREATED, Json(row)))
}

async fn profile(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<Json<Profile>, StatusCode> {
    let me: Option<i32> = current_user_id(&pool, &headers).await.ok();

    let row = sqlx::query_as::<_, ProfileRow>(
        r#"
        SELECT u.id,
               u.username,
               u.display_name,
               u.bio,
               u.age,
               u.location,
               u.interests,
               u.relationship_status,
               u.avatar_url,
               (SELECT COUNT(*) FROM posts WHERE user_id = u.id) AS post_count,
               (SELECT COUNT(*) FROM follows WHERE followee_id = u.id) AS follower_count,
               (SELECT COUNT(*) FROM follows WHERE follower_id = u.id) AS following_count,
               EXISTS (SELECT 1 FROM follows
                       WHERE follower_id = $2 AND followee_id = u.id) AS followed_by_me
        FROM users u
        WHERE u.username = $1
        "#,
    )
    .bind(&username)
    .bind(me)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        eprintln!("profile query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let posts = sqlx::query_as::<_, GridPost>(
        "SELECT id, image_url FROM posts WHERE user_id = $1 ORDER BY id DESC LIMIT 60",
    )
    .bind(row.id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("profile posts failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(Profile {
        username: row.username,
        display_name: row.display_name,
        bio: row.bio,
        age: row.age,
        location: row.location,
        interests: row.interests,
        relationship_status: row.relationship_status,
        avatar_url: row.avatar_url,
        post_count: row.post_count,
        follower_count: row.follower_count,
        following_count: row.following_count,
        followed_by_me: row.followed_by_me,
        is_me: me == Some(row.id),
        posts,
    }))
}

// Trims text; empty becomes "not set"; too long is rejected.
fn clean(opt: Option<String>, max_chars: usize) -> Result<Option<String>, StatusCode> {
    match opt {
        None => Ok(None),
        Some(s) => {
            let t = s.trim();
            if t.is_empty() {
                Ok(None)
            } else if t.chars().count() > max_chars {
                Err(StatusCode::BAD_REQUEST)
            } else {
                Ok(Some(t.to_string()))
            }
        }
    }
}

async fn update_profile(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(p): Json<ProfileUpdate>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;

    let display_name = clean(p.display_name, 50)?;
    let bio = clean(p.bio, 300)?;
    let location = clean(p.location, 60)?;
    let interests = clean(p.interests, 200)?;
    let relationship_status = clean(p.relationship_status, 30)?;

    if let Some(rs) = &relationship_status {
        if !RELATIONSHIP_OPTIONS.contains(&rs.as_str()) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if let Some(a) = p.age {
        if !(13..=120).contains(&a) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    sqlx::query(
        "UPDATE users
         SET display_name = $1, bio = $2, age = $3, location = $4,
             interests = $5, relationship_status = $6
         WHERE id = $7",
    )
    .bind(display_name)
    .bind(bio)
    .bind(p.age)
    .bind(location)
    .bind(interests)
    .bind(relationship_status)
    .bind(user_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("update profile failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn upload_avatar(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<AvatarInfo>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;

    let mut image: Option<(Vec<u8>, &'static str)> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        if field.name() == Some("image") {
            let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            let ext = image_extension(&bytes).ok_or(StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
            image = Some((bytes.to_vec(), ext));
        }
    }
    let (bytes, ext) = image.ok_or(StatusCode::BAD_REQUEST)?;
    let url = save_image(&bytes, ext).await?;

    let old: Option<(Option<String>,)> =
        sqlx::query_as("SELECT avatar_url FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| {
                eprintln!("avatar lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    sqlx::query("UPDATE users SET avatar_url = $1 WHERE id = $2")
        .bind(&url)
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("avatar update failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if let Some((Some(old_url),)) = old {
        remove_upload(&old_url).await;
    }
    Ok(Json(AvatarInfo { avatar_url: url }))
}

async fn remove_avatar(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;

    let old: Option<(Option<String>,)> =
        sqlx::query_as("SELECT avatar_url FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| {
                eprintln!("avatar lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    sqlx::query("UPDATE users SET avatar_url = NULL WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("avatar remove failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if let Some((Some(old_url),)) = old {
        remove_upload(&old_url).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn follow_user(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let me = current_user_id(&pool, &headers).await?;

    let target: Option<(i32,)> = sqlx::query_as("SELECT id FROM users WHERE username = $1")
        .bind(&username)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            eprintln!("follow lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let (target_id,) = target.ok_or(StatusCode::NOT_FOUND)?;

    if target_id == me {
        return Err(StatusCode::BAD_REQUEST); // you can't follow yourself
    }

    sqlx::query(
        "INSERT INTO follows (follower_id, followee_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(me)
    .bind(target_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("follow failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unfollow_user(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let me = current_user_id(&pool, &headers).await?;
    sqlx::query(
        "DELETE FROM follows
         WHERE follower_id = $1
           AND followee_id = (SELECT id FROM users WHERE username = $2)",
    )
    .bind(me)
    .bind(&username)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("unfollow failed: {e}");
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
        .route("/api/me/profile", put(update_profile))
        .route(
            "/api/me/avatar",
            post(upload_avatar)
                .delete(remove_avatar)
                .layer(DefaultBodyLimit::max(5 * 1024 * 1024)),
        )
        .route(
            "/api/posts",
            post(create_post).layer(DefaultBodyLimit::max(10 * 1024 * 1024)),
        )
        .route("/api/posts/{id}/like", post(like_post).delete(unlike_post))
        .route("/api/posts/{id}/comments", get(list_comments).post(add_comment))
        .route("/api/users/{username}", get(profile))
        .route(
            "/api/users/{username}/follow",
            post(follow_user).delete(unfollow_user),
        )
        .fallback_service(ServeDir::new("static"))
        .with_state(pool);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("Running on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}