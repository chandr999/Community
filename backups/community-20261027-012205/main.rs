use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, postgres::PgPoolOptions};
use std::collections::HashSet;
use tower_http::services::ServeDir;

const RELATIONSHIP_OPTIONS: [&str; 5] = [
    "Single",
    "In a relationship",
    "Engaged",
    "Married",
    "It's complicated",
];

const MOOD_OPTIONS: [&str; 11] = [
    "Bored", "Romantic", "Angry", "Sad", "Happy", "Calm", "Excited", "Playful", "Lonely",
    "Grateful", "Stressed",
];

fn mood_is_valid(mood: &str) -> bool {
    MOOD_OPTIONS.contains(&mood)
}

#[derive(Serialize, FromRow)]
struct Post {
    id: i32,
    user: String,
    avatar_url: Option<String>,
    mood: Option<String>,
    image_url: String,
    media_type: String,
    caption: String,
    likes: i64,
    liked_by_me: bool,
    dislikes: i64,
    disliked_by_me: bool,
    hates: i64,
    hated_by_me: bool,
    reshares: i64,
    reshared_by_me: bool,
    followed_by_me: bool,
    favorited_by_me: bool,
    original_author: Option<String>,
    comment_count: i64,
    tags: Vec<String>,
}

#[derive(Serialize, FromRow)]
struct ActiveUser {
    username: String,
    avatar_url: Option<String>,
    mood: Option<String>,
}

#[derive(Serialize, FromRow)]
struct ActivityCheckin {
    id: i64,
    username: String,
    avatar_url: Option<String>,
    category: String,
    place: String,
    note: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    is_following: bool,
    is_follower: bool,
    created_at: String,
}

#[derive(Serialize, FromRow)]
struct LiveLocation {
    username: String,
    avatar_url: Option<String>,
    mood: Option<String>,
    latitude: f64,
    longitude: f64,
    updated_at: String,
}

#[derive(Serialize, FromRow)]
struct Comment {
    id: i32,
    user: String,
    body: String,
}

#[derive(Serialize, FromRow)]
struct ActivityItem {
    id: i64,
    kind: String,
    actor: String,
    post_id: i32,
    image_url: String,
    detail: String,
    created_at: String,
}

#[derive(Serialize, FromRow)]
struct ProfileVisitor {
    username: String,
    avatar_url: Option<String>,
    last_visited_at: String,
}

#[derive(Serialize)]
struct MyActivity {
    notifications: Vec<ActivityItem>,
    visitors: Vec<ProfileVisitor>,
}

#[derive(Serialize, FromRow)]
struct SettingsDashboardStats {
    profile_visits_count: i64,
    follower_count: i64,
    new_followers_count: i64,
    following_count: i64,
    posts_shared_count: i64,
    posts_reshared_by_others_count: i64,
    pending_personal_requests_count: i64,
    admirers_count: i64,
    blocked_profiles_count: i64,
}

#[derive(Serialize)]
struct SettingsDashboard {
    stats: SettingsDashboardStats,
    visitors: Vec<ProfileVisitor>,
    new_followers: Vec<ProfileConnection>,
    blocked_profiles: Vec<BlockedUser>,
}

#[derive(Serialize, FromRow)]
struct ConversationSummary {
    id: i32,
    title: Option<String>,
    is_group: bool,
    is_owner: bool,
    is_muted: bool,
    members: Vec<String>,
    latest_message: Option<String>,
    latest_message_at: Option<String>,
    unread_count: i64,
}

#[derive(Serialize, FromRow)]
struct AdminUser {
    id: i32,
    username: String,
    email: Option<String>,
    phone_number: Option<String>,
    date_of_birth: Option<String>,
    company_name: Option<String>,
    designation: Option<String>,
    contact_email: Option<String>,
    instagram_handle: Option<String>,
    facebook_handle: Option<String>,
    tiktok_handle: Option<String>,
    linkedin_handle: Option<String>,
    birthday_visibility: String,
    work_visibility: String,
    email_visibility: String,
    phone_visibility: String,
    social_visibility: String,
    display_name: Option<String>,
    age: Option<i32>,
    bio: Option<String>,
    location: Option<String>,
    mood: Option<String>,
    interests: Option<String>,
    relationship_status: Option<String>,
    avatar_url: Option<String>,
    cover_image_url: Option<String>,
    created_at: String,
    account_enabled: bool,
    profile_visibility: String,
    message_privacy: String,
    show_activity_status: bool,
    keep_profile_visitors: bool,
    notify_mentions: bool,
    cover_position_x: i32,
    cover_position_y: i32,
    cover_zoom: i32,
    is_admin: bool,
    is_superadmin: bool,
    post_count: i64,
}

#[derive(Serialize, FromRow)]
struct AdminConversation {
    id: i32,
    title: Option<String>,
    is_group: bool,
    owner: String,
    member_count: i64,
    is_muted: bool,
    latest_message: Option<String>,
    created_at: String,
}

#[derive(Serialize, FromRow)]
struct AdminReport {
    id: i64,
    reporter: String,
    post_id: i32,
    post_author: String,
    reason: String,
    details: String,
    created_at: String,
}

#[derive(Serialize, FromRow)]
struct AdminChatReport {
    id: i64,
    reporter: String,
    conversation_id: i32,
    details: String,
    created_at: String,
}

#[derive(Serialize, FromRow)]
struct AdminPost {
    id: i32,
    author: String,
    image_url: String,
    media_type: String,
    caption: String,
    created_at: String,
}

#[derive(Serialize)]
struct AdminOverview {
    users: Vec<AdminUser>,
    conversations: Vec<AdminConversation>,
    posts: Vec<AdminPost>,
    reports: Vec<AdminReport>,
    chat_reports: Vec<AdminChatReport>,
    config: AdminConfig,
    total_users: i64,
    total_posts: i64,
    total_messages: i64,
    total_groups: i64,
    total_reports: i64,
    total_conversations: i64,
    disabled_users: i64,
    is_superadmin: bool,
}

#[derive(Serialize, FromRow)]
struct AdminConfig {
    allow_signups: bool,
    app_enabled: bool,
}

#[derive(Deserialize)]
struct AdminUserUpdate {
    account_enabled: bool,
}

#[derive(Deserialize)]
struct ChatReportRequest {
    details: Option<String>,
}

#[derive(Deserialize)]
struct AdminConfigUpdate {
    allow_signups: Option<bool>,
    app_enabled: Option<bool>,
}

#[derive(Deserialize)]
struct AdminRoleUpdate {
    is_admin: bool,
}

#[derive(Deserialize)]
struct AdminConversationUpdate {
    is_muted: bool,
}

#[derive(Deserialize)]
struct MediaSearchQuery {
    q: String,
    kind: Option<String>,
}

#[derive(Deserialize)]
struct GiphyResponse {
    data: Vec<GiphyItem>,
}

#[derive(Deserialize)]
struct GiphyItem {
    title: String,
    images: GiphyImages,
}

#[derive(Deserialize)]
struct GiphyImages {
    fixed_width: Option<GiphyImage>,
    fixed_width_small: Option<GiphyImage>,
    original: Option<GiphyImage>,
}

#[derive(Deserialize)]
struct GiphyImage {
    url: String,
}

#[derive(Serialize)]
struct ChatMediaResult {
    title: String,
    preview_url: String,
    media_url: String,
    source_url: String,
}

#[derive(Deserialize)]
struct PhotonResponse {
    features: Vec<PhotonFeature>,
}

#[derive(Deserialize)]
struct PhotonFeature {
    properties: PhotonProperties,
}

#[derive(Deserialize)]
struct PhotonProperties {
    name: Option<String>,
    city: Option<String>,
    state: Option<String>,
    country: Option<String>,
}

#[derive(Serialize)]
struct LocationSuggestion {
    label: String,
}

#[derive(Serialize, FromRow)]
struct FollowedPerson {
    username: String,
    display_name: Option<String>,
    avatar_url: Option<String>,
    is_favorite: bool,
}

#[derive(Serialize, FromRow)]
struct CommunitySummary {
    id: i32,
    name: String,
    description: String,
    image_url: Option<String>,
    owner: Option<String>,
    is_member: bool,
    is_disabled: bool,
    is_owner: bool,
    is_admin: bool,
    member_count: i64,
    created_at: String,
}

#[derive(Serialize, FromRow)]
struct CommunityPost {
    id: i64,
    username: String,
    avatar_url: Option<String>,
    title: String,
    body: String,
    created_at: String,
    comment_count: i64,
    can_delete: bool,
    is_html: bool,
    edited: bool,
    can_edit: bool,
}

#[derive(Serialize, FromRow)]
struct CommunityComment {
    id: i64,
    username: String,
    body: String,
    created_at: String,
    can_delete: bool,
}

#[derive(Serialize, FromRow)]
struct CommunityMember {
    id: i32,
    username: String,
    is_owner: bool,
    is_admin: bool,
    enabled: bool,
}

#[derive(Deserialize)]
struct NewCommunity {
    name: String,
    description: Option<String>,
}

#[derive(Deserialize)]
struct AddCommunityMember {
    username: String,
}

#[derive(Deserialize)]
struct CommunityMemberStatus {
    enabled: bool,
}

#[derive(Deserialize)]
struct CommunityMemberAdmin {
    is_admin: bool,
}

#[derive(Deserialize)]
struct CommunityPostInput {
    title: String,
    body: String,
}

#[derive(Deserialize)]
struct CommunityCommentInput {
    body: String,
}

#[derive(Serialize, FromRow)]
struct DirectMessage {
    id: i64,
    sender: String,
    body: String,
    image_url: Option<String>,
    media_type: Option<String>,
    created_at: String,
    can_delete: bool,
}

#[derive(Serialize, FromRow)]
struct GroupMember {
    id: i32,
    username: String,
    is_owner: bool,
}

#[derive(Serialize, FromRow)]
struct GridPost {
    id: i32,
    image_url: String,
    media_type: String,
    caption: String,
    created_at: String,
}

#[derive(Serialize, FromRow)]
struct ProfileConnection {
    username: String,
    avatar_url: Option<String>,
}

#[derive(Serialize, FromRow)]
struct ProfileAdmirer {
    username: String,
    avatar_url: Option<String>,
}

#[derive(Serialize, FromRow)]
struct PersonalChatRequest {
    id: i64,
    username: String,
    status: String,
    incoming: bool,
    conversation_id: Option<i32>,
    requested_at: String,
}

#[derive(FromRow)]
struct ProfileRow {
    id: i32,
    username: String,
    display_name: Option<String>,
    bio: Option<String>,
    age: Option<i32>,
    date_of_birth: Option<String>,
    birthday_visibility: String,
    location: Option<String>,
    company_name: Option<String>,
    designation: Option<String>,
    work_visibility: String,
    contact_email: Option<String>,
    email_visibility: String,
    phone_number: Option<String>,
    phone_visibility: String,
    instagram_handle: Option<String>,
    facebook_handle: Option<String>,
    tiktok_handle: Option<String>,
    linkedin_handle: Option<String>,
    social_visibility: String,
    mood: Option<String>,
    interests: Option<String>,
    relationship_status: Option<String>,
    avatar_url: Option<String>,
    cover_image_url: Option<String>,
    post_count: i64,
    follower_count: i64,
    following_count: i64,
    followed_by_me: bool,
    admired_by_me: bool,
    admiration_count: Option<i64>,
}

#[derive(Serialize)]
struct Profile {
    username: String,
    display_name: Option<String>,
    bio: Option<String>,
    age: Option<i32>,
    date_of_birth: Option<String>,
    birthday_visibility: String,
    location: Option<String>,
    company_name: Option<String>,
    designation: Option<String>,
    work_visibility: String,
    contact_email: Option<String>,
    email_visibility: String,
    phone_number: Option<String>,
    phone_visibility: String,
    instagram_handle: Option<String>,
    facebook_handle: Option<String>,
    tiktok_handle: Option<String>,
    linkedin_handle: Option<String>,
    social_visibility: String,
    mood: Option<String>,
    interests: Option<String>,
    relationship_status: Option<String>,
    avatar_url: Option<String>,
    cover_image_url: Option<String>,
    post_count: i64,
    follower_count: i64,
    following_count: i64,
    followed_by_me: bool,
    admired_by_me: bool,
    admiration_count: Option<i64>,
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
    date_of_birth: Option<String>,
    birthday_visibility: Option<String>,
    location: Option<String>,
    company_name: Option<String>,
    designation: Option<String>,
    work_visibility: Option<String>,
    contact_email: Option<String>,
    email_visibility: Option<String>,
    phone_number: Option<String>,
    phone_visibility: Option<String>,
    instagram_handle: Option<String>,
    facebook_handle: Option<String>,
    tiktok_handle: Option<String>,
    linkedin_handle: Option<String>,
    social_visibility: Option<String>,
    mood: Option<String>,
    interests: Option<String>,
    relationship_status: Option<String>,
}

#[derive(Deserialize)]
struct FeedQuery {
    scope: Option<String>,
}

#[derive(Deserialize)]
struct CheckinQuery {
    category: Option<String>,
    scope: Option<String>,
    offset: Option<i64>,
}

#[derive(Deserialize)]
struct NewCheckin {
    category: String,
    place: String,
    note: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

#[derive(Deserialize)]
struct LocationUpdate {
    latitude: f64,
    longitude: f64,
}

#[derive(Deserialize)]
struct ConnectionsQuery {
    kind: String,
}

#[derive(Deserialize)]
struct NewComment {
    body: String,
}

#[derive(Deserialize)]
struct NewConversation {
    usernames: Vec<String>,
    title: Option<String>,
}

#[derive(Deserialize)]
struct GroupTitleRequest {
    title: String,
}

#[derive(Deserialize)]
struct UserSearchQuery {
    q: String,
}

#[derive(Serialize, FromRow)]
struct UserSearchResult {
    username: String,
    display_name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct PostReportRequest {
    reason: String,
    details: Option<String>,
}

#[derive(Serialize)]
struct NewConversationResponse {
    id: i32,
}

#[derive(Serialize)]
struct AcceptedChatRequest {
    conversation_id: i32,
}

#[derive(Serialize)]
struct NewTalkRequestResponse {
    id: i64,
}

#[derive(Serialize)]
struct NewCheckinResponse {
    id: i64,
}

fn username_character(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn mentioned_usernames(body: &str) -> Vec<String> {
    let chars: Vec<char> = body.chars().collect();
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] != '@' || (i > 0 && username_character(chars[i - 1])) {
            i += 1;
            continue;
        }

        let start = i + 1;
        let mut end = start;
        while end < chars.len() && username_character(chars[end]) {
            end += 1;
        }
        if end > start {
            let name: String = chars[start..end].iter().collect();
            if seen.insert(name.clone()) {
                names.push(name);
            }
        }

        i = end.max(i + 1);
    }

    names
}

fn tagged_usernames(value: &str) -> Result<Vec<String>, StatusCode> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    for name in value
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        if name.len() > 30 || !name.chars().all(username_character) {
            return Err(StatusCode::BAD_REQUEST);
        }
        if seen.insert(name.to_string()) {
            names.push(name.to_string());
        }
    }
    if names.len() > 10 {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(names)
}

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct SignupRequest {
    email: String,
    username: String,
    first_name: String,
    last_name: String,
    age: i32,
    bio: Option<String>,
    phone_number: Option<String>,
    password: String,
}

#[derive(Serialize)]
struct UserInfo {
    username: String,
    is_admin: bool,
    is_superadmin: bool,
    cover_image_url: Option<String>,
    cover_position_x: i32,
    cover_position_y: i32,
    cover_zoom: i32,
}

#[derive(Deserialize)]
struct CoverPositionUpdate {
    position_x: i32,
    position_y: i32,
    zoom: i32,
}

#[derive(Serialize, FromRow)]
struct UserSettings {
    profile_visibility: String,
    message_privacy: String,
    show_activity_status: bool,
    keep_profile_visitors: bool,
    notify_mentions: bool,
    account_enabled: bool,
}

#[derive(Deserialize)]
struct UserSettingsUpdate {
    profile_visibility: String,
    message_privacy: String,
    show_activity_status: bool,
    keep_profile_visitors: bool,
    notify_mentions: bool,
    account_enabled: bool,
}

#[derive(Deserialize)]
struct BlockUserRequest {
    username: String,
}

#[derive(Serialize, FromRow)]
struct BlockedUser {
    username: String,
    avatar_url: Option<String>,
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
               u.mood,
               p.image_url,
               p.media_type,
               p.caption,
               CASE WHEN p.reshared_from_id IS NOT NULL
                    THEN original_user.username
               END AS original_author,
               (SELECT COUNT(*) FROM post_likes l WHERE l.post_id = p.id) AS likes,
               EXISTS (SELECT 1 FROM post_likes l
                       WHERE l.post_id = p.id AND l.user_id = $1) AS liked_by_me,
               (SELECT COUNT(*) FROM post_dislikes d WHERE d.post_id = p.id) AS dislikes,
               EXISTS (SELECT 1 FROM post_dislikes d
                       WHERE d.post_id = p.id AND d.user_id = $1) AS disliked_by_me,
               (SELECT COUNT(*) FROM post_hates h WHERE h.post_id = p.id) AS hates,
               EXISTS (SELECT 1 FROM post_hates h
                       WHERE h.post_id = p.id AND h.user_id = $1) AS hated_by_me,
               (SELECT COUNT(*) FROM posts shares
                WHERE shares.reshared_from_id = COALESCE(p.reshared_from_id, p.id)) AS reshares,
               EXISTS (SELECT 1 FROM posts shares
                       WHERE shares.user_id = $1
                         AND shares.reshared_from_id = COALESCE(p.reshared_from_id, p.id)) AS reshared_by_me,
               EXISTS (SELECT 1 FROM follows f
                       WHERE f.follower_id = $1 AND f.followee_id = u.id) AS followed_by_me,
               EXISTS (SELECT 1 FROM user_favorites f
                       WHERE f.user_id = $1 AND f.favorite_user_id = u.id) AS favorited_by_me,
               (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) AS comment_count,
               COALESCE((SELECT array_agg(t.username ORDER BY t.username)
                         FROM post_tags pt JOIN users t ON t.id = pt.user_id
                         WHERE pt.post_id = p.id), ARRAY[]::TEXT[]) AS tags
        FROM posts p
        JOIN users u ON u.id = p.user_id
        LEFT JOIN posts original_post ON original_post.id = p.reshared_from_id
        LEFT JOIN users original_user ON original_user.id = original_post.user_id
        WHERE (NOT $2
               OR p.user_id = $1
               OR p.user_id IN (SELECT followee_id FROM follows WHERE follower_id = $1))
          AND (
              u.id = $1
              OR (
                  u.account_enabled
                  AND NOT EXISTS (
                      SELECT 1 FROM user_blocks b
                      WHERE (b.blocker_id = u.id AND b.blocked_id = $1)
                         OR (b.blocker_id = $1 AND b.blocked_id = u.id)
                  )
                  AND (
                      u.profile_visibility = 'public'
                      OR (
                          u.profile_visibility = 'followers'
                          AND EXISTS (
                              SELECT 1 FROM follows f
                              WHERE f.follower_id = $1 AND f.followee_id = u.id
                          )
                      )
                  )
              )
          )
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

async fn active_users(State(pool): State<PgPool>) -> Result<Json<Vec<ActiveUser>>, StatusCode> {
    let users = sqlx::query_as::<_, ActiveUser>(
        r#"
        SELECT username, avatar_url, mood
        FROM users
        WHERE last_active_at > now() - interval '2 minutes'
          AND account_enabled
          AND show_activity_status
        ORDER BY last_active_at DESC, username
        LIMIT 30
        "#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("active users query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(users))
}

async fn record_presence(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    sqlx::query("UPDATE users SET last_active_at = now() WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("presence update failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

fn coordinates_are_valid(latitude: f64, longitude: f64) -> bool {
    latitude.is_finite()
        && longitude.is_finite()
        && (-90.0..=90.0).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
}

async fn list_checkins(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(query): Query<CheckinQuery>,
) -> Result<Json<Vec<ActivityCheckin>>, StatusCode> {
    if query
        .category
        .as_deref()
        .is_some_and(|category| !checkin_category_is_valid(category))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    if query
        .scope
        .as_deref()
        .is_some_and(|scope| !matches!(scope, "all" | "connections"))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let offset = query.offset.unwrap_or(0);
    if !(0..=i64::MAX - 50).contains(&offset) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let viewer_id = current_user_id(&pool, &headers).await.ok();
    let checkins = sqlx::query_as::<_, ActivityCheckin>(
        r#"
        SELECT c.id,
               u.username,
               u.avatar_url,
               c.category,
               c.place,
               c.note,
               c.latitude,
               c.longitude,
               EXISTS (SELECT 1 FROM follows f
                       WHERE f.follower_id = $2 AND f.followee_id = u.id) AS is_following,
               EXISTS (SELECT 1 FROM follows f
                       WHERE f.follower_id = u.id AND f.followee_id = $2) AS is_follower,
               to_char(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at
        FROM activity_checkins c
        JOIN users u ON u.id = c.user_id
        WHERE ($1::TEXT IS NULL OR c.category = $1)
          AND (NOT $4::BOOLEAN
               OR u.id = $2
               OR EXISTS (SELECT 1 FROM follows f
                          WHERE f.follower_id = $2 AND f.followee_id = u.id)
               OR EXISTS (SELECT 1 FROM follows f
                          WHERE f.follower_id = u.id AND f.followee_id = $2))
        ORDER BY c.created_at DESC, c.id DESC
        LIMIT 50 OFFSET $3
        "#,
    )
    .bind(query.category.as_deref())
    .bind(viewer_id.unwrap_or(0))
    .bind(offset)
    .bind(query.scope.as_deref() == Some("connections"))
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("activity check-in list failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(checkins))
}

async fn create_checkin(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(checkin): Json<NewCheckin>,
) -> Result<(StatusCode, Json<NewCheckinResponse>), StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    if !checkin_category_is_valid(&checkin.category) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let place = clean(Some(checkin.place), 100)?.ok_or(StatusCode::BAD_REQUEST)?;
    let note = clean(checkin.note, 240)?;
    let coordinates = match (checkin.latitude, checkin.longitude) {
        (Some(latitude), Some(longitude)) if coordinates_are_valid(latitude, longitude) => {
            Some((latitude, longitude))
        }
        (None, None) => None,
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO activity_checkins (user_id, category, place, note, latitude, longitude)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(user_id)
    .bind(&checkin.category)
    .bind(place)
    .bind(note)
    .bind(coordinates.map(|(latitude, _)| latitude))
    .bind(coordinates.map(|(_, longitude)| longitude))
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("activity check-in create failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((StatusCode::CREATED, Json(NewCheckinResponse { id })))
}

async fn delete_checkin(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(checkin_id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query("DELETE FROM activity_checkins WHERE id = $1 AND user_id = $2")
        .bind(checkin_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("activity check-in delete failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn update_live_location(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(location): Json<LocationUpdate>,
) -> Result<StatusCode, StatusCode> {
    if !coordinates_are_valid(location.latitude, location.longitude) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let user_id = current_user_id(&pool, &headers).await?;
    sqlx::query("DELETE FROM live_locations WHERE updated_at <= now() - interval '90 seconds'")
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("expired live location cleanup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    sqlx::query(
        "INSERT INTO live_locations (user_id, latitude, longitude, updated_at)
         VALUES ($1, $2, $3, now())
         ON CONFLICT (user_id) DO UPDATE
         SET latitude = EXCLUDED.latitude,
             longitude = EXCLUDED.longitude,
             updated_at = now()
         WHERE live_locations.updated_at <= now() - interval '5 seconds'",
    )
    .bind(user_id)
    .bind(location.latitude)
    .bind(location.longitude)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("live location update failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_live_location(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    sqlx::query("DELETE FROM live_locations WHERE user_id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("live location removal failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stop_live_location_beacon(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    sqlx::query("DELETE FROM live_locations WHERE user_id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("live location beacon removal failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_live_locations(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<Vec<LiveLocation>>, StatusCode> {
    let viewer_id = current_user_id(&pool, &headers).await?;
    sqlx::query("DELETE FROM live_locations WHERE updated_at <= now() - interval '90 seconds'")
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("expired live location cleanup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let locations = sqlx::query_as::<_, LiveLocation>(
        r#"
        SELECT u.username,
               u.avatar_url,
               u.mood,
               l.latitude,
               l.longitude,
               to_char(l.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS updated_at
        FROM live_locations l
        JOIN users u ON u.id = l.user_id
        WHERE l.updated_at > now() - interval '90 seconds'
          AND u.account_enabled
          AND NOT EXISTS (
              SELECT 1 FROM user_blocks b
              WHERE (b.blocker_id = u.id AND b.blocked_id = $1)
                 OR (b.blocker_id = $1 AND b.blocked_id = u.id)
          )
        ORDER BY l.updated_at DESC
        "#,
    )
    .bind(viewer_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("live location list failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(locations))
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
    Json(c): Json<SignupRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let signups_enabled: bool = sqlx::query_scalar(
        "SELECT value = 'true' FROM app_settings WHERE key = 'allow_signups'",
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        eprintln!("signup setting lookup failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .unwrap_or(true);
    if !signups_enabled {
        return Err(StatusCode::FORBIDDEN);
    }
    let email = c.email.trim().to_ascii_lowercase();
    let first_name = c.first_name.trim();
    let last_name = c.last_name.trim();
    let bio = c.bio.as_deref().unwrap_or("").trim();
    let phone_number = c.phone_number.as_deref().unwrap_or("").trim();
    let name_ok = c.username.len() >= 3
        && c.username.len() <= 30
        && c.username
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    let email_ok = email.len() <= 254
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && !domain.contains('@')
                && domain
                    .rsplit_once('.')
                    .is_some_and(|(host, suffix)| !host.is_empty() && !suffix.is_empty())
        });
    let person_name_ok = !first_name.is_empty()
        && first_name.chars().count() <= 50
        && !last_name.is_empty()
        && last_name.chars().count() <= 50;
    let phone_ok = phone_number.len() <= 30
        && phone_number
            .chars()
            .all(|ch| ch.is_ascii_digit() || "+()- ".contains(ch));
    if !name_ok
        || !email_ok
        || !person_name_ok
        || !(13..=120).contains(&c.age)
        || bio.chars().count() > 300
        || !phone_ok
        || !(8..=128).contains(&c.password.len())
    {
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

    let display_name = format!("{first_name} {last_name}");
    let row: Result<(i32,), sqlx::Error> = sqlx::query_as(
        "INSERT INTO users
         (username, password_hash, email, first_name, last_name, display_name, age, bio, phone_number)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULLIF($9, ''))
         RETURNING id",
    )
    .bind(&c.username)
    .bind(&hash)
    .bind(&email)
    .bind(first_name)
    .bind(last_name)
    .bind(&display_name)
    .bind(c.age)
    .bind(bio)
    .bind(phone_number)
    .fetch_one(&pool)
    .await;

    let user_id = match row {
        Ok((id,)) => id,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(StatusCode::CONFLICT); // username or email already registered
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
        Json(UserInfo {
            username: c.username,
            is_admin: false,
            is_superadmin: false,
            cover_image_url: None,
            cover_position_x: 50,
            cover_position_y: 50,
            cover_zoom: 100,
        }),
    ))
}

async fn login(
    State(pool): State<PgPool>,
    Json(c): Json<Credentials>,
) -> Result<impl IntoResponse, StatusCode> {
    let row: Option<(i32, Option<String>, Option<String>, i32, i32, i32, bool, bool)> = sqlx::query_as(
        "SELECT id, password_hash, cover_image_url, cover_position_x, cover_position_y, cover_zoom, is_admin, is_superadmin
             FROM users WHERE username = $1 AND account_enabled = TRUE",
    )
    .bind(&c.username)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        eprintln!("login query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Same 401 for "no such user" and "wrong password", so attackers can't tell which.
    let (user_id, stored_hash, cover_image_url, cover_position_x, cover_position_y, cover_zoom, is_admin, is_superadmin) =
        match row {
            Some((id, Some(h), cover_image_url, position_x, position_y, zoom, is_admin, is_superadmin)) => {
                (id, h, cover_image_url, position_x, position_y, zoom, is_admin, is_superadmin)
            }
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
        Json(UserInfo {
            username: c.username,
            is_admin,
            is_superadmin,
            cover_image_url,
            cover_position_x,
            cover_position_y,
            cover_zoom,
        }),
    ))
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    cookies
        .split(';')
        .find_map(|part| part.trim().strip_prefix("session=").map(|v| v.to_string()))
}

fn checkin_category_is_valid(category: &str) -> bool {
    matches!(
        category,
        "nightclubs"
            | "bars"
            | "pubs"
            | "restaurants"
            | "hotels"
            | "movies"
            | "concerts"
            | "travel"
            | "dining"
            | "outdoors"
    )
}

async fn me(State(pool): State<PgPool>, headers: HeaderMap) -> Result<Json<UserInfo>, StatusCode> {
    let token = session_token(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let row: Option<(String, Option<String>, i32, i32, i32, bool, bool)> = sqlx::query_as(
        "SELECT u.username, u.cover_image_url, u.cover_position_x, u.cover_position_y, u.cover_zoom, u.is_admin, u.is_superadmin
         FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token = $1 AND s.created_at > now() - interval '7 days' AND u.account_enabled",
    )
    .bind(&token)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        eprintln!("me query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match row {
        Some((username, cover_image_url, cover_position_x, cover_position_y, cover_zoom, is_admin, is_superadmin)) => {
            Ok(Json(UserInfo {
                username,
                is_admin,
                is_superadmin,
                cover_image_url,
                cover_position_x,
                cover_position_y,
                cover_zoom,
            }))
        }
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
        [(
            header::SET_COOKIE,
            "session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
        )],
    )
}

async fn current_user_id(pool: &PgPool, headers: &HeaderMap) -> Result<i32, StatusCode> {
    let token = session_token(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let row: Option<(i32,)> = sqlx::query_as(
        "SELECT s.user_id FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token = $1 AND s.created_at > now() - interval '7 days' AND u.account_enabled",
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

async fn require_admin(pool: &PgPool, headers: &HeaderMap) -> Result<i32, StatusCode> {
    let user_id = current_user_id(pool, headers).await?;
    let is_admin: bool = sqlx::query_scalar("SELECT is_admin FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            eprintln!("admin role lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !is_admin {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(user_id)
}

async fn require_superadmin(pool: &PgPool, headers: &HeaderMap) -> Result<i32, StatusCode> {
    let user_id = current_user_id(pool, headers).await?;
    let is_superadmin: bool = sqlx::query_scalar("SELECT is_superadmin FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .map_err(|e| {
            eprintln!("superadmin role lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !is_superadmin {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(user_id)
}

async fn app_status(State(pool): State<PgPool>) -> Result<Json<bool>, StatusCode> {
    let enabled = sqlx::query_scalar(
        "SELECT COALESCE((SELECT value = 'true' FROM app_settings WHERE key = 'app_enabled'), TRUE)",
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("app status lookup failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(enabled))
}

async fn app_enabled_guard(
    State(pool): State<PgPool>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if matches!(path, "/api/login" | "/api/me" | "/api/status" | "/api/logout") {
        return next.run(request).await;
    }
    let enabled: Result<bool, sqlx::Error> = sqlx::query_scalar(
        "SELECT COALESCE((SELECT value = 'true' FROM app_settings WHERE key = 'app_enabled'), TRUE)",
    )
    .fetch_one(&pool)
    .await;
    match enabled {
        Ok(true) => return next.run(request).await,
        Ok(false) => {}
        Err(error) => {
            eprintln!("app access status lookup failed: {error}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    let token = session_token(request.headers());
    let is_superadmin = if let Some(token) = token {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token = $1 AND s.created_at > now() - interval '7 days' AND u.account_enabled AND u.is_superadmin)",
        )
        .bind(token)
        .fetch_one(&pool)
        .await
        .unwrap_or(false)
    } else {
        false
    };
    if is_superadmin {
        next.run(request).await
    } else {
        StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}

async fn admin_overview(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<AdminOverview>, StatusCode> {
    let admin_id = require_admin(&pool, &headers).await?;
    let is_superadmin: bool = sqlx::query_scalar("SELECT is_superadmin FROM users WHERE id = $1")
        .bind(admin_id)
        .fetch_one(&pool)
        .await
        .map_err(|e| {
            eprintln!("admin privilege lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let config = sqlx::query_as::<_, AdminConfig>(
        "SELECT (SELECT value = 'true' FROM app_settings WHERE key = 'allow_signups') AS allow_signups, COALESCE((SELECT value = 'true' FROM app_settings WHERE key = 'app_enabled'), TRUE) AS app_enabled",
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("admin configuration query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let users = sqlx::query_as::<_, AdminUser>(
        r#"SELECT u.id, u.username, u.email, u.phone_number,
                  to_char(u.date_of_birth, 'YYYY-MM-DD') AS date_of_birth,
                  u.company_name, u.designation, u.contact_email,
                  u.instagram_handle, u.facebook_handle, u.tiktok_handle, u.linkedin_handle,
                  u.birthday_visibility, u.work_visibility, u.email_visibility,
                  u.phone_visibility, u.social_visibility,
                  u.display_name, u.age, u.bio, u.location, u.mood, u.interests,
                  u.relationship_status, u.avatar_url, u.cover_image_url,
                  to_char(u.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
                  u.account_enabled, u.profile_visibility, u.message_privacy,
                  u.show_activity_status, u.keep_profile_visitors, u.notify_mentions,
                  u.cover_position_x, u.cover_position_y, u.cover_zoom, u.is_admin, u.is_superadmin,
                  (SELECT COUNT(*) FROM posts p WHERE p.user_id = u.id) AS post_count
           FROM users u ORDER BY u.created_at DESC, u.id DESC"#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("admin users query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let conversations = sqlx::query_as::<_, AdminConversation>(
        r#"SELECT c.id, c.title, c.is_group, owner.username AS owner,
                  (SELECT COUNT(*) FROM conversation_members cm WHERE cm.conversation_id = c.id) AS member_count,
                  c.is_muted,
                  (SELECT NULLIF(m.body, '') FROM direct_messages m WHERE m.conversation_id = c.id ORDER BY m.id DESC LIMIT 1) AS latest_message,
                  to_char(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at
           FROM conversations c JOIN users owner ON owner.id = c.created_by
           ORDER BY c.created_at DESC, c.id DESC LIMIT 500"#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("admin conversations query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let posts = sqlx::query_as::<_, AdminPost>(
        r#"SELECT p.id, u.username AS author, p.image_url, p.media_type, p.caption,
                  to_char(p.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at
           FROM posts p JOIN users u ON u.id = p.user_id
           ORDER BY p.created_at DESC, p.id DESC LIMIT 500"#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("admin posts query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let reports = sqlx::query_as::<_, AdminReport>(
        r#"SELECT r.id, reporter.username AS reporter, r.post_id,
                  author.username AS post_author, r.reason, r.details,
                  to_char(r.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at
           FROM post_reports r
           JOIN users reporter ON reporter.id = r.reporter_id
           JOIN posts p ON p.id = r.post_id
           JOIN users author ON author.id = p.user_id
           ORDER BY r.created_at DESC LIMIT 200"#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("admin reports query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let chat_reports = sqlx::query_as::<_, AdminChatReport>(
        r#"SELECT r.id, u.username AS reporter, r.conversation_id, r.details,
                  to_char(r.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at
           FROM conversation_reports r JOIN users u ON u.id = r.reporter_id
           ORDER BY r.created_at DESC LIMIT 200"#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("admin chat reports query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let (total_users, total_posts, total_messages, total_groups, total_reports, total_conversations, disabled_users): (i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        r#"SELECT (SELECT COUNT(*) FROM users), (SELECT COUNT(*) FROM posts),
                  (SELECT COUNT(*) FROM direct_messages),
                  (SELECT COUNT(*) FROM conversations WHERE is_group),
                  (SELECT (SELECT COUNT(*) FROM post_reports) + (SELECT COUNT(*) FROM conversation_reports)),
                  (SELECT COUNT(*) FROM conversations),
                  (SELECT COUNT(*) FROM users WHERE NOT account_enabled)"#,
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("admin metrics query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(AdminOverview {
        users,
        conversations,
        posts,
        reports,
        chat_reports,
        config,
        total_users,
        total_posts,
        total_messages,
        total_groups,
        total_reports,
        total_conversations,
        disabled_users,
        is_superadmin,
    }))
}

async fn admin_update_config(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(update): Json<AdminConfigUpdate>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&pool, &headers).await?;
    if update.app_enabled.is_some() {
        require_superadmin(&pool, &headers).await?;
    }
    if let Some(allow_signups) = update.allow_signups {
        sqlx::query("INSERT INTO app_settings (key, value) VALUES ('allow_signups', $1) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()")
            .bind(if allow_signups { "true" } else { "false" })
            .execute(&pool)
            .await
            .map_err(|e| { eprintln!("signup configuration update failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR })?;
    }
    if let Some(app_enabled) = update.app_enabled {
        sqlx::query("INSERT INTO app_settings (key, value) VALUES ('app_enabled', $1) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()")
            .bind(if app_enabled { "true" } else { "false" })
            .execute(&pool)
            .await
            .map_err(|e| { eprintln!("app state update failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR })?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_update_role(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(target_id): Path<i32>,
    Json(update): Json<AdminRoleUpdate>,
) -> Result<StatusCode, StatusCode> {
    let superadmin_id = require_superadmin(&pool, &headers).await?;
    if target_id == superadmin_id {
        return Err(StatusCode::BAD_REQUEST);
    }
    let result = sqlx::query(
        "UPDATE users SET is_admin = $1, is_superadmin = CASE WHEN $1 THEN is_superadmin ELSE FALSE END WHERE id = $2",
    )
    .bind(update.is_admin)
    .bind(target_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("admin role update failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_delete_user(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(target_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let superadmin_id = require_superadmin(&pool, &headers).await?;
    if target_id == superadmin_id {
        return Err(StatusCode::BAD_REQUEST);
    }
    let result = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(target_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("admin user removal failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_delete_conversation(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(conversation_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    require_superadmin(&pool, &headers).await?;
    let result = sqlx::query("DELETE FROM conversations WHERE id = $1")
        .bind(conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("admin conversation removal failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_update_conversation(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(conversation_id): Path<i32>,
    Json(update): Json<AdminConversationUpdate>,
) -> Result<StatusCode, StatusCode> {
    require_admin(&pool, &headers).await?;
    let result = sqlx::query("UPDATE conversations SET is_muted = $1 WHERE id = $2")
        .bind(update.is_muted)
        .bind(conversation_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("admin conversation moderation failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn search_chat_media(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(query): Query<MediaSearchQuery>,
) -> Result<Json<Vec<ChatMediaResult>>, StatusCode> {
    current_user_id(&pool, &headers).await?;
    let term = query.q.trim();
    if term.is_empty() || term.chars().count() > 80 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let is_sticker = query.kind.as_deref() == Some("stickers");
    let Ok(api_key) = std::env::var("GIPHY_API_KEY") else {
        return search_commons_chat_media(term, is_sticker).await.map(Json);
    };
    let endpoint = if is_sticker { "stickers" } else { "gifs" };
    let mut request_url = reqwest::Url::parse(&format!("https://api.giphy.com/v1/{endpoint}/search"))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    request_url.query_pairs_mut()
        .append_pair("api_key", &api_key)
        .append_pair("q", term)
        .append_pair("limit", "24")
        .append_pair("rating", "pg-13");
    let response = reqwest::Client::new()
        .get(request_url)
        .send()
        .await
        .map_err(|e| {
            eprintln!("GIPHY search request failed: {e}");
            StatusCode::BAD_GATEWAY
        })?;
    if !response.status().is_success() {
        eprintln!("GIPHY search returned status {}", response.status());
        return Err(StatusCode::BAD_GATEWAY);
    }
    let response_bytes = response.bytes().await.map_err(|e| {
        eprintln!("GIPHY response read failed: {e}");
        StatusCode::BAD_GATEWAY
    })?;
    let results = serde_json::from_slice::<GiphyResponse>(&response_bytes)
        .map_err(|e| {
            eprintln!("GIPHY response decode failed: {e}");
            StatusCode::BAD_GATEWAY
        })?
        .data
        .into_iter()
        .filter_map(|item| {
            let preview = item.images.fixed_width_small.or(item.images.fixed_width)?;
            let media = item.images.original?;
            Some(ChatMediaResult { title: item.title, preview_url: preview.url, media_url: media.url, source_url: "https://giphy.com/".to_string() })
        })
        .collect();
    Ok(Json(results))
}

async fn search_commons_chat_media(term: &str, is_sticker: bool) -> Result<Vec<ChatMediaResult>, StatusCode> {
    let file_type = if is_sticker { "filemime:image/png" } else { "filemime:image/gif" };
    let mut url = reqwest::Url::parse("https://commons.wikimedia.org/w/api.php")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    url.query_pairs_mut()
        .append_pair("action", "query")
        .append_pair("generator", "search")
        .append_pair("gsrsearch", &format!("{} {file_type}", term))
        .append_pair("gsrnamespace", "6")
        .append_pair("gsrlimit", "24")
        .append_pair("prop", "imageinfo")
        .append_pair("iiprop", "url")
        .append_pair("iiurlwidth", "250")
        .append_pair("format", "json");
    let response = reqwest::Client::new()
        .get(url)
        .header(reqwest::header::USER_AGENT, "CommunityInstaClone/1.0 (chat media search)")
        .send()
        .await
        .map_err(|e| { eprintln!("Wikimedia chat media search failed: {e}"); StatusCode::BAD_GATEWAY })?;
    if !response.status().is_success() { return Err(StatusCode::BAD_GATEWAY); }
    let bytes = response.bytes().await
        .map_err(|e| { eprintln!("Wikimedia chat media response read failed: {e}"); StatusCode::BAD_GATEWAY })?;
    let data = serde_json::from_slice::<serde_json::Value>(&bytes)
        .map_err(|e| { eprintln!("Wikimedia chat media response decode failed: {e}"); StatusCode::BAD_GATEWAY })?;
    let results = data.get("query").and_then(|q| q.get("pages")).and_then(|p| p.as_object())
        .into_iter().flatten().filter_map(|(_, page)| {
            let info = page.get("imageinfo")?.as_array()?.first()?;
            let media_url = info.get("url")?.as_str()?;
            let preview_url = info.get("thumburl").and_then(|v| v.as_str()).unwrap_or(media_url);
            let source_url = info.get("descriptionurl")?.as_str()?;
            Some(ChatMediaResult {
                title: page.get("title").and_then(|v| v.as_str()).unwrap_or("Community media").to_string(),
                preview_url: preview_url.to_string(),
                media_url: media_url.to_string(),
                source_url: source_url.to_string(),
            })
        }).collect();
    Ok(results)
}

async fn search_locations(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(query): Query<MediaSearchQuery>,
) -> Result<Json<Vec<LocationSuggestion>>, StatusCode> {
    current_user_id(&pool, &headers).await?;
    let term = query.q.trim();
    if term.chars().count() < 2 || term.chars().count() > 100 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut url = reqwest::Url::parse("https://photon.komoot.io/api/")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    url.query_pairs_mut()
        .append_pair("q", term)
        .append_pair("limit", "8")
        .append_pair("lang", "en");
    let response = reqwest::Client::new()
        .get(url)
        .header(reqwest::header::USER_AGENT, "CommunityInstaClone/1.0 profile-location-search")
        .send()
        .await
        .map_err(|e| {
            eprintln!("location lookup request failed: {e}");
            StatusCode::BAD_GATEWAY
        })?;
    if !response.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }
    let bytes = response.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let results = serde_json::from_slice::<PhotonResponse>(&bytes)
        .map_err(|e| {
            eprintln!("location lookup response decode failed: {e}");
            StatusCode::BAD_GATEWAY
        })?
        .features
        .into_iter()
        .filter_map(|feature| {
            let props = feature.properties;
            let place = props.name.or(props.city)?;
            let mut parts = vec![place];
            if let Some(state) = props.state.filter(|value| !parts.contains(value)) {
                parts.push(state);
            }
            if let Some(country) = props.country.filter(|value| !parts.contains(value)) {
                parts.push(country);
            }
            Some(LocationSuggestion { label: parts.join(", ") })
        })
        .collect();
    Ok(Json(results))
}

async fn admin_update_user(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(target_id): Path<i32>,
    Json(update): Json<AdminUserUpdate>,
) -> Result<StatusCode, StatusCode> {
    let admin_id = require_admin(&pool, &headers).await?;
    if target_id == admin_id {
        return Err(StatusCode::BAD_REQUEST);
    }
    let result = sqlx::query("UPDATE users SET account_enabled = $1 WHERE id = $2")
        .bind(update.account_enabled)
        .bind(target_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("admin user update failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn can_view_profile(
    pool: &PgPool,
    viewer_id: Option<i32>,
    profile_id: i32,
) -> Result<bool, StatusCode> {
    sqlx::query_scalar(
        r#"
        SELECT
            u.id = $2
            OR (
                u.account_enabled
                AND u.profile_visibility <> 'private'
                AND NOT EXISTS (
                    SELECT 1 FROM user_blocks b
                    WHERE (b.blocker_id = u.id AND b.blocked_id = $2)
                       OR (b.blocker_id = $2 AND b.blocked_id = u.id)
                )
                AND (
                    u.profile_visibility = 'public'
                    OR EXISTS (
                        SELECT 1 FROM follows f
                        WHERE f.follower_id = $2 AND f.followee_id = u.id
                    )
                )
            )
        FROM users u
        WHERE u.id = $1
        "#,
    )
    .bind(profile_id)
    .bind(viewer_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        eprintln!("profile visibility check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)
}

async fn get_user_settings(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<UserSettings>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let settings = sqlx::query_as::<_, UserSettings>(
        "SELECT profile_visibility, message_privacy, show_activity_status,
                keep_profile_visitors, notify_mentions, account_enabled
         FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("user settings query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(settings))
}

async fn update_user_settings(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(settings): Json<UserSettingsUpdate>,
) -> Result<Json<UserSettings>, StatusCode> {
    if !matches!(
        settings.profile_visibility.as_str(),
        "public" | "followers" | "private"
    ) || !matches!(
        settings.message_privacy.as_str(),
        "everyone" | "following" | "nobody"
    ) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let user_id = current_user_id(&pool, &headers).await?;
    let updated = sqlx::query_as::<_, UserSettings>(
        "UPDATE users
         SET profile_visibility = $2, message_privacy = $3,
             show_activity_status = $4, keep_profile_visitors = $5,
             notify_mentions = $6, account_enabled = $7
         WHERE id = $1
         RETURNING profile_visibility, message_privacy, show_activity_status,
                   keep_profile_visitors, notify_mentions, account_enabled",
    )
    .bind(user_id)
    .bind(settings.profile_visibility)
    .bind(settings.message_privacy)
    .bind(settings.show_activity_status)
    .bind(settings.keep_profile_visitors)
    .bind(settings.notify_mentions)
    .bind(settings.account_enabled)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("user settings update failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(updated))
}

async fn list_blocked_users(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<Vec<BlockedUser>>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let blocked = sqlx::query_as::<_, BlockedUser>(
        "SELECT u.username, u.avatar_url
         FROM user_blocks b JOIN users u ON u.id = b.blocked_id
         WHERE b.blocker_id = $1 ORDER BY u.username",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("blocked users query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(blocked))
}

async fn block_user(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(request): Json<BlockUserRequest>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let username = request.username.trim();
    if username.is_empty() || username.len() > 30 || !username.chars().all(username_character) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let target: Option<(i32,)> =
        sqlx::query_as("SELECT id FROM users WHERE LOWER(username) = LOWER($1)")
            .bind(username)
            .fetch_optional(&pool)
            .await
            .map_err(|e| {
                eprintln!("blocked user lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let (target_id,) = target.ok_or(StatusCode::NOT_FOUND)?;
    if target_id == user_id {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut tx = pool.begin().await.map_err(|e| {
        eprintln!("user block transaction failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query(
        "INSERT INTO user_blocks (blocker_id, blocked_id) VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(target_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        eprintln!("user block insert failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query(
        "DELETE FROM follows
         WHERE (follower_id = $1 AND followee_id = $2)
            OR (follower_id = $2 AND followee_id = $1)",
    )
    .bind(user_id)
    .bind(target_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        eprintln!("blocked user follow cleanup failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query(
        "DELETE FROM user_favorites
         WHERE (user_id = $1 AND favorite_user_id = $2)
            OR (user_id = $2 AND favorite_user_id = $1)",
    )
    .bind(user_id)
    .bind(target_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        eprintln!("blocked user favorite cleanup failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query(
        "DELETE FROM profile_admirations
         WHERE (admirer_id = $1 AND recipient_id = $2)
            OR (admirer_id = $2 AND recipient_id = $1)",
    )
    .bind(user_id)
    .bind(target_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        eprintln!("blocked user admiration cleanup failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query(
        "DELETE FROM personal_chat_requests
         WHERE (sender_id = $1 AND recipient_id = $2)
            OR (sender_id = $2 AND recipient_id = $1)",
    )
    .bind(user_id)
    .bind(target_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        eprintln!("blocked user request cleanup failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    tx.commit().await.map_err(|e| {
        eprintln!("user block commit failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unblock_user(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    sqlx::query(
        "DELETE FROM user_blocks
         WHERE blocker_id = $1
           AND blocked_id = (SELECT id FROM users WHERE LOWER(username) = LOWER($2))",
    )
    .bind(user_id)
    .bind(username)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("user unblock failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
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

fn video_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        Some("mp4")
    } else if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        Some("webm")
    } else {
        None
    }
}

// Saves an image under a random name and returns its URL (like "/uploads/<id>.jpg").
// The extension is chosen by us, so uploads can't overwrite each other or sneak in
// a name like "../something".
async fn save_image(bytes: &[u8], ext: &str) -> Result<String, StatusCode> {
    let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    tokio::fs::create_dir_all("static/uploads")
        .await
        .map_err(|e| {
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

async fn save_private_image(bytes: &[u8], ext: &str) -> Result<String, StatusCode> {
    let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    tokio::fs::create_dir_all("private_uploads")
        .await
        .map_err(|e| {
            eprintln!("could not create private uploads folder: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    tokio::fs::write(format!("private_uploads/{filename}"), bytes)
        .await
        .map_err(|e| {
            eprintln!("could not save private message image: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(filename)
}

// Deletes an old uploaded file, but only if the URL looks exactly like one we created.
async fn remove_upload(url: &str) {
    if let Some(name) = url.strip_prefix("/uploads/") {
        let safe =
            !name.is_empty() && !name.contains('/') && !name.contains('\\') && !name.contains("..");
        if safe {
            let _ = tokio::fs::remove_file(format!("static/uploads/{name}")).await;
        }
    }
}

async fn remove_private_image(filename: &str) {
    let safe = !filename.is_empty()
        && !filename.contains('/')
        && !filename.contains('\\')
        && !filename.contains("..");
    if safe {
        let _ = tokio::fs::remove_file(format!("private_uploads/{filename}")).await;
    }
}

async fn create_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let account_enabled: bool =
        sqlx::query_scalar("SELECT account_enabled FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .map_err(|e| {
                eprintln!("post account status lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    if !account_enabled {
        return Err(StatusCode::FORBIDDEN);
    }

    let mut caption = String::new();
    let mut media: Option<(Vec<u8>, &'static str, &'static str)> = None;
    let mut tags_input = String::new();

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
                if bytes.len() > 50 * 1024 * 1024 {
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
                let uploaded_media = if let Some(ext) = image_extension(&bytes) {
                    (bytes.to_vec(), ext, "image")
                } else if let Some(ext) = video_extension(&bytes) {
                    (bytes.to_vec(), ext, "video")
                } else {
                    return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
                };
                media = Some(uploaded_media);
            }
            Some("tags") => {
                tags_input = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            }
            _ => {}
        }
    }

    let (bytes, ext, media_type) = media.ok_or(StatusCode::BAD_REQUEST)?;
    let caption = caption.trim();
    if caption.len() > 500 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let tags = tagged_usernames(&tags_input)?;
    let tagged_users: Vec<(i32,)> = if tags.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as("SELECT id FROM users WHERE username = ANY($1)")
            .bind(&tags)
            .fetch_all(&pool)
            .await
            .map_err(|e| {
                eprintln!("post tag lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
    };
    if tagged_users.len() != tags.len() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let url = save_image(&bytes, ext).await?;
    let result: Result<(), sqlx::Error> = async {
        let mut tx = pool.begin().await?;
        let (post_id,): (i32,) = sqlx::query_as(
            "INSERT INTO posts (user_id, image_url, media_type, caption)
             VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(user_id)
        .bind(&url)
        .bind(media_type)
        .bind(caption)
        .fetch_one(&mut *tx)
        .await?;

        for (tagged_user_id,) in &tagged_users {
            sqlx::query("INSERT INTO post_tags (post_id, user_id) VALUES ($1, $2)")
                .bind(post_id)
                .bind(tagged_user_id)
                .execute(&mut *tx)
                .await?;
            if *tagged_user_id != user_id {
                sqlx::query(
                    "INSERT INTO activity_notifications (recipient_id, actor_id, post_id, kind)
                     VALUES ($1, $2, $3, 'photo_tag')",
                )
                .bind(tagged_user_id)
                .bind(user_id)
                .bind(post_id)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await
    }
    .await;
    if let Err(e) = result {
        remove_upload(&url).await;
        eprintln!("create post failed: {e}");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    Ok(StatusCode::CREATED)
}

async fn like_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(post_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let mut tx = pool.begin().await.map_err(|e| {
        eprintln!("like transaction failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query("DELETE FROM post_dislikes WHERE user_id = $1 AND post_id = $2")
        .bind(user_id)
        .bind(post_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            _ => {
                eprintln!("remove dislike before liking failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;
    sqlx::query("INSERT INTO post_likes (user_id, post_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(user_id)
        .bind(post_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_foreign_key_violation() => StatusCode::NOT_FOUND,
            _ => {
                eprintln!("like failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;
    tx.commit().await.map_err(|e| {
        eprintln!("like transaction commit failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
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

async fn dislike_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(post_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let mut tx = pool.begin().await.map_err(|e| {
        eprintln!("dislike transaction failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let result = sqlx::query(
        "INSERT INTO post_dislikes (user_id, post_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(post_id)
    .execute(&mut *tx)
    .await;
    if let Err(error) = result {
        return Err(match &error {
            sqlx::Error::Database(db) if db.is_foreign_key_violation() => StatusCode::NOT_FOUND,
            _ => {
                eprintln!("dislike failed: {error}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        });
    }
    sqlx::query("DELETE FROM post_likes WHERE user_id = $1 AND post_id = $2")
        .bind(user_id)
        .bind(post_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            eprintln!("remove like after dislike failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    tx.commit().await.map_err(|e| {
        eprintln!("dislike transaction commit failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn undislike_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(post_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    sqlx::query("DELETE FROM post_dislikes WHERE user_id = $1 AND post_id = $2")
        .bind(user_id)
        .bind(post_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("undislike failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn hate_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(post_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query(
        "INSERT INTO post_hates (user_id, post_id)
         VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(post_id)
    .execute(&pool)
    .await
    .map_err(|error| {
        if let sqlx::Error::Database(db_error) = &error
            && db_error.code().as_deref() == Some("23503")
        {
            return StatusCode::NOT_FOUND;
        }
        eprintln!("post hate failed: {error}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if result.rows_affected() == 0 {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM posts WHERE id = $1)")
            .bind(post_id)
            .fetch_one(&pool)
            .await
            .map_err(|e| {
                eprintln!("hated post lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !exists {
            return Err(StatusCode::NOT_FOUND);
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn unhate_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(post_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    sqlx::query("DELETE FROM post_hates WHERE user_id = $1 AND post_id = $2")
        .bind(user_id)
        .bind(post_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("remove post hate failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reshare_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(post_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query(
        r#"
        INSERT INTO posts (user_id, image_url, caption, media_type, reshared_from_id)
        SELECT $1, source.image_url, source.caption, source.media_type,
               COALESCE(source.reshared_from_id, source.id)
        FROM posts source
        WHERE source.id = $2
        "#,
    )
    .bind(user_id)
    .bind(post_id)
    .execute(&pool)
    .await;
    let result = match result {
        Ok(result) => result,
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            return Err(StatusCode::CONFLICT);
        }
        Err(error) => {
            eprintln!("reshare failed: {error}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::CREATED)
}

async fn undo_reshare(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(post_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query(
        r#"
        DELETE FROM posts
        WHERE user_id = $1
          AND reshared_from_id = (
              SELECT COALESCE(reshared_from_id, id)
              FROM posts
              WHERE id = $2
          )
        "#,
    )
    .bind(user_id)
    .bind(post_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("undo reshare failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn report_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(post_id): Path<i32>,
    Json(report): Json<PostReportRequest>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let reason = report.reason.trim();
    if !matches!(
        reason,
        "spam" | "harassment" | "hate" | "nudity" | "violence" | "scam" | "other"
    ) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let details = report.details.unwrap_or_default().trim().to_string();
    if details.chars().count() > 500 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let result = sqlx::query(
        "INSERT INTO post_reports (reporter_id, post_id, reason, details) VALUES ($1, $2, $3, $4)",
    )
    .bind(user_id)
    .bind(post_id)
    .bind(reason)
    .bind(details)
    .execute(&pool)
    .await;
    match result {
        Ok(_) => Ok(StatusCode::CREATED),
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
            Err(StatusCode::CONFLICT)
        }
        Err(sqlx::Error::Database(error)) if error.is_foreign_key_violation() => {
            Err(StatusCode::NOT_FOUND)
        }
        Err(error) => {
            eprintln!("post report insert failed: {error}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
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

    let mentioned = mentioned_usernames(body);
    let mentioned_users: Vec<(i32,)> = if mentioned.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as("SELECT id FROM users WHERE username = ANY($1)")
            .bind(&mentioned)
            .fetch_all(&pool)
            .await
            .map_err(|e| {
                eprintln!("comment mention lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
    };

    let mut tx = pool.begin().await.map_err(|e| {
        eprintln!("add comment transaction failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
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
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_foreign_key_violation() => StatusCode::NOT_FOUND,
        _ => {
            eprintln!("add comment failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    })?;

    for (recipient_id,) in mentioned_users {
        if recipient_id != user_id {
            sqlx::query(
                "INSERT INTO activity_notifications (recipient_id, actor_id, post_id, comment_id, kind)
                 VALUES ($1, $2, $3, $4, 'comment_mention')",
            )
            .bind(recipient_id)
            .bind(user_id)
            .bind(post_id)
            .bind(row.id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                eprintln!("comment notification insert failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
    }

    tx.commit().await.map_err(|e| {
        eprintln!("add comment commit failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
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
               CASE WHEN u.birthday_visibility = 'everyone' OR u.id = $2 OR
                    (u.birthday_visibility = 'friends' AND EXISTS (
                      SELECT 1 FROM follows a WHERE a.follower_id = $2 AND a.followee_id = u.id
                    ) AND EXISTS (
                      SELECT 1 FROM follows b WHERE b.follower_id = u.id AND b.followee_id = $2
                    )) THEN to_char(u.date_of_birth, 'YYYY-MM-DD') END AS date_of_birth,
               u.birthday_visibility,
               u.location,
               CASE WHEN u.work_visibility = 'everyone' OR u.id = $2 OR
                    (u.work_visibility = 'friends' AND EXISTS (
                      SELECT 1 FROM follows a WHERE a.follower_id = $2 AND a.followee_id = u.id
                    ) AND EXISTS (
                      SELECT 1 FROM follows b WHERE b.follower_id = u.id AND b.followee_id = $2
                    )) THEN u.company_name END AS company_name,
               CASE WHEN u.work_visibility = 'everyone' OR u.id = $2 OR
                    (u.work_visibility = 'friends' AND EXISTS (
                      SELECT 1 FROM follows a WHERE a.follower_id = $2 AND a.followee_id = u.id
                    ) AND EXISTS (
                      SELECT 1 FROM follows b WHERE b.follower_id = u.id AND b.followee_id = $2
                    )) THEN u.designation END AS designation,
               u.work_visibility,
               CASE WHEN u.email_visibility = 'everyone' OR u.id = $2 OR
                    (u.email_visibility = 'friends' AND EXISTS (
                      SELECT 1 FROM follows a WHERE a.follower_id = $2 AND a.followee_id = u.id
                    ) AND EXISTS (
                      SELECT 1 FROM follows b WHERE b.follower_id = u.id AND b.followee_id = $2
                    )) THEN u.contact_email END AS contact_email,
               u.email_visibility,
               CASE WHEN u.phone_visibility = 'everyone' OR u.id = $2 OR
                    (u.phone_visibility = 'friends' AND EXISTS (
                      SELECT 1 FROM follows a WHERE a.follower_id = $2 AND a.followee_id = u.id
                    ) AND EXISTS (
                      SELECT 1 FROM follows b WHERE b.follower_id = u.id AND b.followee_id = $2
                    )) THEN u.phone_number END AS phone_number,
               u.phone_visibility,
               CASE WHEN u.social_visibility = 'everyone' OR u.id = $2 OR
                    (u.social_visibility = 'friends' AND EXISTS (
                      SELECT 1 FROM follows a WHERE a.follower_id = $2 AND a.followee_id = u.id
                    ) AND EXISTS (
                      SELECT 1 FROM follows b WHERE b.follower_id = u.id AND b.followee_id = $2
                    )) THEN u.instagram_handle END AS instagram_handle,
               CASE WHEN u.social_visibility = 'everyone' OR u.id = $2 OR
                    (u.social_visibility = 'friends' AND EXISTS (
                      SELECT 1 FROM follows a WHERE a.follower_id = $2 AND a.followee_id = u.id
                    ) AND EXISTS (
                      SELECT 1 FROM follows b WHERE b.follower_id = u.id AND b.followee_id = $2
                    )) THEN u.facebook_handle END AS facebook_handle,
               CASE WHEN u.social_visibility = 'everyone' OR u.id = $2 OR
                    (u.social_visibility = 'friends' AND EXISTS (
                      SELECT 1 FROM follows a WHERE a.follower_id = $2 AND a.followee_id = u.id
                    ) AND EXISTS (
                      SELECT 1 FROM follows b WHERE b.follower_id = u.id AND b.followee_id = $2
                    )) THEN u.tiktok_handle END AS tiktok_handle,
               CASE WHEN u.social_visibility = 'everyone' OR u.id = $2 OR
                    (u.social_visibility = 'friends' AND EXISTS (
                      SELECT 1 FROM follows a WHERE a.follower_id = $2 AND a.followee_id = u.id
                    ) AND EXISTS (
                      SELECT 1 FROM follows b WHERE b.follower_id = u.id AND b.followee_id = $2
                    )) THEN u.linkedin_handle END AS linkedin_handle,
               u.social_visibility,
               u.mood,
               u.interests,
               u.relationship_status,
               u.avatar_url,
               u.cover_image_url,
               (SELECT COUNT(*) FROM posts WHERE user_id = u.id) AS post_count,
               (SELECT COUNT(*) FROM follows WHERE followee_id = u.id) AS follower_count,
               (SELECT COUNT(*) FROM follows WHERE follower_id = u.id) AS following_count,
               EXISTS (SELECT 1 FROM follows
                       WHERE follower_id = $2 AND followee_id = u.id) AS followed_by_me,
               EXISTS (SELECT 1 FROM profile_admirations
                       WHERE admirer_id = $2 AND recipient_id = u.id) AS admired_by_me,
               CASE WHEN $2 = u.id
                    THEN (SELECT COUNT(*) FROM profile_admirations
                          WHERE recipient_id = u.id)
               END AS admiration_count
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

    if !can_view_profile(&pool, me, row.id).await? {
        return Err(StatusCode::NOT_FOUND);
    }

    if let Some(visitor_id) = me.filter(|visitor_id| *visitor_id != row.id) {
        sqlx::query(
            "INSERT INTO profile_visits (profile_id, visitor_id, last_visited_at)
             SELECT $1, $2, now()
             WHERE EXISTS (SELECT 1 FROM users WHERE id = $1 AND keep_profile_visitors)
             ON CONFLICT (profile_id, visitor_id)
             DO UPDATE SET last_visited_at = EXCLUDED.last_visited_at",
        )
        .bind(row.id)
        .bind(visitor_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("profile visit recording failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    let posts = sqlx::query_as::<_, GridPost>(
        "SELECT id, image_url, media_type, caption, to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
         FROM posts
         WHERE user_id = $1 AND reshared_from_id IS NULL
         ORDER BY id DESC",
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
        date_of_birth: row.date_of_birth,
        birthday_visibility: row.birthday_visibility,
        location: row.location,
        company_name: row.company_name,
        designation: row.designation,
        work_visibility: row.work_visibility,
        contact_email: row.contact_email,
        email_visibility: row.email_visibility,
        phone_number: row.phone_number,
        phone_visibility: row.phone_visibility,
        instagram_handle: row.instagram_handle,
        facebook_handle: row.facebook_handle,
        tiktok_handle: row.tiktok_handle,
        linkedin_handle: row.linkedin_handle,
        social_visibility: row.social_visibility,
        mood: row.mood,
        interests: row.interests,
        relationship_status: row.relationship_status,
        avatar_url: row.avatar_url,
        cover_image_url: row.cover_image_url,
        post_count: row.post_count,
        follower_count: row.follower_count,
        following_count: row.following_count,
        followed_by_me: row.followed_by_me,
        admired_by_me: row.admired_by_me,
        admiration_count: row.admiration_count,
        is_me: me == Some(row.id),
        posts,
    }))
}

async fn profile_connections(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(username): Path<String>,
    Query(query): Query<ConnectionsQuery>,
) -> Result<Json<Vec<ProfileConnection>>, StatusCode> {
    if !matches!(query.kind.as_str(), "followers" | "following") {
        return Err(StatusCode::BAD_REQUEST);
    }
    let profile_id: Option<(i32,)> = sqlx::query_as("SELECT id FROM users WHERE username = $1")
        .bind(&username)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            eprintln!("profile connection owner lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let (profile_id,) = profile_id.ok_or(StatusCode::NOT_FOUND)?;
    let viewer_id = current_user_id(&pool, &headers).await.ok();
    if !can_view_profile(&pool, viewer_id, profile_id).await? {
        return Err(StatusCode::NOT_FOUND);
    }
    let connections = if query.kind == "followers" {
        sqlx::query_as::<_, ProfileConnection>(
            r#"
            SELECT follower.username, follower.avatar_url
            FROM follows f
            JOIN users target ON target.id = f.followee_id
            JOIN users follower ON follower.id = f.follower_id
            WHERE target.username = $1
            ORDER BY follower.username
            "#,
        )
        .bind(&username)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query_as::<_, ProfileConnection>(
            r#"
            SELECT following.username, following.avatar_url
            FROM follows f
            JOIN users target ON target.id = f.follower_id
            JOIN users following ON following.id = f.followee_id
            WHERE target.username = $1
            ORDER BY following.username
            "#,
        )
        .bind(&username)
        .fetch_all(&pool)
        .await
    }
    .map_err(|e| {
        eprintln!("profile connections query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if connections.is_empty() {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE username = $1)")
                .bind(&username)
                .fetch_one(&pool)
                .await
                .map_err(|e| {
                    eprintln!("profile connection owner lookup failed: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        if !exists {
            return Err(StatusCode::NOT_FOUND);
        }
    }
    Ok(Json(connections))
}

async fn profile_admirers(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<Json<Vec<ProfileAdmirer>>, StatusCode> {
    let viewer_id = current_user_id(&pool, &headers).await?;
    let owner: Option<(i32,)> = sqlx::query_as("SELECT id FROM users WHERE username = $1")
        .bind(&username)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            eprintln!("admirer profile lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let (owner_id,) = owner.ok_or(StatusCode::NOT_FOUND)?;
    if owner_id != viewer_id {
        return Err(StatusCode::FORBIDDEN);
    }
    let admirers = sqlx::query_as::<_, ProfileAdmirer>(
        r#"
        SELECT u.username, u.avatar_url
        FROM profile_admirations a
        JOIN users u ON u.id = a.admirer_id
        WHERE a.recipient_id = $1
        ORDER BY a.created_at DESC, u.username
        "#,
    )
    .bind(owner_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("profile admirers query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(admirers))
}

async fn admire_user(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let admirer_id = current_user_id(&pool, &headers).await?;
    let target: Option<(i32,)> = sqlx::query_as("SELECT id FROM users WHERE username = $1")
        .bind(&username)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            eprintln!("admiration target lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let (recipient_id,) = target.ok_or(StatusCode::NOT_FOUND)?;
    if recipient_id == admirer_id {
        return Err(StatusCode::BAD_REQUEST);
    }
    let is_follower: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM follows WHERE follower_id = $1 AND followee_id = $2)",
    )
    .bind(admirer_id)
    .bind(recipient_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("admiration follower check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !is_follower {
        return Err(StatusCode::FORBIDDEN);
    }
    sqlx::query(
        "INSERT INTO profile_admirations (admirer_id, recipient_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(admirer_id)
    .bind(recipient_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("profile admiration insert failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unadmire_user(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let admirer_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query(
        "DELETE FROM profile_admirations
         WHERE admirer_id = $1
           AND recipient_id = (SELECT id FROM users WHERE username = $2)",
    )
    .bind(admirer_id)
    .bind(&username)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("profile admiration removal failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if result.rows_affected() == 0 {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE username = $1)")
                .bind(&username)
                .fetch_one(&pool)
                .await
                .map_err(|e| {
                    eprintln!("admiration target check failed: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        if !exists {
            return Err(StatusCode::NOT_FOUND);
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn my_activity(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<MyActivity>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let notifications = sqlx::query_as::<_, ActivityItem>(
        r#"
        SELECT n.id,
               n.kind,
               actor.username AS "actor",
               p.id AS post_id,
               p.image_url,
               COALESCE(c.body, p.caption) AS detail,
               to_char(n.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at
        FROM activity_notifications n
        JOIN users actor ON actor.id = n.actor_id
        JOIN posts p ON p.id = n.post_id
        LEFT JOIN comments c ON c.id = n.comment_id
        WHERE n.recipient_id = $1
          AND EXISTS (SELECT 1 FROM users WHERE id = $1 AND notify_mentions)
        ORDER BY n.created_at DESC, n.id DESC
        LIMIT 20
        "#,
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("activity notifications query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let visitors = sqlx::query_as::<_, ProfileVisitor>(
        r#"
        SELECT visitor.username,
               visitor.avatar_url,
               to_char(v.last_visited_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS last_visited_at
        FROM profile_visits v
        JOIN users visitor ON visitor.id = v.visitor_id
        WHERE v.profile_id = $1
          AND EXISTS (SELECT 1 FROM users WHERE id = $1 AND keep_profile_visitors)
        ORDER BY v.last_visited_at DESC
        LIMIT 20
        "#,
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("profile visitors query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(MyActivity {
        notifications,
        visitors,
    }))
}

async fn settings_dashboard(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<SettingsDashboard>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let stats = sqlx::query_as::<_, SettingsDashboardStats>(
        r#"
        SELECT
            (SELECT COUNT(*) FROM profile_visits
             WHERE profile_id = $1
               AND EXISTS (
                   SELECT 1 FROM users
                   WHERE id = $1 AND keep_profile_visitors
               )) AS profile_visits_count,
            (SELECT COUNT(*) FROM follows WHERE followee_id = $1) AS follower_count,
            (SELECT COUNT(*) FROM follows
             WHERE followee_id = $1 AND created_at >= now() - interval '30 days')
                AS new_followers_count,
            (SELECT COUNT(*) FROM follows WHERE follower_id = $1) AS following_count,
            (SELECT COUNT(*) FROM posts
             WHERE user_id = $1 AND reshared_from_id IS NULL) AS posts_shared_count,
            (SELECT COUNT(*)
             FROM posts shares
             JOIN posts originals ON originals.id = shares.reshared_from_id
             WHERE originals.user_id = $1 AND shares.user_id <> $1)
                AS posts_reshared_by_others_count,
            (SELECT COUNT(*) FROM personal_chat_requests
             WHERE recipient_id = $1 AND status = 'pending')
                AS pending_personal_requests_count,
            (SELECT COUNT(*) FROM profile_admirations
             WHERE recipient_id = $1) AS admirers_count,
            (SELECT COUNT(*) FROM user_blocks
             WHERE blocker_id = $1) AS blocked_profiles_count
        "#,
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("settings dashboard stats query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let visitors = sqlx::query_as::<_, ProfileVisitor>(
        r#"
        SELECT visitor.username,
               visitor.avatar_url,
               to_char(v.last_visited_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS last_visited_at
        FROM profile_visits v
        JOIN users visitor ON visitor.id = v.visitor_id
        WHERE v.profile_id = $1
          AND EXISTS (SELECT 1 FROM users WHERE id = $1 AND keep_profile_visitors)
        ORDER BY v.last_visited_at DESC
        LIMIT 10
        "#,
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("settings dashboard visitors query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let new_followers = sqlx::query_as::<_, ProfileConnection>(
        r#"
        SELECT follower.username, follower.avatar_url
        FROM follows f
        JOIN users follower ON follower.id = f.follower_id
        WHERE f.followee_id = $1
          AND f.created_at >= now() - interval '30 days'
        ORDER BY f.created_at DESC
        LIMIT 10
        "#,
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("settings dashboard new followers query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let blocked_profiles = sqlx::query_as::<_, BlockedUser>(
        r#"
        SELECT u.username, u.avatar_url
        FROM user_blocks b
        JOIN users u ON u.id = b.blocked_id
        WHERE b.blocker_id = $1
        ORDER BY b.created_at DESC
        LIMIT 50
        "#,
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("settings dashboard blocked profiles query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(SettingsDashboard {
        stats,
        visitors,
        new_followers,
        blocked_profiles,
    }))
}

async fn list_conversations(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<Vec<ConversationSummary>>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let conversations = sqlx::query_as::<_, ConversationSummary>(
        r#"
        SELECT c.id,
               c.title,
               c.is_group,
               (c.created_by = $1) AS is_owner,
               c.is_muted,
               ARRAY(
                   SELECT u.username
                   FROM conversation_members cm
                   JOIN users u ON u.id = cm.user_id
                   WHERE cm.conversation_id = c.id
                   ORDER BY u.username
               ) AS members,
               (SELECT CASE
                           WHEN m.body <> '' THEN m.body
                           ELSE 'Photo'
                       END
                FROM direct_messages m
                WHERE m.conversation_id = c.id
                ORDER BY m.id DESC
                LIMIT 1) AS latest_message,
               (SELECT to_char(m.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
                FROM direct_messages m
                WHERE m.conversation_id = c.id
                ORDER BY m.id DESC
                LIMIT 1) AS latest_message_at,
               (SELECT COUNT(*) FROM direct_messages m
                WHERE m.conversation_id = c.id
                  AND m.sender_id <> $1
                  AND m.created_at > COALESCE(mine.last_read_at, mine.joined_at)) AS unread_count
        FROM conversations c
        JOIN conversation_members mine
          ON mine.conversation_id = c.id AND mine.user_id = $1
        ORDER BY COALESCE(
            (SELECT m.created_at FROM direct_messages m
             WHERE m.conversation_id = c.id ORDER BY m.id DESC LIMIT 1),
            c.created_at
        ) DESC
        LIMIT 100
        "#,
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("conversation list failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(conversations))
}

async fn create_conversation(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(new_conversation): Json<NewConversation>,
) -> Result<(StatusCode, Json<NewConversationResponse>), StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let mut usernames = Vec::new();
    let mut seen = HashSet::new();
    for username in new_conversation.usernames.iter().map(|name| name.trim()) {
        if username.is_empty() || username.len() > 30 || !username.chars().all(username_character) {
            return Err(StatusCode::BAD_REQUEST);
        }
        if seen.insert(username.to_string()) {
            usernames.push(username.to_string());
        }
    }
    if usernames.is_empty() || usernames.len() > 9 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let title = new_conversation
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty());
    if title.is_some_and(|title| title.chars().count() > 60) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let recipient_ids: Vec<(i32,)> =
        sqlx::query_as("SELECT id FROM users WHERE username = ANY($1)")
            .bind(&usernames)
            .fetch_all(&pool)
            .await
            .map_err(|e| {
                eprintln!("conversation participant lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    if recipient_ids.len() != usernames.len() || recipient_ids.iter().any(|(id,)| *id == user_id) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let recipient_id_values: Vec<i32> = recipient_ids.iter().map(|(id,)| *id).collect();
    let recipients_reject_message: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM users recipient
            WHERE recipient.id = ANY($1)
              AND (
                  NOT recipient.account_enabled
                  OR recipient.message_privacy = 'nobody'
                  OR (
                      recipient.message_privacy = 'following'
                      AND NOT EXISTS (
                          SELECT 1 FROM follows f
                          WHERE f.follower_id = recipient.id AND f.followee_id = $2
                      )
                  )
                  OR EXISTS (
                      SELECT 1 FROM user_blocks b
                      WHERE (b.blocker_id = recipient.id AND b.blocked_id = $2)
                         OR (b.blocker_id = $2 AND b.blocked_id = recipient.id)
                  )
              )
        )
        "#,
    )
    .bind(&recipient_id_values)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("conversation privacy check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if recipients_reject_message {
        return Err(StatusCode::FORBIDDEN);
    }
    let is_group = recipient_ids.len() > 1;

    let mut tx = pool.begin().await.map_err(|e| {
        eprintln!("conversation transaction failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !is_group {
        let existing: Option<(i32,)> = sqlx::query_as(
            r#"
            SELECT c.id
            FROM conversations c
            JOIN conversation_members mine
              ON mine.conversation_id = c.id AND mine.user_id = $1
            JOIN conversation_members other
              ON other.conversation_id = c.id AND other.user_id = $2
            WHERE NOT c.is_group
              AND (SELECT COUNT(*) FROM conversation_members cm
                   WHERE cm.conversation_id = c.id) = 2
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .bind(recipient_ids[0].0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| {
            eprintln!("direct conversation lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if let Some((conversation_id,)) = existing {
            tx.commit().await.map_err(|e| {
                eprintln!("conversation lookup commit failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            return Ok((
                StatusCode::OK,
                Json(NewConversationResponse {
                    id: conversation_id,
                }),
            ));
        }
    }

    let title = if is_group {
        title.map(str::to_string).or_else(|| {
            let mut members = usernames.clone();
            members.sort();
            Some(members.join(", "))
        })
    } else {
        None
    };
    let (conversation_id,): (i32,) = sqlx::query_as(
        "INSERT INTO conversations (title, is_group, created_by) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(title)
    .bind(is_group)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        eprintln!("conversation creation failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    sqlx::query("INSERT INTO conversation_members (conversation_id, user_id) VALUES ($1, $2)")
        .bind(conversation_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            eprintln!("conversation owner insert failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    for (recipient_id,) in recipient_ids {
        sqlx::query("INSERT INTO conversation_members (conversation_id, user_id) VALUES ($1, $2)")
            .bind(conversation_id)
            .bind(recipient_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                eprintln!("conversation member insert failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    }
    tx.commit().await.map_err(|e| {
        eprintln!("conversation creation commit failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((
        StatusCode::CREATED,
        Json(NewConversationResponse {
            id: conversation_id,
        }),
    ))
}

async fn create_personal_chat_request(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<(StatusCode, Json<NewTalkRequestResponse>), StatusCode> {
    let sender_id = current_user_id(&pool, &headers).await?;
    let recipient: Option<(i32,)> = sqlx::query_as("SELECT id FROM users WHERE username = $1")
        .bind(&username)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            eprintln!("personal chat request target lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let (recipient_id,) = recipient.ok_or(StatusCode::NOT_FOUND)?;
    if recipient_id == sender_id {
        return Err(StatusCode::BAD_REQUEST);
    }
    let recipient_rejects_request: bool = sqlx::query_scalar(
        r#"
        SELECT NOT u.account_enabled
            OR u.message_privacy = 'nobody'
            OR (
                u.message_privacy = 'following'
                AND NOT EXISTS (
                    SELECT 1 FROM follows f
                    WHERE f.follower_id = u.id AND f.followee_id = $2
                )
            )
            OR EXISTS (
                SELECT 1 FROM user_blocks b
                WHERE (b.blocker_id = u.id AND b.blocked_id = $2)
                   OR (b.blocker_id = $2 AND b.blocked_id = u.id)
            )
        FROM users u WHERE u.id = $1
        "#,
    )
    .bind(recipient_id)
    .bind(sender_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("personal chat request privacy check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if recipient_rejects_request {
        return Err(StatusCode::FORBIDDEN);
    }
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO personal_chat_requests (sender_id, recipient_id)
         VALUES ($1, $2) RETURNING id",
    )
    .bind(sender_id)
    .bind(recipient_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("personal chat request insert failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((StatusCode::CREATED, Json(NewTalkRequestResponse { id })))
}

async fn list_personal_chat_requests(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<Vec<PersonalChatRequest>>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let requests = sqlx::query_as::<_, PersonalChatRequest>(
        r#"
        SELECT r.id,
               u.username,
               r.status,
               (r.recipient_id = $1) AS incoming,
               r.conversation_id,
               to_char(r.requested_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS requested_at
        FROM personal_chat_requests r
        JOIN users u ON u.id = CASE
            WHEN r.sender_id = $1 THEN r.recipient_id
            ELSE r.sender_id
        END
        WHERE r.sender_id = $1 OR r.recipient_id = $1
        ORDER BY r.requested_at DESC, r.id DESC
        LIMIT 100
        "#,
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("personal chat request list failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(requests))
}

async fn accept_personal_chat_request(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(request_id): Path<i64>,
) -> Result<Json<AcceptedChatRequest>, StatusCode> {
    let recipient_id = current_user_id(&pool, &headers).await?;
    let mut tx = pool.begin().await.map_err(|e| {
        eprintln!("personal chat acceptance transaction failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let request: Option<(i32, String)> = sqlx::query_as(
        "SELECT sender_id, status FROM personal_chat_requests
         WHERE id = $1 AND recipient_id = $2 FOR UPDATE",
    )
    .bind(request_id)
    .bind(recipient_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        eprintln!("personal chat request lookup failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let (sender_id, status) = request.ok_or(StatusCode::NOT_FOUND)?;
    if status != "pending" {
        return Err(StatusCode::CONFLICT);
    }

    let existing: Option<(i32,)> = sqlx::query_as(
        r#"
        SELECT c.id
        FROM conversations c
        JOIN conversation_members mine
          ON mine.conversation_id = c.id AND mine.user_id = $1
        JOIN conversation_members other
          ON other.conversation_id = c.id AND other.user_id = $2
        WHERE NOT c.is_group
          AND (SELECT COUNT(*) FROM conversation_members cm
               WHERE cm.conversation_id = c.id) = 2
        LIMIT 1
        "#,
    )
    .bind(recipient_id)
    .bind(sender_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        eprintln!("accepted direct conversation lookup failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let conversation_id = if let Some((id,)) = existing {
        id
    } else {
        let (id,): (i32,) =
            sqlx::query_as("INSERT INTO conversations (is_group) VALUES (FALSE) RETURNING id")
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| {
                    eprintln!("accepted direct conversation creation failed: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        for member_id in [recipient_id, sender_id] {
            sqlx::query(
                "INSERT INTO conversation_members (conversation_id, user_id) VALUES ($1, $2)",
            )
            .bind(id)
            .bind(member_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                eprintln!("accepted conversation member insert failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }
        id
    };
    sqlx::query(
        "UPDATE personal_chat_requests
         SET status = 'accepted', conversation_id = $1, responded_at = now()
         WHERE id = $2",
    )
    .bind(conversation_id)
    .bind(request_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        eprintln!("personal chat request acceptance update failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    tx.commit().await.map_err(|e| {
        eprintln!("personal chat acceptance commit failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(AcceptedChatRequest { conversation_id }))
}

async fn reject_personal_chat_request(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(request_id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    let recipient_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query(
        "UPDATE personal_chat_requests
         SET status = 'rejected', responded_at = now()
         WHERE id = $1 AND recipient_id = $2 AND status = 'pending'",
    )
    .bind(request_id)
    .bind(recipient_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("personal chat request rejection failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if result.rows_affected() > 0 {
        return Ok(StatusCode::NO_CONTENT);
    }
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM personal_chat_requests WHERE id = $1 AND recipient_id = $2)",
    )
    .bind(request_id)
    .bind(recipient_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("personal chat request state check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if exists {
        Err(StatusCode::CONFLICT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn search_users(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(query): Query<UserSearchQuery>,
) -> Result<Json<Vec<UserSearchResult>>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let search = query.q.trim();
    if search.chars().count() < 2 || search.chars().count() > 50 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let users = sqlx::query_as::<_, UserSearchResult>(
        "SELECT username, display_name, avatar_url FROM users
         WHERE id <> $1
           AND account_enabled
           AND (
               profile_visibility = 'public'
               OR (
                   profile_visibility = 'followers'
                   AND EXISTS (
                       SELECT 1 FROM follows f
                       WHERE f.follower_id = $1 AND f.followee_id = users.id
                   )
               )
           )
           AND (LOWER(LEFT(username, char_length($2))) = LOWER($2)
                OR STRPOS(LOWER(COALESCE(display_name, '')), LOWER($2)) > 0)
           AND NOT EXISTS (
               SELECT 1 FROM user_blocks b
               WHERE (b.blocker_id = $1 AND b.blocked_id = users.id)
                  OR (b.blocker_id = users.id AND b.blocked_id = $1)
           )
         ORDER BY CASE WHEN LOWER(LEFT(username, char_length($2))) = LOWER($2) THEN 0 ELSE 1 END, username
         LIMIT 50",
    )
    .bind(user_id)
    .bind(search)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("user search failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(users))
}

async fn list_messages(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(conversation_id): Path<i32>,
) -> Result<Json<Vec<DirectMessage>>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM conversation_members WHERE conversation_id = $1 AND user_id = $2)",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("conversation access check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !is_member {
        return Err(StatusCode::NOT_FOUND);
    }
    sqlx::query("UPDATE conversation_members SET last_read_at = now() WHERE conversation_id = $1 AND user_id = $2")
        .bind(conversation_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("conversation read receipt update failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let messages = sqlx::query_as::<_, DirectMessage>(
        r#"
        SELECT m.id,
               u.username AS "sender",
               m.body,
               CASE WHEN m.image_path LIKE 'https://%.giphy.com/%' OR m.image_path LIKE 'https://%.wikimedia.org/%'
                    THEN m.image_path
                    WHEN m.image_path IS NOT NULL
                    THEN '/api/messages/' || m.id || '/image'
               END AS image_url,
               CASE WHEN RIGHT(m.image_path, 4) = '.mp4' OR RIGHT(m.image_path, 5) = '.webm'
                    THEN 'video' ELSE 'image' END AS media_type,
               to_char(m.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
               (m.sender_id = $2 OR (c.is_group AND c.created_by = $2)) AS can_delete
        FROM direct_messages m
        JOIN users u ON u.id = m.sender_id
        JOIN conversations c ON c.id = m.conversation_id
        WHERE m.conversation_id = $1
        ORDER BY m.id DESC
        LIMIT 100
        "#,
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("conversation messages query failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(messages.into_iter().rev().collect()))
}

async fn send_message(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(conversation_id): Path<i32>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<DirectMessage>), StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM conversation_members WHERE conversation_id = $1 AND user_id = $2)",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("message conversation access check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !is_member {
        return Err(StatusCode::NOT_FOUND);
    }
    let is_muted: bool = sqlx::query_scalar("SELECT is_muted FROM conversations WHERE id = $1")
        .bind(conversation_id)
        .fetch_one(&pool)
        .await
        .map_err(|e| {
            eprintln!("conversation moderation lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if is_muted {
        return Err(StatusCode::FORBIDDEN);
    }
    let other_member_blocks_sender: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM conversation_members cm
            JOIN users recipient ON recipient.id = cm.user_id
            WHERE cm.conversation_id = $1
              AND cm.user_id <> $2
              AND (
                  NOT recipient.account_enabled
                  OR recipient.message_privacy = 'nobody'
                  OR (
                      recipient.message_privacy = 'following'
                      AND NOT EXISTS (
                          SELECT 1 FROM follows f
                          WHERE f.follower_id = recipient.id AND f.followee_id = $2
                      )
                  )
                  OR EXISTS (
                      SELECT 1 FROM user_blocks b
                      WHERE (b.blocker_id = recipient.id AND b.blocked_id = $2)
                         OR (b.blocker_id = $2 AND b.blocked_id = recipient.id)
                  )
              )
        )
        "#,
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("message privacy check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if other_member_blocks_sender {
        return Err(StatusCode::FORBIDDEN);
    }

    let mut body = String::new();
    let mut image: Option<(Vec<u8>, &'static str)> = None;
    let mut remote_media_url: Option<String> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        match field.name() {
            Some("body") => body = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?,
            Some("remote_media_url") => {
                let value = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                let parsed = reqwest::Url::parse(&value).map_err(|_| StatusCode::BAD_REQUEST)?;
                let host = parsed.host_str().unwrap_or_default();
                if parsed.scheme() != "https"
                    || !(host == "giphy.com" || host.ends_with(".giphy.com")
                        || host == "upload.wikimedia.org" || host == "thumb.wikimedia.org"
                        || host == "commons.wikimedia.org")
                    || value.len() > 2048
                {
                    return Err(StatusCode::BAD_REQUEST);
                }
                remote_media_url = Some(value);
            }
            Some("image") => {
                let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                if bytes.len() > 50 * 1024 * 1024 {
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
                let extension = image_extension(&bytes)
                    .or_else(|| video_extension(&bytes))
                    .ok_or(StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
                image = Some((bytes.to_vec(), extension));
            }
            _ => {}
        }
    }
    let body = body.trim();
    if body.chars().count() > 2000 || (body.is_empty() && image.is_none() && remote_media_url.is_none()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let image_path = if let Some(url) = remote_media_url {
        Some(url)
    } else if let Some((bytes, extension)) = image {
        Some(save_private_image(&bytes, extension).await?)
    } else {
        None
    };
    let message = sqlx::query_as::<_, DirectMessage>(
        r#"
        WITH inserted AS (
        INSERT INTO direct_messages (conversation_id, sender_id, body, image_path)
        VALUES ($1, $2, $3, $4)
        RETURNING id, sender_id, body, image_path, created_at
        )
        SELECT inserted.id,
               u.username AS "sender",
               inserted.body,
        CASE WHEN inserted.image_path LIKE 'https://%.giphy.com/%' OR inserted.image_path LIKE 'https://%.wikimedia.org/%'
                    THEN inserted.image_path
                    WHEN inserted.image_path IS NOT NULL
                    THEN '/api/messages/' || inserted.id || '/image'
               END AS image_url,
               CASE WHEN RIGHT(inserted.image_path, 4) = '.mp4' OR RIGHT(inserted.image_path, 5) = '.webm'
                    THEN 'video' ELSE 'image' END AS media_type,
               to_char(inserted.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
               TRUE AS can_delete
        FROM inserted JOIN users u ON u.id = inserted.sender_id
        "#,
    )
    .bind(conversation_id)
    .bind(user_id)
    .bind(body)
    .bind(&image_path)
    .fetch_one(&pool)
    .await;
    match message {
        Ok(message) => Ok((StatusCode::CREATED, Json(message))),
        Err(e) => {
            if let Some(image_path) = image_path.filter(|path| !path.starts_with("https://")) {
                remove_private_image(&image_path).await;
            }
            eprintln!("send message failed: {e}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn delete_conversation(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(conversation_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query(
        "DELETE FROM conversation_members cm USING conversations c WHERE cm.conversation_id = c.id AND cm.conversation_id = $1 AND cm.user_id = $2 AND (NOT c.is_group OR c.created_by <> $2)",
    )
    .bind(conversation_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("conversation removal failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn update_group(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(conversation_id): Path<i32>,
    Json(request): Json<GroupTitleRequest>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let title = request.title.trim();
    if title.is_empty() || title.chars().count() > 60 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let result = sqlx::query(
        "UPDATE conversations SET title = $1 WHERE id = $2 AND is_group AND created_by = $3",
    )
    .bind(title)
    .bind(conversation_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("group rename failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_group_members(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(conversation_id): Path<i32>,
) -> Result<Json<Vec<GroupMember>>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM conversations c JOIN conversation_members cm ON cm.conversation_id = c.id WHERE c.id = $1 AND c.is_group AND cm.user_id = $2)",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("group member access check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !is_member {
        return Err(StatusCode::NOT_FOUND);
    }
    let members = sqlx::query_as::<_, GroupMember>(
        "SELECT u.id, u.username, (u.id = c.created_by) AS is_owner FROM conversation_members cm JOIN users u ON u.id = cm.user_id JOIN conversations c ON c.id = cm.conversation_id WHERE c.id = $1 ORDER BY is_owner DESC, u.username",
    )
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("group member list failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(members))
}

async fn remove_group_member(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path((conversation_id, member_id)): Path<(i32, i32)>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query(
        "DELETE FROM conversation_members cm USING conversations c WHERE cm.conversation_id = c.id AND cm.conversation_id = $1 AND cm.user_id = $2 AND c.is_group AND c.created_by = $3 AND cm.user_id <> c.created_by",
    )
    .bind(conversation_id)
    .bind(member_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("group member removal failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_group_messages(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(conversation_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let is_owner: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM conversations WHERE id = $1 AND is_group AND created_by = $2)",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("group owner check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !is_owner {
        return Err(StatusCode::NOT_FOUND);
    }
    let paths: Vec<(Option<String>,)> = sqlx::query_as(
        "DELETE FROM direct_messages m USING conversations c WHERE m.conversation_id = c.id AND c.id = $1 AND c.is_group AND c.created_by = $2 RETURNING m.image_path",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("group message deletion failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    for (path,) in paths {
        if let Some(path) = path {
            remove_private_image(&path).await;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_group(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(conversation_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query(
        "DELETE FROM conversations WHERE id = $1 AND is_group AND created_by = $2",
    )
    .bind(conversation_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("group deletion failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_message(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path((conversation_id, message_id)): Path<(i32, i64)>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "DELETE FROM direct_messages m USING conversations c, conversation_members mine WHERE m.id = $1 AND m.conversation_id = c.id AND c.id = $2 AND mine.conversation_id = c.id AND mine.user_id = $3 AND (m.sender_id = $3 OR (c.is_group AND c.created_by = $3)) RETURNING m.image_path",
    )
    .bind(message_id)
    .bind(conversation_id)
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        eprintln!("message deletion failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some((Some(path),)) = row.as_ref() else {
        if row.is_none() {
            return Err(StatusCode::NOT_FOUND);
        }
        return Ok(StatusCode::NO_CONTENT);
    };
    remove_private_image(path).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn report_conversation(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(conversation_id): Path<i32>,
    Json(report): Json<ChatReportRequest>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM conversation_members WHERE conversation_id = $1 AND user_id = $2)",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("chat report membership check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !is_member {
        return Err(StatusCode::NOT_FOUND);
    }
    let details = report.details.unwrap_or_default().trim().to_string();
    if details.chars().count() > 500 {
        return Err(StatusCode::BAD_REQUEST);
    }
    sqlx::query(
        "INSERT INTO conversation_reports (conversation_id, reporter_id, details) VALUES ($1, $2, $3) ON CONFLICT (conversation_id, reporter_id) DO UPDATE SET details = EXCLUDED.details, created_at = now()",
    )
    .bind(conversation_id)
    .bind(user_id)
    .bind(details)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("chat report submission failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::CREATED)
}

async fn read_message_image(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(message_id): Path<i64>,
) -> Result<Response, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let path: Option<(String,)> = sqlx::query_as(
        r#"
        SELECT m.image_path
        FROM direct_messages m
        JOIN conversation_members cm ON cm.conversation_id = m.conversation_id
        WHERE m.id = $1 AND cm.user_id = $2 AND m.image_path IS NOT NULL
        "#,
    )
    .bind(message_id)
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        eprintln!("message image access check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let (path,) = path.ok_or(StatusCode::NOT_FOUND)?;
    if path.starts_with("https://") {
        let parsed = reqwest::Url::parse(&path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let host = parsed.host_str().unwrap_or_default();
        if parsed.scheme() == "https" && (host == "giphy.com" || host.ends_with(".giphy.com")
            || host == "upload.wikimedia.org" || host == "thumb.wikimedia.org" || host == "commons.wikimedia.org") {
            return Ok(Redirect::temporary(&path).into_response());
        }
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let safe =
        !path.is_empty() && !path.contains('/') && !path.contains('\\') && !path.contains("..");
    if !safe {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let bytes = tokio::fs::read(format!("private_uploads/{path}"))
        .await
        .map_err(|e| {
            eprintln!("could not read private message image: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let content_type = match path.rsplit('.').next() {
        Some("jpg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        _ => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        bytes,
    ).into_response())
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
    let bio = clean(p.bio, 3500)?;
    let location = clean(p.location, 60)?;
    let mood = clean(p.mood, 20)?;
    let interests = clean(p.interests, 200)?;
    let relationship_status = clean(p.relationship_status, 30)?;
    let company_name = clean(p.company_name, 100)?;
    let designation = clean(p.designation, 100)?;
    let contact_email = clean(p.contact_email, 254)?.map(|value| value.to_ascii_lowercase());
    let phone_number = clean(p.phone_number, 30)?;
    let instagram_handle = clean(p.instagram_handle, 60)?;
    let facebook_handle = clean(p.facebook_handle, 60)?;
    let tiktok_handle = clean(p.tiktok_handle, 60)?;
    let linkedin_handle = clean(p.linkedin_handle, 100)?;
    let date_of_birth = clean(p.date_of_birth, 10)?;
    let birthday_visibility = p.birthday_visibility.unwrap_or_else(|| "self".to_string());
    let work_visibility = p.work_visibility.unwrap_or_else(|| "self".to_string());
    let email_visibility = p.email_visibility.unwrap_or_else(|| "self".to_string());
    let phone_visibility = p.phone_visibility.unwrap_or_else(|| "self".to_string());
    let social_visibility = p.social_visibility.unwrap_or_else(|| "self".to_string());

    let valid_visibility = |value: &str| matches!(value, "everyone" | "friends" | "self");
    if !valid_visibility(&birthday_visibility)
        || !valid_visibility(&work_visibility)
        || !valid_visibility(&email_visibility)
        || !valid_visibility(&phone_visibility)
        || !valid_visibility(&social_visibility)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    if let Some(email) = &contact_email {
        let valid = email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && !domain.contains('@')
                && domain.rsplit_once('.').is_some_and(|(host, suffix)| !host.is_empty() && !suffix.is_empty())
        });
        if !valid {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if phone_number.as_deref().is_some_and(|phone| !phone.chars().all(|ch| ch.is_ascii_digit() || "+()- ".contains(ch))) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let valid_handle = |value: &Option<String>| value.as_deref().is_none_or(|handle| {
        handle.trim_start_matches('@').chars().all(|ch| ch.is_ascii_alphanumeric() || "._-".contains(ch))
    });
    if !valid_handle(&instagram_handle) || !valid_handle(&facebook_handle)
        || !valid_handle(&tiktok_handle) || !valid_handle(&linkedin_handle)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    if let Some(date) = &date_of_birth {
        if date.len() != 10 || date.as_bytes().get(4) != Some(&b'-') || date.as_bytes().get(7) != Some(&b'-')
            || !date.chars().enumerate().all(|(i, ch)| i == 4 || i == 7 || ch.is_ascii_digit())
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        let valid_age: bool = sqlx::query_scalar(
            "SELECT $1::date <= CURRENT_DATE - INTERVAL '13 years' AND $1::date >= CURRENT_DATE - INTERVAL '120 years'",
        )
        .bind(date)
        .fetch_one(&pool)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
        if !valid_age {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    if mood.as_deref().is_some_and(|value| !mood_is_valid(value)) {
        return Err(StatusCode::BAD_REQUEST);
    }
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
             mood = $5, interests = $6, relationship_status = $7,
             date_of_birth = $8::date, birthday_visibility = $9,
             company_name = $10, designation = $11, work_visibility = $12,
             contact_email = $13, email_visibility = $14,
             phone_number = $15, phone_visibility = $16,
             instagram_handle = $17, facebook_handle = $18,
             tiktok_handle = $19, linkedin_handle = $20, social_visibility = $21
         WHERE id = $22",
    )
    .bind(display_name)
    .bind(bio)
    .bind(p.age)
    .bind(location)
    .bind(mood)
    .bind(interests)
    .bind(relationship_status)
    .bind(date_of_birth)
    .bind(birthday_visibility)
    .bind(company_name)
    .bind(designation)
    .bind(work_visibility)
    .bind(contact_email)
    .bind(email_visibility)
    .bind(phone_number)
    .bind(phone_visibility)
    .bind(instagram_handle)
    .bind(facebook_handle)
    .bind(tiktok_handle)
    .bind(linkedin_handle)
    .bind(social_visibility)
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

async fn upload_home_cover(
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
    let old: Option<(Option<String>,)> =
        sqlx::query_as("SELECT cover_image_url FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| {
                eprintln!("home cover lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let url = save_image(&bytes, ext).await?;
    let update = sqlx::query(
        "UPDATE users
         SET cover_image_url = $1, cover_position_x = 50, cover_position_y = 50, cover_zoom = 100
         WHERE id = $2",
    )
    .bind(&url)
    .bind(user_id)
    .execute(&pool)
    .await;
    if let Err(e) = update {
        remove_upload(&url).await;
        eprintln!("home cover update failed: {e}");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    if let Some((Some(old_url),)) = old {
        remove_upload(&old_url).await;
    }
    Ok(Json(AvatarInfo { avatar_url: url }))
}

async fn update_home_cover_position(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(position): Json<CoverPositionUpdate>,
) -> Result<StatusCode, StatusCode> {
    if !(0..=100).contains(&position.position_x)
        || !(0..=100).contains(&position.position_y)
        || !(100..=200).contains(&position.zoom)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let user_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query(
        "UPDATE users
         SET cover_position_x = $1, cover_position_y = $2, cover_zoom = $3
         WHERE id = $4 AND cover_image_url IS NOT NULL",
    )
    .bind(position.position_x)
    .bind(position.position_y)
    .bind(position.zoom)
    .bind(user_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("homepage cover position update failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_home_cover(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let old: Option<(Option<String>,)> =
        sqlx::query_as("SELECT cover_image_url FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&pool)
            .await
            .map_err(|e| {
                eprintln!("home cover lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    sqlx::query(
        "UPDATE users
         SET cover_image_url = NULL, cover_position_x = 50, cover_position_y = 50, cover_zoom = 100
         WHERE id = $1",
    )
    .bind(user_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("home cover removal failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if let Some((Some(old_url),)) = old {
        remove_upload(&old_url).await;
    }
    Ok(StatusCode::NO_CONTENT)
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

async fn favorite_user(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let target: Option<(i32,)> = sqlx::query_as("SELECT id FROM users WHERE username = $1")
        .bind(&username)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            eprintln!("favorite lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let (favorite_user_id,) = target.ok_or(StatusCode::NOT_FOUND)?;
    if favorite_user_id == user_id {
        return Err(StatusCode::BAD_REQUEST);
    }
    sqlx::query(
        "INSERT INTO user_favorites (user_id, favorite_user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(favorite_user_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("favorite insert failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unfavorite_user(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    sqlx::query(
        "DELETE FROM user_favorites
         WHERE user_id = $1
           AND favorite_user_id = (SELECT id FROM users WHERE username = $2)",
    )
    .bind(user_id)
    .bind(&username)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("unfavorite failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_friends(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<Vec<FollowedPerson>>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let people = sqlx::query_as::<_, FollowedPerson>(
        r#"SELECT u.username, u.display_name, u.avatar_url,
                  EXISTS (SELECT 1 FROM user_favorites f
                          WHERE f.user_id = $1 AND f.favorite_user_id = u.id) AS is_favorite
           FROM follows follow
           JOIN users u ON u.id = follow.followee_id
           WHERE follow.follower_id = $1 AND u.account_enabled
           ORDER BY u.username"#,
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("friends list failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(people))
}

async fn list_communities(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<Vec<CommunitySummary>>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let groups = sqlx::query_as::<_, CommunitySummary>(
        r#"SELECT c.id, c.name, c.description, c.image_url, owner.username AS owner,
                  EXISTS (SELECT 1 FROM community_members mine WHERE mine.community_id = c.id AND mine.user_id = $1 AND mine.is_enabled) AS is_member,
                  EXISTS (SELECT 1 FROM community_members mine WHERE mine.community_id = c.id AND mine.user_id = $1 AND NOT mine.is_enabled) AS is_disabled,
                  COALESCE(c.created_by = $1, FALSE) AS is_owner,
                  EXISTS (SELECT 1 FROM community_members mine WHERE mine.community_id = c.id AND mine.user_id = $1 AND mine.is_enabled AND mine.is_admin) AS is_admin,
                  (SELECT COUNT(*) FROM community_members cm WHERE cm.community_id = c.id) AS member_count,
                  to_char(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at
           FROM communities c LEFT JOIN users owner ON owner.id = c.created_by
           ORDER BY (c.created_by IS NULL) DESC, c.name"#,
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("community list failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(groups))
}

async fn create_community(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(input): Json<NewCommunity>,
) -> Result<(StatusCode, Json<CommunitySummary>), StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let name = input.name.trim();
    let description = input.description.as_deref().unwrap_or("").trim();
    if !(3..=50).contains(&name.chars().count()) || description.chars().count() > 500 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut tx = pool.begin().await.map_err(|e| {
        eprintln!("community creation transaction failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let created = sqlx::query_as::<_, (i32,)>(
        "INSERT INTO communities (name, description, created_by) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(name)
    .bind(description)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await;
    let (community_id,) = match created {
        Ok(row) => row,
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => return Err(StatusCode::CONFLICT),
        Err(error) => {
            eprintln!("community creation failed: {error}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    sqlx::query("INSERT INTO community_members (community_id, user_id) VALUES ($1, $2)")
        .bind(community_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            eprintln!("community owner membership creation failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    tx.commit().await.map_err(|e| {
        eprintln!("community creation commit failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let summary = sqlx::query_as::<_, CommunitySummary>(
        r#"SELECT c.id, c.name, c.description, c.image_url, owner.username AS owner, TRUE AS is_member, FALSE AS is_disabled, TRUE AS is_owner, FALSE AS is_admin,
                  1::BIGINT AS member_count,
                  to_char(c.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at
           FROM communities c JOIN users owner ON owner.id = c.created_by WHERE c.id = $1"#,
    )
    .bind(community_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("created community lookup failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((StatusCode::CREATED, Json(summary)))
}

async fn join_community(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(community_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM communities WHERE id = $1)")
        .bind(community_id).fetch_one(&pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !exists { return Err(StatusCode::NOT_FOUND); }
    sqlx::query("INSERT INTO community_members (community_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(community_id).bind(user_id).execute(&pool).await.map_err(|e| {
            eprintln!("community join failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let enabled: bool = sqlx::query_scalar("SELECT is_enabled FROM community_members WHERE community_id = $1 AND user_id = $2")
        .bind(community_id).bind(user_id).fetch_one(&pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !enabled { return Err(StatusCode::FORBIDDEN); }
    Ok(StatusCode::NO_CONTENT)
}

async fn claim_community(
    State(pool): State<PgPool>, headers: HeaderMap, Path(community_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query("UPDATE communities c SET created_by=$2 WHERE c.id=$1 AND c.created_by IS NULL AND EXISTS(SELECT 1 FROM community_members m WHERE m.community_id=c.id AND m.user_id=$2 AND m.is_enabled)")
        .bind(community_id).bind(user_id).execute(&pool).await.map_err(|e| {
            eprintln!("community ownership claim failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM communities WHERE id=$1)").bind(community_id).fetch_one(&pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Err(if exists { StatusCode::CONFLICT } else { StatusCode::NOT_FOUND });
    }
    sqlx::query("INSERT INTO community_members (community_id,user_id,is_enabled) VALUES ($1,$2,TRUE) ON CONFLICT (community_id,user_id) DO UPDATE SET is_enabled=TRUE")
        .bind(community_id).bind(user_id).execute(&pool).await.map_err(|e| {
            eprintln!("claimed community membership update failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn leave_community(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(community_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let owner: Option<(Option<i32>,)> = sqlx::query_as("SELECT created_by FROM communities WHERE id = $1")
        .bind(community_id).fetch_optional(&pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some((owner_id,)) = owner else { return Err(StatusCode::NOT_FOUND); };
    if owner_id == Some(user_id) { return Err(StatusCode::FORBIDDEN); }
    sqlx::query("DELETE FROM community_members WHERE community_id = $1 AND user_id = $2")
        .bind(community_id).bind(user_id).execute(&pool).await.map_err(|e| {
            eprintln!("community leave failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_community(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(community_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query("DELETE FROM communities WHERE id = $1 AND created_by = $2")
        .bind(community_id).bind(user_id).execute(&pool).await.map_err(|e| {
            eprintln!("community deletion failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 { return Err(StatusCode::FORBIDDEN); }
    Ok(StatusCode::NO_CONTENT)
}

async fn update_community(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(community_id): Path<i32>,
    Json(input): Json<NewCommunity>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let name = input.name.trim();
    let description = input.description.as_deref().unwrap_or("").trim();
    if !(3..=50).contains(&name.chars().count()) || description.chars().count() > 500 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let result = sqlx::query("UPDATE communities SET name = $1, description = $2 WHERE id = $3 AND created_by = $4")
        .bind(name).bind(description).bind(community_id).bind(user_id).execute(&pool).await.map_err(|e| {
            if e.as_database_error().is_some_and(|db| db.is_unique_violation()) { return StatusCode::CONFLICT; }
            eprintln!("community update failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 { return Err(StatusCode::FORBIDDEN); }
    Ok(StatusCode::NO_CONTENT)
}

async fn upload_community_image(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(community_id): Path<i32>,
    mut multipart: Multipart,
) -> Result<Json<AvatarInfo>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let old: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT image_url FROM communities WHERE id = $1 AND created_by = $2",
    )
    .bind(community_id)
    .bind(user_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        eprintln!("community image lookup failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let Some((old_url,)) = old else {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM communities WHERE id = $1)")
            .bind(community_id)
            .fetch_one(&pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        return Err(if exists { StatusCode::FORBIDDEN } else { StatusCode::NOT_FOUND });
    };

    let mut image: Option<(Vec<u8>, &'static str)> = None;
    while let Some(field) = multipart.next_field().await.map_err(|_| StatusCode::BAD_REQUEST)? {
        if field.name() == Some("image") {
            let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            if bytes.len() > 5 * 1024 * 1024 {
                return Err(StatusCode::PAYLOAD_TOO_LARGE);
            }
            let ext = image_extension(&bytes).ok_or(StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
            image = Some((bytes.to_vec(), ext));
        }
    }
    let (bytes, ext) = image.ok_or(StatusCode::BAD_REQUEST)?;
    let url = save_image(&bytes, ext).await?;
    let result = sqlx::query("UPDATE communities SET image_url = $1 WHERE id = $2 AND created_by = $3")
        .bind(&url)
        .bind(community_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("community image update failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 {
        remove_upload(&url).await;
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(old_url) = old_url {
        remove_upload(&old_url).await;
    }
    Ok(Json(AvatarInfo { avatar_url: url }))
}

async fn list_community_members(
    State(pool): State<PgPool>, headers: HeaderMap, Path(community_id): Path<i32>,
) -> Result<Json<Vec<CommunityMember>>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let is_member: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM community_members WHERE community_id=$1 AND user_id=$2 AND is_enabled)")
        .bind(community_id).bind(user_id).fetch_one(&pool).await.map_err(|e| {
            eprintln!("community membership check failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !is_member { return Err(StatusCode::FORBIDDEN); }
    let members = sqlx::query_as::<_, CommunityMember>(
        "SELECT u.id, u.username, COALESCE(u.id = c.created_by, FALSE) AS is_owner, cm.is_admin, cm.is_enabled AS enabled FROM community_members cm JOIN users u ON u.id=cm.user_id JOIN communities c ON c.id=cm.community_id WHERE cm.community_id=$1 ORDER BY is_owner DESC, cm.is_admin DESC, u.username",
    ).bind(community_id).fetch_all(&pool).await.map_err(|e| {
        eprintln!("community members lookup failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(members))
}

async fn add_community_member(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(community_id): Path<i32>,
    Json(input): Json<AddCommunityMember>,
) -> Result<StatusCode, StatusCode> {
    let owner_id = current_user_id(&pool, &headers).await?;
    let can_manage: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM communities c WHERE c.id=$1 AND (c.created_by=$2 OR EXISTS(SELECT 1 FROM community_members m WHERE m.community_id=c.id AND m.user_id=$2 AND m.is_enabled AND m.is_admin)))")
        .bind(community_id).bind(owner_id).fetch_one(&pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !can_manage { return Err(StatusCode::FORBIDDEN); }
    let username = input.username.trim();
    if username.is_empty() || username.chars().count() > 30 { return Err(StatusCode::BAD_REQUEST); }
    let member_id: Option<i32> = sqlx::query_scalar("SELECT id FROM users WHERE lower(username)=lower($1) AND account_enabled")
        .bind(username).fetch_optional(&pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let Some(member_id) = member_id else { return Err(StatusCode::NOT_FOUND); };
    sqlx::query("INSERT INTO community_members (community_id,user_id,is_enabled) VALUES ($1,$2,TRUE) ON CONFLICT (community_id,user_id) DO UPDATE SET is_enabled=TRUE")
        .bind(community_id).bind(member_id).execute(&pool).await.map_err(|e| {
            eprintln!("community member add failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_community_member_status(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path((community_id, member_id)): Path<(i32, i32)>,
    Json(input): Json<CommunityMemberStatus>,
) -> Result<StatusCode, StatusCode> {
    let owner_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query("UPDATE community_members cm SET is_enabled=$1 FROM communities c WHERE cm.community_id=c.id AND cm.community_id=$2 AND cm.user_id=$3 AND cm.user_id<>c.created_by AND NOT cm.is_admin AND (c.created_by=$4 OR EXISTS(SELECT 1 FROM community_members actor WHERE actor.community_id=c.id AND actor.user_id=$4 AND actor.is_enabled AND actor.is_admin))")
        .bind(input.enabled).bind(community_id).bind(member_id).bind(owner_id).execute(&pool).await.map_err(|e| {
            eprintln!("community member status update failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 { return Err(StatusCode::FORBIDDEN); }
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_community_member(
    State(pool): State<PgPool>, headers: HeaderMap, Path((community_id, member_id)): Path<(i32, i32)>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query("DELETE FROM community_members cm USING communities c WHERE cm.community_id=c.id AND cm.community_id=$1 AND cm.user_id=$2 AND cm.user_id<>c.created_by AND NOT cm.is_admin AND (c.created_by=$3 OR EXISTS(SELECT 1 FROM community_members actor WHERE actor.community_id=c.id AND actor.user_id=$3 AND actor.is_enabled AND actor.is_admin))")
        .bind(community_id).bind(member_id).bind(user_id).execute(&pool).await.map_err(|e| {
            eprintln!("community member removal failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 { return Err(StatusCode::FORBIDDEN); }
    Ok(StatusCode::NO_CONTENT)
}

async fn set_community_member_admin(
    State(pool): State<PgPool>, headers: HeaderMap, Path((community_id, member_id)): Path<(i32, i32)>, Json(input): Json<CommunityMemberAdmin>,
) -> Result<StatusCode, StatusCode> {
    let owner_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query("UPDATE community_members cm SET is_admin=$1 FROM communities c WHERE cm.community_id=c.id AND cm.community_id=$2 AND cm.user_id=$3 AND cm.user_id<>c.created_by AND c.created_by=$4")
        .bind(input.is_admin).bind(community_id).bind(member_id).bind(owner_id).execute(&pool).await.map_err(|e| {
            eprintln!("community moderator role update failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected()==0 { return Err(StatusCode::FORBIDDEN); }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_community_posts(
    State(pool): State<PgPool>, headers: HeaderMap, Path(community_id): Path<i32>,
) -> Result<Json<Vec<CommunityPost>>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let member: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM community_members WHERE community_id=$1 AND user_id=$2 AND is_enabled)")
        .bind(community_id).bind(user_id).fetch_one(&pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !member { return Err(StatusCode::FORBIDDEN); }
    let posts = sqlx::query_as::<_, CommunityPost>(
        r#"SELECT p.id, u.username, u.avatar_url, p.title, p.body, p.is_html, (p.edited_at IS NOT NULL) AS edited, (p.user_id=$2) AS can_edit,
                  to_char(p.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
                  (SELECT COUNT(*) FROM community_comments cc WHERE cc.post_id=p.id) AS comment_count,
                  (p.user_id=$2 OR c.created_by=$2 OR EXISTS(SELECT 1 FROM community_members mod WHERE mod.community_id=c.id AND mod.user_id=$2 AND mod.is_enabled AND mod.is_admin)) AS can_delete
           FROM community_posts p JOIN users u ON u.id=p.user_id JOIN communities c ON c.id=p.community_id
           WHERE p.community_id=$1 ORDER BY p.created_at DESC, p.id DESC LIMIT 200"#,
    ).bind(community_id).bind(user_id).fetch_all(&pool).await.map_err(|e| {
        eprintln!("community posts lookup failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(posts))
}

async fn create_community_post(
    State(pool): State<PgPool>, headers: HeaderMap, Path(community_id): Path<i32>, Json(input): Json<CommunityPostInput>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let title=input.title.trim(); let cleaned=clean_rich_text(input.body.trim()); let body=cleaned.as_str();
    if title.is_empty() || title.chars().count()>120 || rich_is_blank(body) || body.chars().count()>20000 { return Err(StatusCode::BAD_REQUEST); }
    let result=sqlx::query("INSERT INTO community_posts(community_id,user_id,title,body,is_html) SELECT $1,$2,$3,$4,TRUE WHERE EXISTS(SELECT 1 FROM community_members WHERE community_id=$1 AND user_id=$2 AND is_enabled)")
        .bind(community_id).bind(user_id).bind(title).bind(body).execute(&pool).await.map_err(|e| { eprintln!("community post create failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR })?;
    if result.rows_affected()==0 { return Err(StatusCode::FORBIDDEN); }
    Ok(StatusCode::CREATED)
}

async fn delete_community_post(
    State(pool): State<PgPool>, headers: HeaderMap, Path((community_id, post_id)): Path<(i32, i64)>,
) -> Result<StatusCode, StatusCode> {
    let user_id=current_user_id(&pool,&headers).await?;
    let result=sqlx::query("DELETE FROM community_posts p USING communities c WHERE p.community_id=c.id AND p.community_id=$1 AND p.id=$2 AND (p.user_id=$3 OR c.created_by=$3 OR EXISTS(SELECT 1 FROM community_members mod WHERE mod.community_id=c.id AND mod.user_id=$3 AND mod.is_enabled AND mod.is_admin))")
        .bind(community_id).bind(post_id).bind(user_id).execute(&pool).await.map_err(|e| { eprintln!("community post delete failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR })?;
    if result.rows_affected()==0 { return Err(StatusCode::FORBIDDEN); }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_community_comments(
    State(pool): State<PgPool>, headers: HeaderMap, Path((community_id, post_id)): Path<(i32, i64)>,
) -> Result<Json<Vec<CommunityComment>>, StatusCode> {
    let user_id=current_user_id(&pool,&headers).await?;
    let member:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM community_members WHERE community_id=$1 AND user_id=$2 AND is_enabled)")
        .bind(community_id).bind(user_id).fetch_one(&pool).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !member { return Err(StatusCode::FORBIDDEN); }
    let rows=sqlx::query_as::<_,CommunityComment>(
        r#"SELECT cc.id,u.username,cc.body,to_char(cc.created_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
                  (cc.user_id=$3 OR c.created_by=$3 OR EXISTS(SELECT 1 FROM community_members mod WHERE mod.community_id=c.id AND mod.user_id=$3 AND mod.is_enabled AND mod.is_admin)) AS can_delete
           FROM community_comments cc JOIN users u ON u.id=cc.user_id JOIN communities c ON c.id=cc.community_id
           WHERE cc.community_id=$1 AND cc.post_id=$2 ORDER BY cc.created_at,cc.id"#
    ).bind(community_id).bind(post_id).bind(user_id).fetch_all(&pool).await.map_err(|e| { eprintln!("community comments lookup failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR })?;
    Ok(Json(rows))
}

async fn create_community_comment(
    State(pool): State<PgPool>, headers: HeaderMap, Path((community_id, post_id)): Path<(i32, i64)>, Json(input): Json<CommunityCommentInput>,
) -> Result<StatusCode, StatusCode> {
    let user_id=current_user_id(&pool,&headers).await?;
    let body=input.body.trim(); if body.is_empty() || body.chars().count()>2000 { return Err(StatusCode::BAD_REQUEST); }
    let result=sqlx::query("INSERT INTO community_comments(community_id,post_id,user_id,body) SELECT $1,$2,$3,$4 WHERE EXISTS(SELECT 1 FROM community_members WHERE community_id=$1 AND user_id=$3 AND is_enabled) AND EXISTS(SELECT 1 FROM community_posts WHERE id=$2 AND community_id=$1)")
        .bind(community_id).bind(post_id).bind(user_id).bind(body).execute(&pool).await.map_err(|e| { eprintln!("community comment create failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR })?;
    if result.rows_affected()==0 { return Err(StatusCode::FORBIDDEN); }
    Ok(StatusCode::CREATED)
}

async fn delete_community_comment(
    State(pool): State<PgPool>, headers: HeaderMap, Path((community_id, post_id, comment_id)): Path<(i32, i64, i64)>,
) -> Result<StatusCode, StatusCode> {
    let user_id=current_user_id(&pool,&headers).await?;
    let result=sqlx::query("DELETE FROM community_comments cc USING communities c WHERE cc.community_id=c.id AND cc.community_id=$1 AND cc.post_id=$2 AND cc.id=$3 AND (cc.user_id=$4 OR c.created_by=$4 OR EXISTS(SELECT 1 FROM community_members mod WHERE mod.community_id=c.id AND mod.user_id=$4 AND mod.is_enabled AND mod.is_admin))")
        .bind(community_id).bind(post_id).bind(comment_id).bind(user_id).execute(&pool).await.map_err(|e| { eprintln!("community comment delete failed: {e}"); StatusCode::INTERNAL_SERVER_ERROR })?;
    if result.rows_affected()==0 { return Err(StatusCode::FORBIDDEN); }
    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_user_settings_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"ALTER TABLE users
            ADD COLUMN IF NOT EXISTS date_of_birth DATE,
            ADD COLUMN IF NOT EXISTS birthday_visibility TEXT NOT NULL DEFAULT 'self',
            ADD COLUMN IF NOT EXISTS company_name TEXT,
            ADD COLUMN IF NOT EXISTS designation TEXT,
            ADD COLUMN IF NOT EXISTS work_visibility TEXT NOT NULL DEFAULT 'self',
            ADD COLUMN IF NOT EXISTS contact_email TEXT,
            ADD COLUMN IF NOT EXISTS email_visibility TEXT NOT NULL DEFAULT 'self',
            ADD COLUMN IF NOT EXISTS phone_visibility TEXT NOT NULL DEFAULT 'self',
            ADD COLUMN IF NOT EXISTS instagram_handle TEXT,
            ADD COLUMN IF NOT EXISTS facebook_handle TEXT,
            ADD COLUMN IF NOT EXISTS tiktok_handle TEXT,
            ADD COLUMN IF NOT EXISTS linkedin_handle TEXT,
            ADD COLUMN IF NOT EXISTS social_visibility TEXT NOT NULL DEFAULT 'self'"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS communities (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            image_url TEXT,
            created_by INT REFERENCES users(id) ON DELETE SET NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
    ).execute(pool).await?;
    sqlx::query("ALTER TABLE communities ADD COLUMN IF NOT EXISTS image_url TEXT")
        .execute(pool).await?;
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS communities_name_lower_idx ON communities (lower(name))")
        .execute(pool).await?;
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS community_members (
            community_id INT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
            user_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
            joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            PRIMARY KEY (community_id, user_id)
        )"#,
    ).execute(pool).await?;
    sqlx::query("ALTER TABLE community_members ADD COLUMN IF NOT EXISTS is_enabled BOOLEAN NOT NULL DEFAULT TRUE")
        .execute(pool).await?;
    sqlx::query("ALTER TABLE community_members ADD COLUMN IF NOT EXISTS is_admin BOOLEAN NOT NULL DEFAULT FALSE")
        .execute(pool).await?;
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS community_posts (
            id BIGSERIAL PRIMARY KEY,
            community_id INT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
            user_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            title TEXT NOT NULL,
            body TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
    ).execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS community_posts_feed_idx ON community_posts (community_id, created_at DESC)")
        .execute(pool).await?;
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS community_comments (
            id BIGSERIAL PRIMARY KEY,
            community_id INT NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
            post_id BIGINT NOT NULL REFERENCES community_posts(id) ON DELETE CASCADE,
            user_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            body TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
    ).execute(pool).await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS community_comments_post_idx ON community_comments (post_id, created_at)")
        .execute(pool).await?;
    sqlx::query("INSERT INTO communities (name, description) VALUES ('Men', 'A place for men to connect and support one another.'), ('Women', 'A welcoming space for women to share and support.'), ('Tech', 'Discuss technology, products, careers, and ideas.'), ('Cricket', 'Talk matches, teams, players, and cricket news.') ON CONFLICT DO NOTHING")
        .execute(pool).await?;
    sqlx::query(
        r#"
        DO $$
        BEGIN
            IF NOT EXISTS (
                SELECT 1
                FROM information_schema.columns
                WHERE table_schema = 'public'
                  AND table_name = 'users'
                  AND column_name = 'profile_visibility'
            ) THEN
                ALTER TABLE users ADD COLUMN profile_visibility TEXT NOT NULL DEFAULT 'public';
            END IF;

            IF NOT EXISTS (
                SELECT 1
                FROM information_schema.columns
                WHERE table_schema = 'public'
                  AND table_name = 'users'
                  AND column_name = 'message_privacy'
            ) THEN
                ALTER TABLE users ADD COLUMN message_privacy TEXT NOT NULL DEFAULT 'everyone';
            END IF;

            IF NOT EXISTS (
                SELECT 1
                FROM information_schema.columns
                WHERE table_schema = 'public'
                  AND table_name = 'users'
                  AND column_name = 'show_activity_status'
            ) THEN
                ALTER TABLE users ADD COLUMN show_activity_status BOOLEAN NOT NULL DEFAULT TRUE;
            END IF;

            IF NOT EXISTS (
                SELECT 1
                FROM information_schema.columns
                WHERE table_schema = 'public'
                  AND table_name = 'users'
                  AND column_name = 'keep_profile_visitors'
            ) THEN
                ALTER TABLE users ADD COLUMN keep_profile_visitors BOOLEAN NOT NULL DEFAULT TRUE;
            END IF;

            IF NOT EXISTS (
                SELECT 1
                FROM information_schema.columns
                WHERE table_schema = 'public'
                  AND table_name = 'users'
                  AND column_name = 'notify_mentions'
            ) THEN
                ALTER TABLE users ADD COLUMN notify_mentions BOOLEAN NOT NULL DEFAULT TRUE;
            END IF;

            IF NOT EXISTS (
                SELECT 1
                FROM information_schema.columns
                WHERE table_schema = 'public'
                  AND table_name = 'users'
                  AND column_name = 'account_enabled'
            ) THEN
                ALTER TABLE users ADD COLUMN account_enabled BOOLEAN NOT NULL DEFAULT TRUE;
            END IF;

            IF NOT EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'is_admin'
            ) THEN
                ALTER TABLE users ADD COLUMN is_admin BOOLEAN NOT NULL DEFAULT FALSE;
            END IF;

            IF NOT EXISTS (
                SELECT 1
                FROM information_schema.columns
                WHERE table_schema = 'public'
                  AND table_name = 'users'
                  AND column_name = 'mood'
            ) THEN
                ALTER TABLE users ADD COLUMN mood TEXT
                    CHECK (mood IS NULL OR mood IN (
                        'Bored', 'Romantic', 'Angry', 'Sad', 'Happy', 'Calm',
                        'Excited', 'Playful', 'Lonely', 'Grateful', 'Stressed'
                    ));
            END IF;

            IF NOT EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'email'
            ) THEN
                ALTER TABLE users ADD COLUMN email TEXT;
            END IF;

            IF NOT EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'first_name'
            ) THEN
                ALTER TABLE users ADD COLUMN first_name TEXT;
            END IF;

            IF NOT EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'last_name'
            ) THEN
                ALTER TABLE users ADD COLUMN last_name TEXT;
            END IF;

            IF NOT EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'phone_number'
            ) THEN
                ALTER TABLE users ADD COLUMN phone_number TEXT;
            END IF;

            IF NOT EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'cover_position_x'
            ) THEN
                ALTER TABLE users ADD COLUMN cover_position_x INT NOT NULL DEFAULT 50
                    CHECK (cover_position_x BETWEEN 0 AND 100);
            END IF;

            IF NOT EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'cover_position_y'
            ) THEN
                ALTER TABLE users ADD COLUMN cover_position_y INT NOT NULL DEFAULT 50
                    CHECK (cover_position_y BETWEEN 0 AND 100);
            END IF;

            IF NOT EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'cover_zoom'
            ) THEN
                ALTER TABLE users ADD COLUMN cover_zoom INT NOT NULL DEFAULT 100
                    CHECK (cover_zoom BETWEEN 100 AND 200);
            END IF;
        END $$;
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("ALTER TABLE conversation_members ADD COLUMN IF NOT EXISTS last_read_at TIMESTAMPTZ")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE conversations ADD COLUMN IF NOT EXISTS is_muted BOOLEAN NOT NULL DEFAULT FALSE")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS conversation_reports (
            id BIGSERIAL PRIMARY KEY,
            conversation_id INT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
            reporter_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            details TEXT NOT NULL DEFAULT '',
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            UNIQUE (conversation_id, reporter_id),
            CHECK (char_length(details) <= 500)
        )"#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
    )
    .execute(pool)
    .await?;
    sqlx::query("INSERT INTO app_settings (key, value) VALUES ('allow_signups', 'true') ON CONFLICT (key) DO NOTHING")
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO app_settings (key, value) VALUES ('app_enabled', 'true') ON CONFLICT (key) DO NOTHING")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE users ADD COLUMN IF NOT EXISTS is_superadmin BOOLEAN NOT NULL DEFAULT FALSE")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS user_blocks (
            blocker_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            blocked_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            PRIMARY KEY (blocker_id, blocked_id),
            CHECK (blocker_id <> blocked_id)
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        ALTER TABLE activity_checkins
            ADD COLUMN IF NOT EXISTS latitude DOUBLE PRECISION,
            ADD COLUMN IF NOT EXISTS longitude DOUBLE PRECISION;
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        DO $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM pg_constraint
                WHERE conrelid = 'activity_checkins'::regclass
                  AND conname = 'activity_checkins_category_check'
                  AND pg_get_constraintdef(oid) NOT LIKE '%hotels%'
            ) THEN
                ALTER TABLE activity_checkins
                    DROP CONSTRAINT activity_checkins_category_check;
            END IF;

            IF NOT EXISTS (
                SELECT 1 FROM pg_constraint
                WHERE conrelid = 'activity_checkins'::regclass
                  AND conname = 'activity_checkins_category_check'
            ) THEN
                ALTER TABLE activity_checkins
                    ADD CONSTRAINT activity_checkins_category_check
                    CHECK (category IN (
                        'nightclubs', 'bars', 'pubs', 'restaurants', 'hotels',
                        'movies', 'concerts', 'travel', 'dining', 'outdoors'
                    ));
            END IF;

            IF NOT EXISTS (
                SELECT 1 FROM pg_constraint
                WHERE conrelid = 'activity_checkins'::regclass
                  AND conname = 'activity_checkins_coordinates_check'
            ) THEN
                ALTER TABLE activity_checkins
                    ADD CONSTRAINT activity_checkins_coordinates_check
                    CHECK (
                        (latitude IS NULL AND longitude IS NULL)
                        OR (
                            latitude BETWEEN -90 AND 90
                            AND longitude BETWEEN -180 AND 180
                        )
                    );
            END IF;
        END $$;
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS live_locations (
            user_id INT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
            latitude DOUBLE PRECISION NOT NULL CHECK (latitude BETWEEN -90 AND 90),
            longitude DOUBLE PRECISION NOT NULL CHECK (longitude BETWEEN -180 AND 180),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS live_locations_updated_idx ON live_locations (updated_at DESC)",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS users_email_unique_idx ON users (email) WHERE email IS NOT NULL")
        .execute(pool)
        .await?;

    Ok(())
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
    if std::env::var("RUN_MIGRATIONS").map(|v| v == "1").unwrap_or(false) {
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("database migrations failed");
    }
    ensure_user_settings_schema(&pool)
        .await
        .expect("user settings schema could not be initialized");
    if let Ok(admin_usernames) = std::env::var("ADMIN_USERNAMES") {
        let names: Vec<String> = admin_usernames
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if !names.is_empty() {
            sqlx::query("UPDATE users SET is_admin = TRUE WHERE username = ANY($1)")
                .bind(names)
                .execute(&pool)
                .await
                .expect("could not apply ADMIN_USERNAMES");
        }
    }
    if let Ok(superadmin_usernames) = std::env::var("SUPERADMIN_USERNAMES") {
        let names: Vec<String> = superadmin_usernames
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if !names.is_empty() {
            sqlx::query("UPDATE users SET is_admin = TRUE, is_superadmin = TRUE WHERE username = ANY($1)")
                .bind(names)
                .execute(&pool)
                .await
                .expect("could not apply SUPERADMIN_USERNAMES");
        }
    }

    let app = Router::new()
        .route("/api/feed", get(feed))
        .route("/api/signup", post(signup))
        .route("/api/login", post(login))
        .route("/api/me", get(me))
        .route("/api/status", get(app_status))
        .route("/api/admin/overview", get(admin_overview))
        .route("/api/admin/users/{id}", put(admin_update_user).delete(admin_delete_user))
        .route("/api/admin/users/{id}/role", put(admin_update_role))
        .route("/api/admin/conversations/{id}", delete(admin_delete_conversation))
        .route("/api/admin/conversations/{id}/moderation", put(admin_update_conversation))
        .route("/api/chat-media/search", get(search_chat_media))
        .route("/api/locations/search", get(search_locations))
        .route("/api/admin/config", put(admin_update_config))
        .route(
            "/api/me/settings",
            get(get_user_settings).put(update_user_settings),
        )
        .route(
            "/api/me/blocked-users",
            get(list_blocked_users).post(block_user),
        )
        .route("/api/me/blocked-users/{username}", delete(unblock_user))
        .route("/api/me/activity", get(my_activity))
        .route("/api/me/dashboard", get(settings_dashboard))
        .route("/api/checkins", get(list_checkins).post(create_checkin))
        .route("/api/news", get(get_news))
        .route("/api/checkins/{id}", delete(delete_checkin))
        .route("/api/feedback", axum::routing::post(submit_feedback))
        .route("/api/stories", get(story_tray).post(create_story))
        .route("/api/stories/{id}", delete(delete_story))
        .route("/api/stories/{id}/view", axum::routing::post(view_story))
        .route("/api/users/{username}/stories", get(user_stories))
        .route("/api/posts/{id}", delete(delete_post))
        .route("/api/posts/{id}/comments/{comment_id}", delete(delete_comment))
        .route("/api/locations/live", get(list_live_locations))
        .route(
            "/api/me/location",
            post(update_live_location).delete(remove_live_location),
        )
        .route("/api/me/location/stop", post(stop_live_location_beacon))
        .route("/api/me/presence", post(record_presence))
        .route(
            "/api/me/personal-chat-requests",
            get(list_personal_chat_requests),
        )
        .route(
            "/api/personal-chat-requests/{id}/accept",
            post(accept_personal_chat_request),
        )
        .route(
            "/api/personal-chat-requests/{id}/reject",
            post(reject_personal_chat_request),
        )
        .route("/api/users/active", get(active_users))
        .route(
            "/api/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route(
            "/api/conversations/{id}/messages",
            get(list_messages)
                .post(send_message)
                .delete(delete_group_messages)
                .layer(DefaultBodyLimit::max(51 * 1024 * 1024)),
        )
        .route("/api/conversations/{id}/messages/{message_id}", delete(delete_message))
        .route("/api/conversations/{id}/members", get(list_group_members))
        .route("/api/conversations/{id}/members/{member_id}", delete(remove_group_member))
        .route("/api/conversations/{id}/group", delete(delete_group))
        .route("/api/conversations/{id}", put(update_group).delete(delete_conversation))
        .route("/api/conversations/{id}/report", post(report_conversation))
        .route("/api/users/search", get(search_users))
        .route("/api/messages/{id}/image", get(read_message_image))
        .route("/api/logout", post(logout))
        .route("/api/me/profile", put(update_profile))
        .route(
            "/api/me/avatar",
            post(upload_avatar)
                .delete(remove_avatar)
                .layer(DefaultBodyLimit::max(5 * 1024 * 1024)),
        )
        .route(
            "/api/me/cover",
            post(upload_home_cover)
                .delete(remove_home_cover)
                .put(update_home_cover_position)
                .layer(DefaultBodyLimit::max(5 * 1024 * 1024)),
        )
        .route(
            "/api/posts",
            post(create_post).layer(DefaultBodyLimit::max(52 * 1024 * 1024)),
        )
        .route("/api/posts/{id}/like", post(like_post).delete(unlike_post))
        .route(
            "/api/posts/{id}/dislike",
            post(dislike_post).delete(undislike_post),
        )
        .route("/api/posts/{id}/hate", post(hate_post).delete(unhate_post))
        .route(
            "/api/posts/{id}/reshare",
            post(reshare_post).delete(undo_reshare),
        )
        .route("/api/posts/{id}/report", post(report_post))
        .route(
            "/api/posts/{id}/comments",
            get(list_comments).post(add_comment),
        )
        .route("/api/users/{username}", get(profile))
        .route(
            "/api/users/{username}/connections",
            get(profile_connections),
        )
        .route("/api/users/{username}/admirers", get(profile_admirers))
        .route(
            "/api/users/{username}/admire",
            post(admire_user).delete(unadmire_user),
        )
        .route(
            "/api/users/{username}/personal-chat-requests",
            post(create_personal_chat_request),
        )
        .route(
            "/api/users/{username}/follow",
            post(follow_user).delete(unfollow_user),
        )
        .route(
            "/api/users/{username}/favorite",
            post(favorite_user).delete(unfavorite_user),
        )
        .route("/api/me/friends", get(list_friends))
        .route("/api/communities", get(list_communities).post(create_community))
        .route("/api/communities/{id}", put(update_community).delete(delete_community))
        .route(
            "/api/communities/{id}/image",
            post(upload_community_image).layer(DefaultBodyLimit::max(5 * 1024 * 1024 + 64 * 1024)),
        )
        .route("/api/communities/{id}/join", post(join_community).delete(leave_community))
        .route("/api/communities/{id}/active", get(list_active_community_members))
        .route("/api/communities/{id}/posts/{post_id}/report", post(report_community_post))
        .route("/api/communities/{id}/claim", post(claim_community))
        .route("/api/communities/{id}/members", get(list_community_members).post(add_community_member))
        .route("/api/communities/{id}/members/{member_id}", delete(remove_community_member))
        .route("/api/communities/{id}/members/{member_id}/status", put(set_community_member_status))
        .route("/api/communities/{id}/members/{member_id}/admin", put(set_community_member_admin))
        .route("/api/communities/{id}/posts", get(list_community_posts).post(create_community_post))
        .route("/api/communities/{id}/posts/{post_id}", delete(delete_community_post).put(edit_community_post))
        .route("/api/communities/{id}/posts/{post_id}/comments", get(list_community_comments).post(create_community_comment))
        .route("/api/communities/{id}/posts/{post_id}/comments/{comment_id}", delete(delete_community_comment))
        .route_layer(middleware::from_fn_with_state(
            pool.clone(),
            app_enabled_guard,
        ))
        .fallback_service(ServeDir::new("static"))
        .with_state(pool);

    let listener = tokio::net::TcpListener::bind(std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string()))
        .await
        .unwrap();
    println!("Running on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::{
        checkin_category_is_valid, coordinates_are_valid, mentioned_usernames, mood_is_valid,
        tagged_usernames, video_extension,
    };
    use axum::http::StatusCode;

    #[test]
    fn accepts_only_supported_checkin_categories() {
        for category in [
            "nightclubs",
            "bars",
            "pubs",
            "restaurants",
            "hotels",
            "movies",
            "concerts",
            "travel",
            "dining",
            "outdoors",
        ] {
            assert!(checkin_category_is_valid(category));
        }
        assert!(!checkin_category_is_valid("unknown"));
    }

    #[test]
    fn accepts_only_valid_map_coordinates() {
        assert!(coordinates_are_valid(0.0, 0.0));
        assert!(coordinates_are_valid(90.0, 180.0));
        assert!(coordinates_are_valid(-90.0, -180.0));
        assert!(!coordinates_are_valid(90.1, 0.0));
        assert!(!coordinates_are_valid(0.0, -180.1));
        assert!(!coordinates_are_valid(f64::NAN, 0.0));
        assert!(!coordinates_are_valid(0.0, f64::INFINITY));
    }

    #[test]
    fn accepts_only_age_appropriate_profile_moods() {
        for mood in [
            "Bored", "Romantic", "Angry", "Sad", "Happy", "Calm", "Excited", "Playful", "Lonely",
            "Grateful", "Stressed",
        ] {
            assert!(mood_is_valid(mood));
        }
        assert!(!mood_is_valid("Sexually aroused"));
        assert!(!mood_is_valid("unknown"));
    }

    #[test]
    fn recognizes_only_mp4_and_webm_video_signatures() {
        assert_eq!(video_extension(b"\0\0\0\x18ftypisom"), Some("mp4"));
        assert_eq!(video_extension(b"\x1A\x45\xDF\xA3webm"), Some("webm"));
        assert_eq!(video_extension(b"not a video file"), None);
    }

    #[test]
    fn finds_unique_mentions_without_matching_email_addresses() {
        assert_eq!(
            mentioned_usernames("Hi @chand_2, cc @mina! @chand_2 test@example.com"),
            vec!["chand_2", "mina"]
        );
    }

    #[test]
    fn parses_unique_photo_tags_and_rejects_invalid_or_excessive_tags() {
        assert_eq!(
            tagged_usernames("mina, ravi, mina").unwrap(),
            vec!["mina", "ravi"]
        );
        assert_eq!(tagged_usernames("not valid"), Err(StatusCode::BAD_REQUEST));
        assert_eq!(
            tagged_usernames("a,b,c,d,e,f,g,h,i,j,k"),
            Err(StatusCode::BAD_REQUEST)
        );
    }
}

// ---------- top stories (Google News RSS, cached 15 minutes) ----------
#[derive(serde::Serialize, Clone)]
struct NewsItem {
    title: String,
    link: String,
    source: String,
}

#[derive(serde::Serialize, Clone)]
struct NewsPayload {
    local: Vec<NewsItem>,
    world: Vec<NewsItem>,
    tech: Vec<NewsItem>,
}

static NEWS_CACHE: std::sync::OnceLock<tokio::sync::Mutex<Option<(std::time::Instant, NewsPayload)>>> =
    std::sync::OnceLock::new();

const NEWS_TTL: std::time::Duration = std::time::Duration::from_secs(15 * 60);
const NEWS_LOCAL: &str = "https://news.google.com/rss?hl=en-IN&gl=IN&ceid=IN:en";
const NEWS_WORLD: &str = "https://news.google.com/rss/headlines/section/topic/WORLD?hl=en-IN&gl=IN&ceid=IN:en";
const NEWS_TECH: &str = "https://news.google.com/rss/headlines/section/topic/TECHNOLOGY?hl=en-IN&gl=IN&ceid=IN:en";

async fn fetch_feed(client: &reqwest::Client, url: &str) -> Vec<NewsItem> {
    let text = match client.get(url).send().await.and_then(|r| r.error_for_status()) {
        Ok(resp) => match resp.text().await {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        },
        Err(_) => return Vec::new(),
    };
    let doc = match roxmltree::Document::parse(&text) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    doc.descendants()
        .filter(|n| n.has_tag_name("item"))
        .take(3)
        .filter_map(|item| {
            let get = |name: &str| {
                item.children()
                    .find(|c| c.has_tag_name(name))
                    .and_then(|c| c.text())
                    .map(|s| s.trim().to_string())
            };
            let full_title = get("title")?;
            let link = get("link")?;
            let source = get("source").unwrap_or_default();
            // Google News titles end with " - Source Name"
            let title = match full_title.rsplit_once(" - ") {
                Some((t, _)) if !source.is_empty() => t.to_string(),
                _ => full_title,
            };
            Some(NewsItem { title, link, source })
        })
        .collect()
}

async fn get_news() -> Result<axum::Json<NewsPayload>, axum::http::StatusCode> {
    let cache = NEWS_CACHE.get_or_init(|| tokio::sync::Mutex::new(None));
    let mut guard = cache.lock().await;
    if let Some((at, data)) = guard.as_ref() {
        if at.elapsed() < NEWS_TTL {
            return Ok(axum::Json(data.clone()));
        }
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("Community/1.0")
        .build()
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    let (local, world, tech) = tokio::join!(
        fetch_feed(&client, NEWS_LOCAL),
        fetch_feed(&client, NEWS_WORLD),
        fetch_feed(&client, NEWS_TECH),
    );
    let payload = NewsPayload { local, world, tech };
    if payload.local.is_empty() && payload.world.is_empty() && payload.tech.is_empty() {
        // all three failed: serve the last good copy if we have one
        if let Some((_, data)) = guard.as_ref() {
            return Ok(axum::Json(data.clone()));
        }
        return Err(axum::http::StatusCode::BAD_GATEWAY);
    }
    *guard = Some((std::time::Instant::now(), payload.clone()));
    Ok(axum::Json(payload))
}
// ---------- delete a post / delete a comment ----------
async fn delete_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(post_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let result = sqlx::query("DELETE FROM posts WHERE id = $1 AND user_id = $2")
        .bind(post_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            eprintln!("post delete failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_comment(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path((post_id, comment_id)): Path<(i32, i32)>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    // the comment's author or the post's owner may delete it
    let result = sqlx::query(
        "DELETE FROM comments AS c USING posts AS p
         WHERE c.id = $1 AND c.post_id = $2 AND p.id = c.post_id
           AND (c.user_id = $3 OR p.user_id = $3)",
    )
    .bind(comment_id)
    .bind(post_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("comment delete failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
// ---------- stories (24-hour text / image / video posts) ----------
const STORY_MOODS: [&str; 11] = [
    "Bored", "Romantic", "Angry", "Sad", "Happy", "Calm",
    "Excited", "Playful", "Lonely", "Grateful", "Stressed",
];

#[derive(serde::Serialize, sqlx::FromRow)]
struct StoryTrayItem {
    username: String,
    avatar_url: Option<String>,
    story_count: i64,
    all_seen: bool,
    thumb_kind: String,
    thumb_url: Option<String>,
    thumb_bg: String,
    thumb_body: String,
}

#[derive(serde::Serialize, sqlx::FromRow)]
struct StoryItem {
    id: i32,
    kind: String,
    body: String,
    media_url: Option<String>,
    bg_color: String,
    mood: Option<String>,
    created_at: String,
    viewed: bool,
    view_count: i64,
}

async fn purge_expired_stories(pool: &PgPool) {
    let rows: Vec<(Option<String>,)> = sqlx::query_as(
        "DELETE FROM stories WHERE expires_at <= now() - interval '1 hour' RETURNING media_url",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (url,) in rows {
        if let Some(u) = url {
            remove_upload(&u).await;
        }
    }
}

async fn create_story(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let account_enabled: bool =
        sqlx::query_scalar("SELECT account_enabled FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .map_err(|e| {
                eprintln!("story account status lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    if !account_enabled {
        return Err(StatusCode::FORBIDDEN);
    }

    let mut body = String::new();
    let mut mood: Option<String> = None;
    let mut bg = String::from("#6a54c6");
    let mut media: Option<(Vec<u8>, &'static str, &'static str)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        let name = field.name().map(|s| s.to_string());
        match name.as_deref() {
            Some("body") => {
                body = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            }
            Some("mood") => {
                let m = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                let m = m.trim().to_string();
                if !m.is_empty() {
                    mood = Some(m);
                }
            }
            Some("bg_color") => {
                bg = field
                    .text()
                    .await
                    .map_err(|_| StatusCode::BAD_REQUEST)?
                    .trim()
                    .to_string();
            }
            Some("media") => {
                let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                if bytes.is_empty() {
                    continue;
                }
                if bytes.len() > 25 * 1024 * 1024 {
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
                let uploaded = if let Some(ext) = image_extension(&bytes) {
                    (bytes.to_vec(), ext, "image")
                } else if let Some(ext) = video_extension(&bytes) {
                    (bytes.to_vec(), ext, "video")
                } else {
                    return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
                };
                media = Some(uploaded);
            }
            _ => {}
        }
    }

    let body = body.trim();
    if body.chars().count() > 280 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if media.is_none() && body.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if let Some(m) = &mood {
        if !STORY_MOODS.contains(&m.as_str()) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    let bg_ok = bg.len() == 7
        && bg.starts_with('#')
        && bg[1..].chars().all(|c| c.is_ascii_hexdigit());
    if !bg_ok {
        return Err(StatusCode::BAD_REQUEST);
    }

    purge_expired_stories(&pool).await;

    let (kind, url): (&str, Option<String>) = match media {
        Some((bytes, ext, media_type)) => {
            let saved = save_image(&bytes, ext).await?;
            (media_type, Some(saved))
        }
        None => ("text", None),
    };

    let result = sqlx::query(
        "INSERT INTO stories (user_id, kind, body, media_url, bg_color, mood)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(user_id)
    .bind(kind)
    .bind(body)
    .bind(&url)
    .bind(&bg)
    .bind(&mood)
    .execute(&pool)
    .await;
    if let Err(e) = result {
        if let Some(u) = &url {
            remove_upload(u).await;
        }
        eprintln!("create story failed: {e}");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    Ok(StatusCode::CREATED)
}

async fn story_tray(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<Vec<StoryTrayItem>>, StatusCode> {
    let me = current_user_id(&pool, &headers).await?;
    let rows = sqlx::query_as::<_, StoryTrayItem>(
        r#"
        SELECT u.username,
               u.avatar_url,
               COUNT(s.id) AS story_count,
               COALESCE(BOOL_AND(v.story_id IS NOT NULL), false) AS all_seen,
               (ARRAY_AGG(s.kind ORDER BY s.created_at DESC))[1] AS thumb_kind,
               (ARRAY_AGG(s.media_url ORDER BY s.created_at DESC))[1] AS thumb_url,
               (ARRAY_AGG(s.bg_color ORDER BY s.created_at DESC))[1] AS thumb_bg,
               (ARRAY_AGG(s.body ORDER BY s.created_at DESC))[1] AS thumb_body
        FROM stories s
        JOIN users u ON u.id = s.user_id
        LEFT JOIN story_views v ON v.story_id = s.id AND v.viewer_id = $1
        WHERE s.expires_at > now()
          AND (s.user_id = $1
               OR s.user_id IN (SELECT followee_id FROM follows WHERE follower_id = $1)
               OR (s.user_id IN (SELECT follower_id FROM follows WHERE followee_id = $1)
                   AND u.profile_visibility = 'public'))
          AND NOT EXISTS (
              SELECT 1 FROM user_blocks b
              WHERE b.blocker_id = s.user_id AND b.blocked_id = $1)
        GROUP BY u.id, u.username, u.avatar_url
        ORDER BY (u.id = $1) DESC, all_seen ASC, MAX(s.created_at) DESC
        LIMIT 50
        "#,
    )
    .bind(me)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("story tray failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows))
}

async fn user_stories(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<Json<Vec<StoryItem>>, StatusCode> {
    let me = current_user_id(&pool, &headers).await?;
    let target: Option<(i32, String)> =
        sqlx::query_as("SELECT id, profile_visibility FROM users WHERE username = $1")
            .bind(&username)
            .fetch_optional(&pool)
            .await
            .map_err(|e| {
                eprintln!("story user lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let (target_id, visibility) = target.ok_or(StatusCode::NOT_FOUND)?;

    if target_id != me {
        let blocked: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM user_blocks WHERE blocker_id = $1 AND blocked_id = $2)",
        )
        .bind(target_id)
        .bind(me)
        .fetch_one(&pool)
        .await
        .map_err(|e| {
            eprintln!("story block check failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
        if blocked {
            return Err(StatusCode::NOT_FOUND);
        }
        if visibility != "public" {
            let follows: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM follows WHERE follower_id = $1 AND followee_id = $2)",
            )
            .bind(me)
            .bind(target_id)
            .fetch_one(&pool)
            .await
            .map_err(|e| {
                eprintln!("story follow check failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            if !follows {
                return Err(StatusCode::FORBIDDEN);
            }
        }
    }

    let rows = sqlx::query_as::<_, StoryItem>(
        r#"
        SELECT s.id, s.kind, s.body, s.media_url, s.bg_color, s.mood,
               to_char(s.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
               EXISTS(SELECT 1 FROM story_views v WHERE v.story_id = s.id AND v.viewer_id = $1) AS viewed,
               CASE WHEN s.user_id = $1
                    THEN (SELECT COUNT(*) FROM story_views v2 WHERE v2.story_id = s.id)
                    ELSE 0::bigint END AS view_count
        FROM stories s
        WHERE s.user_id = $2 AND s.expires_at > now()
        ORDER BY s.created_at ASC
        LIMIT 30
        "#,
    )
    .bind(me)
    .bind(target_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("list stories failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows))
}

async fn view_story(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(story_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let me = current_user_id(&pool, &headers).await?;
    sqlx::query(
        "INSERT INTO story_views (story_id, viewer_id)
         SELECT s.id, $2 FROM stories s JOIN users u ON u.id = s.user_id
         WHERE s.id = $1 AND s.expires_at > now() AND s.user_id <> $2
           AND NOT EXISTS (SELECT 1 FROM user_blocks b
                           WHERE b.blocker_id = s.user_id AND b.blocked_id = $2)
           AND (u.profile_visibility = 'public'
                OR EXISTS (SELECT 1 FROM follows f
                           WHERE f.follower_id = $2 AND f.followee_id = s.user_id))
         ON CONFLICT DO NOTHING",
    )
    .bind(story_id)
    .bind(me)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("story view failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_story(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(story_id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let me = current_user_id(&pool, &headers).await?;
    let deleted: Option<(Option<String>,)> = sqlx::query_as(
        "DELETE FROM stories WHERE id = $1 AND user_id = $2 RETURNING media_url",
    )
    .bind(story_id)
    .bind(me)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        eprintln!("story delete failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    match deleted {
        None => Err(StatusCode::NOT_FOUND),
        Some((url,)) => {
            if let Some(u) = url {
                remove_upload(&u).await;
            }
            Ok(StatusCode::NO_CONTENT)
        }
    }
}
// ---------- tester feedback ----------
#[derive(serde::Deserialize)]
struct NewFeedback {
    category: String,
    message: String,
    page: Option<String>,
}

async fn submit_feedback(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(f): Json<NewFeedback>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let message = f.message.trim();
    let len = message.chars().count();
    if len < 3 || len > 1000 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !["bug", "idea", "praise", "other"].contains(&f.category.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let recent: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM feedback
         WHERE user_id = $1 AND created_at > now() - interval '1 hour'",
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("feedback rate check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if recent >= 5 {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    let page: String = f.page.unwrap_or_default().chars().take(120).collect();
    let ua: String = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .chars()
        .take(200)
        .collect();
    sqlx::query(
        "INSERT INTO feedback (user_id, category, message, page, user_agent)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(&f.category)
    .bind(message)
    .bind(&page)
    .bind(&ua)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("feedback insert failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(StatusCode::CREATED)
}

// ---------- community upgrades: rich text, edit, report, active members ----------
fn clean_rich_text(input: &str) -> String {
    let tags: std::collections::HashSet<&str> = [
        "p", "div", "br", "b", "strong", "i", "em", "u", "ul", "ol", "li", "a", "blockquote",
    ]
    .into_iter()
    .collect();
    let schemes: std::collections::HashSet<&str> = ["http", "https", "mailto"].into_iter().collect();
    ammonia::Builder::default()
        .tags(tags)
        .url_schemes(schemes)
        .link_rel(Some("noopener noreferrer nofollow"))
        .clean(input)
        .to_string()
}

fn rich_is_blank(html: &str) -> bool {
    let text = ammonia::Builder::empty().clean(html).to_string();
    text.replace("&nbsp;", " ").trim().is_empty()
}

#[derive(Deserialize)]
struct CommunityPostEdit {
    title: String,
    body: String,
}

#[derive(Deserialize)]
struct CommunityPostReport {
    reason: String,
    details: Option<String>,
}

#[derive(Serialize, FromRow)]
struct ActiveCommunityMember {
    id: i32,
    username: String,
    avatar_url: Option<String>,
}

async fn edit_community_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path((community_id, post_id)): Path<(i32, i64)>,
    Json(input): Json<CommunityPostEdit>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let title = input.title.trim();
    let body = clean_rich_text(input.body.trim());
    if title.is_empty() || title.chars().count() > 120 || rich_is_blank(&body) || body.chars().count() > 20000 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let result = sqlx::query(
        "UPDATE community_posts SET title=$1, body=$2, is_html=TRUE, edited_at=now()
         WHERE id=$3 AND community_id=$4 AND user_id=$5
           AND EXISTS(SELECT 1 FROM community_members WHERE community_id=$4 AND user_id=$5 AND is_enabled)",
    )
    .bind(title)
    .bind(&body)
    .bind(post_id)
    .bind(community_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("community post edit failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn report_community_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path((community_id, post_id)): Path<(i32, i64)>,
    Json(input): Json<CommunityPostReport>,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    if !["spam", "harassment", "hate", "nudity", "violence", "scam", "other"].contains(&input.reason.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let details = input.details.unwrap_or_default();
    let details = details.trim();
    if details.chars().count() > 500 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM community_posts p
           JOIN community_members m ON m.community_id = p.community_id AND m.user_id = $3 AND m.is_enabled
           WHERE p.id = $2 AND p.community_id = $1 AND p.user_id <> $3)",
    )
    .bind(community_id)
    .bind(post_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("community report check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !allowed {
        return Err(StatusCode::FORBIDDEN);
    }
    let result = sqlx::query(
        "INSERT INTO community_post_reports (community_id, post_id, reporter_id, reason, details)
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (post_id, reporter_id) DO NOTHING",
    )
    .bind(community_id)
    .bind(post_id)
    .bind(user_id)
    .bind(&input.reason)
    .bind(details)
    .execute(&pool)
    .await
    .map_err(|e| {
        eprintln!("community report failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if result.rows_affected() == 0 {
        return Err(StatusCode::CONFLICT);
    }
    Ok(StatusCode::CREATED)
}

async fn list_active_community_members(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(community_id): Path<i32>,
) -> Result<Json<Vec<ActiveCommunityMember>>, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let is_member: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM community_members WHERE community_id=$1 AND user_id=$2 AND is_enabled)",
    )
    .bind(community_id)
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        eprintln!("community active membership check failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    if !is_member {
        return Err(StatusCode::FORBIDDEN);
    }
    let rows = sqlx::query_as::<_, ActiveCommunityMember>(
        "SELECT u.id, u.username, u.avatar_url
         FROM community_members cm JOIN users u ON u.id = cm.user_id
         WHERE cm.community_id = $1 AND cm.is_enabled AND u.account_enabled AND u.show_activity_status
           AND u.last_active_at > now() - interval '5 minutes'
         ORDER BY u.last_active_at DESC LIMIT 12",
    )
    .bind(community_id)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("community active members failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows))
}