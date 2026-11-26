use askama::Template;
use axum::{
    Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, Redirect},
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

use super::{AppState, Failure, Locale, page};
use crate::auth::{self, Viewer};

#[derive(Deserialize)]
pub(super) struct AccountQuery {
    saved: Option<bool>,
}

#[derive(sqlx::FromRow)]
struct Bookmark {
    id: i64,
    title: String,
    language: String,
    author: String,
}

#[derive(Template)]
#[template(path = "account.html")]
struct Account {
    en: bool,
    viewer: Option<Viewer>,
    oauth_ready: bool,
    saved: bool,
    bookmarks: Vec<Bookmark>,
}

pub(super) async fn show(
    State(state): State<AppState>,
    locale: Locale,
    jar: CookieJar,
    Query(query): Query<AccountQuery>,
) -> Result<Html<String>, Failure> {
    let viewer = auth::viewer(&state, &jar).await?;
    let bookmarks = match &viewer {
        Some(user) => {
            sqlx::query_as(
                "SELECT a.id, a.title, a.language, m.login AS author
                 FROM bookmarks b
                 JOIN articles a ON a.id = b.article_id
                 JOIN maintainers m ON m.github_id = a.author_id
                 WHERE b.github_id = ? AND a.published_at IS NOT NULL
                 ORDER BY b.saved_at DESC, a.id DESC",
            )
            .bind(user.github_id)
            .fetch_all(&state.db)
            .await?
        }
        None => Vec::new(),
    };
    page(
        locale,
        locale.tr("Личный кабинет", "My account"),
        Account {
            en: locale.0,
            viewer,
            bookmarks,
            oauth_ready: state.oauth.is_some(),
            saved: query.saved.unwrap_or(false),
        },
    )
}

#[derive(Deserialize)]
pub(super) struct ProfileForm {
    csrf: String,
    display_name: String,
}

pub(super) async fn profile(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(input): Form<ProfileForm>,
) -> Result<Redirect, Failure> {
    let viewer = signed_in(&state, &jar, &input.csrf).await?;
    let name = input.display_name.trim();
    if name.chars().count() > 60 || name.chars().any(char::is_control) {
        return Err(Failure::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Некорректное имя",
            "Invalid display name",
            "Введите до 60 символов без переносов строк.",
            "Enter up to 60 characters without line breaks.",
        ));
    }
    sqlx::query("UPDATE accounts SET display_name = ? WHERE github_id = ?")
        .bind(name)
        .bind(viewer.github_id)
        .execute(&state.db)
        .await?;
    Ok(Redirect::to("/account?saved=true#account-profile"))
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum BookmarkIntent {
    Save,
    Remove,
}

#[derive(Deserialize)]
pub(super) struct BookmarkForm {
    csrf: String,
    intent: BookmarkIntent,
    #[serde(default)]
    from_article: bool,
}

pub(super) async fn bookmark(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<i64>,
    Form(input): Form<BookmarkForm>,
) -> Result<Redirect, Failure> {
    let viewer = signed_in(&state, &jar, &input.csrf).await?;
    match input.intent {
        BookmarkIntent::Save => {
            sqlx::query_scalar::<_, i64>(
                "INSERT INTO bookmarks (github_id, article_id)
                 SELECT ?, id FROM articles WHERE id = ? AND published_at IS NOT NULL
                 ON CONFLICT(github_id, article_id) DO UPDATE SET article_id = excluded.article_id
                 RETURNING article_id",
            )
            .bind(viewer.github_id)
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(Failure::missing)?;
        }
        BookmarkIntent::Remove => {
            sqlx::query("DELETE FROM bookmarks WHERE github_id = ? AND article_id = ?")
                .bind(viewer.github_id)
                .bind(id)
                .execute(&state.db)
                .await?;
        }
    }
    Ok(if input.from_article {
        Redirect::to(&format!("/articles/{id}#article-bookmark"))
    } else {
        Redirect::to("/account#account-bookmarks")
    })
}

pub(super) async fn is_bookmarked(
    state: &AppState,
    user_id: i64,
    article_id: i64,
) -> Result<bool, Failure> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM bookmarks WHERE github_id = ? AND article_id = ?)",
    )
    .bind(user_id)
    .bind(article_id)
    .fetch_one(&state.db)
    .await?)
}

async fn signed_in(state: &AppState, jar: &CookieJar, csrf: &str) -> Result<Viewer, Failure> {
    let viewer = auth::viewer(state, jar)
        .await?
        .ok_or_else(Failure::forbidden)?;
    viewer.check_csrf(csrf)?;
    Ok(viewer)
}
