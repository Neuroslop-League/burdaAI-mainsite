use std::{collections::HashMap, sync::Arc};

use askama::Template;
use axum::{
    Extension, Form, Router,
    extract::{DefaultBodyLimit, FromRequestParts, Path, Query, Request, State},
    http::{HeaderValue, StatusCode, header, request::Parts},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use sqlx::SqlitePool;
use tower_http::services::ServeDir;

use crate::{
    auth::{self, OAuth, Viewer},
    db::{self, Article, Maintainer},
    github::{Activity, GitHub, Snapshot, Tree},
    project_map::{ComponentLink, Map, Selection, component_links},
};

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub github: GitHub,
    pub oauth: Option<Arc<OAuth>>,
}

impl AppState {
    pub fn new(db: SqlitePool, github: GitHub, oauth: Option<OAuth>) -> Self {
        Self {
            db,
            github,
            oauth: oauth.map(Arc::new),
        }
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/articles", get(article_list))
        .route("/articles/{id}", get(article))
        .route("/maintainers", get(maintainer_list))
        .route("/maintainers/{id}", get(profile))
        .route("/project", get(|| async { Redirect::to("/project/map") }))
        .route("/project/map", get(project_map))
        .route(
            "/project/map/tokenize",
            post(crate::prompt_trace::tokenize).layer(DefaultBodyLimit::max(6 * 1024)),
        )
        .route("/project/tree", get(project_tree))
        .route("/studio", get(studio))
        .route("/studio/new", get(new_article))
        .route("/studio/articles/{id}", get(edit_article))
        .route("/studio/preview", post(preview_article))
        .route("/studio/save", post(save_article))
        .route("/studio/maintainers", get(manage).post(add_maintainer))
        .route("/studio/maintainers/{id}/remove", post(remove_maintainer))
        .route("/auth/github", get(auth::start))
        .route("/auth/github/callback", get(auth::callback))
        .route("/auth/logout", post(auth::logout))
        .fallback(|| async { Failure::missing() })
        .layer(DefaultBodyLimit::max(700 * 1024))
        .layer(middleware::from_fn(page_middleware))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state)
}

#[derive(Clone, Copy, Default)]
pub struct Locale(bool);

impl Locale {
    fn tr(self, ru: &'static str, en: &'static str) -> &'static str {
        if self.0 { en } else { ru }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Locale {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(parts.extensions.get::<Self>().copied().unwrap_or_default())
    }
}

async fn page_middleware(mut request: Request, next: Next) -> Response {
    let lang = request.uri().query().and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes()).find_map(|(key, value)| {
            if key != "lang" {
                return None;
            }
            match value.as_ref() {
                "ru" => Some(Locale(false)),
                "en" => Some(Locale(true)),
                _ => None,
            }
        })
    });
    let cookies = CookieJar::from_headers(request.headers());
    let locale = lang.unwrap_or_else(|| {
        Locale(
            cookies
                .get("burda_lang")
                .is_some_and(|cookie| cookie.value() == "en"),
        )
    });
    let private =
        request.uri().path().starts_with("/studio") || request.uri().path().starts_with("/auth");
    request.extensions_mut().insert(locale);
    let mut response = next.run(request).await;
    if let Some(failure) = response.extensions_mut().remove::<Failure>() {
        let title = locale.tr(failure.ru, failure.en);
        response = match page(
            locale,
            title,
            ErrorPage {
                en: locale.0,
                title,
                detail: locale.tr(failure.detail_ru, failure.detail_en),
            },
        ) {
            Ok(html) => (failure.code, html).into_response(),
            Err(_) => (failure.code, title).into_response(),
        };
    } else if matches!(
        response.status(),
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY | StatusCode::PAYLOAD_TOO_LARGE
    ) && !response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|value| value.as_bytes().starts_with(b"text/html"))
    {
        let code = response.status();
        let title = locale.tr("Некорректный запрос", "Invalid request");
        let detail = locale.tr(
            "Проверьте поля формы и размер текста.",
            "Check the form fields and text length.",
        );
        if let Ok(html) = page(
            locale,
            title,
            ErrorPage {
                en: locale.0,
                title,
                detail,
            },
        ) {
            response = (code, html).into_response();
        }
    }
    let headers = response.headers_mut();
    headers.insert(header::VARY, HeaderValue::from_static("Cookie"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(
        "default-src 'self'; img-src 'self' https://avatars.githubusercontent.com; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"
    ));
    if private {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    if let Some(lang) = lang {
        headers.append(
            header::SET_COOKIE,
            HeaderValue::from_static(lang.tr(
                "burda_lang=ru; Path=/; Max-Age=31536000; SameSite=Lax",
                "burda_lang=en; Path=/; Max-Age=31536000; SameSite=Lax",
            )),
        );
    }
    response
}

#[derive(Clone)]
pub struct Failure {
    code: StatusCode,
    ru: &'static str,
    en: &'static str,
    detail_ru: &'static str,
    detail_en: &'static str,
}

impl Failure {
    const fn new(
        code: StatusCode,
        ru: &'static str,
        en: &'static str,
        detail_ru: &'static str,
        detail_en: &'static str,
    ) -> Self {
        Self {
            code,
            ru,
            en,
            detail_ru,
            detail_en,
        }
    }

    fn missing() -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "Страница не найдена",
            "Page not found",
            "Проверьте адрес или вернитесь на главную.",
            "Check the address or go back home.",
        )
    }

    pub fn forbidden() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "Доступ закрыт",
            "Access denied",
            "Войдите в разрешённый аккаунт GitHub и попробуйте снова.",
            "Sign in with an authorized GitHub account and try again.",
        )
    }

    pub fn oauth_unavailable() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Вход не настроен",
            "Sign-in is unavailable",
            "Добавьте локальные настройки GitHub OAuth.",
            "Configure the local GitHub OAuth settings.",
        )
    }

    pub fn invalid_login() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "Вход не завершён",
            "Sign-in incomplete",
            "Откройте редактор и запустите вход заново.",
            "Open the studio and start sign-in again.",
        )
    }

    pub fn github() -> Self {
        Self::new(
            StatusCode::BAD_GATEWAY,
            "GitHub недоступен",
            "GitHub unavailable",
            "Повторите попытку позже.",
            "Try again later.",
        )
    }

    fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Ошибка сервера",
            "Server error",
            "Не удалось выполнить запрос.",
            "The request could not be completed.",
        )
    }

    fn conflict() -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "Статья уже изменена",
            "Article has changed",
            "Откройте актуальную версию в редакторе перед сохранением.",
            "Open the latest version in the studio before saving.",
        )
    }

    fn invalid_user() -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "GitHub-аккаунт не найден",
            "GitHub account not found",
            "Проверьте логин и попробуйте снова.",
            "Check the username and try again.",
        )
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (self.code, Extension(self)).into_response()
    }
}

impl From<sqlx::Error> for Failure {
    fn from(error: sqlx::Error) -> Self {
        eprintln!("Database query failed: {error}");
        Self::internal()
    }
}

#[derive(Template)]
#[template(path = "layout.html")]
struct Layout<'a> {
    lang: &'static str,
    en: bool,
    title: &'a str,
    body: String,
}

fn page(locale: Locale, title: &str, content: impl Template) -> Result<Html<String>, Failure> {
    let body = content.render().map_err(|_| Failure::internal())?;
    let layout = Layout {
        lang: locale.tr("ru", "en"),
        en: locale.0,
        body,
        title,
    };
    Ok(Html(layout.render().map_err(|_| Failure::internal())?))
}

#[derive(Template)]
#[template(path = "error.html")]
struct ErrorPage<'a> {
    en: bool,
    title: &'a str,
    detail: &'a str,
}

#[derive(Template)]
#[template(path = "home.html")]
struct Home {
    en: bool,
}

async fn home(locale: Locale) -> Result<Html<String>, Failure> {
    page(locale, "Neuroslop League", Home { en: locale.0 })
}

#[derive(Template)]
#[template(path = "articles.html")]
struct Articles {
    en: bool,
    articles: Vec<Article>,
}

async fn article_list(
    State(state): State<AppState>,
    locale: Locale,
) -> Result<Html<String>, Failure> {
    page(
        locale,
        locale.tr("Статьи", "Articles"),
        Articles {
            en: locale.0,
            articles: db::articles(&state.db, None, false).await?,
        },
    )
}

#[derive(Template)]
#[template(path = "article.html")]
struct ArticlePage<'a> {
    article: &'a Article,
    rendered: String,
}

async fn article(
    State(state): State<AppState>,
    locale: Locale,
    Path(id): Path<i64>,
) -> Result<Html<String>, Failure> {
    let article = db::article(&state.db, id)
        .await?
        .filter(|item| item.published)
        .ok_or_else(Failure::missing)?;
    page(
        locale,
        &article.title,
        ArticlePage {
            rendered: markdown(&article.body),
            article: &article,
        },
    )
}

const PROJECT_FOUNDER_ID: i64 = 132206435;
const FOUNDER_WORK: &str =
    "https://github.com/Neuroslop-League/burdaAI/commit/95924200951f29291487f6cd977800fb21a169f4";

#[derive(Template)]
#[template(path = "maintainers.html")]
struct Maintainers {
    en: bool,
    cards: Vec<MaintainerCard>,
}

struct MaintainerCard {
    member: Maintainer,
    zones: Vec<ComponentLink>,
    highlight: Option<MaintainerHighlight>,
}

struct MaintainerHighlight {
    label: &'static str,
    title: String,
    url: String,
    language: Option<String>,
    external: bool,
}

async fn maintainer_list(
    State(state): State<AppState>,
    locale: Locale,
) -> Result<Html<String>, Failure> {
    let (members, articles) = tokio::try_join!(
        db::maintainers(&state.db),
        db::maintainer_articles(&state.db)
    )?;
    let mut articles: HashMap<_, _> = articles
        .into_iter()
        .map(|article| (article.author_id, article))
        .collect();
    let activity = if members
        .iter()
        .any(|member| !articles.contains_key(&member.github_id))
    {
        state.github.activity().await.value
    } else {
        None
    };
    let cards = members
        .into_iter()
        .map(|member| {
            let zones = match member.github_id {
                PROJECT_FOUNDER_ID => component_links(&["runtime", "tokenizer", "model"], locale.0),
                _ => Vec::new(),
            };
            let highlight = articles
                .remove(&member.github_id)
                .map(|article| MaintainerHighlight {
                    label: locale.tr("Последняя статья", "Latest article"),
                    title: article.title,
                    url: format!("/articles/{}", article.id),
                    language: Some(article.language),
                    external: false,
                })
                .or_else(|| {
                    let event = activity.as_ref()?.iter().find(|event| {
                        event.author_id == member.github_id && !event.title.is_empty()
                    })?;
                    Some(MaintainerHighlight {
                        label: locale.tr("Недавний вклад", "Recent contribution"),
                        title: event.title.clone(),
                        url: event.url.clone(),
                        language: None,
                        external: true,
                    })
                })
                .or_else(|| {
                    (member.github_id == PROJECT_FOUNDER_ID).then(|| MaintainerHighlight {
                        label: locale.tr("Из проекта", "From the project"),
                        title: locale
                            .tr(
                                "Гибридный запуск burdaAI с Q4 GGUF",
                                "Hybrid burdaAI inference with Q4 GGUF",
                            )
                            .into(),
                        url: FOUNDER_WORK.into(),
                        language: None,
                        external: true,
                    })
                });
            MaintainerCard {
                member,
                zones,
                highlight,
            }
        })
        .collect();
    page(
        locale,
        locale.tr("Мейнтейнеры", "Maintainers"),
        Maintainers {
            en: locale.0,
            cards,
        },
    )
}

#[derive(Template)]
#[template(path = "profile.html")]
struct Profile<'a> {
    en: bool,
    member: &'a Maintainer,
    articles: Vec<Article>,
    activity: Snapshot<Vec<Activity>>,
    has_events: bool,
}

async fn profile(
    State(state): State<AppState>,
    locale: Locale,
    Path(id): Path<i64>,
) -> Result<Html<String>, Failure> {
    let member = db::maintainer(&state.db, id)
        .await?
        .ok_or_else(Failure::missing)?;
    let (articles, activity) = tokio::join!(
        db::articles(&state.db, Some(id), false),
        state.github.activity()
    );
    let has_events = activity
        .value
        .as_ref()
        .is_some_and(|events| events.iter().any(|event| event.author_id == id));
    page(
        locale,
        &member.login,
        Profile {
            en: locale.0,
            member: &member,
            articles: articles?,
            activity,
            has_events,
        },
    )
}

#[derive(Template)]
#[template(path = "project.html")]
struct Project {
    en: bool,
    snapshot: Snapshot<Tree>,
}

async fn project_tree(
    State(state): State<AppState>,
    locale: Locale,
) -> Result<Html<String>, Failure> {
    page(
        locale,
        locale.tr("Дерево PR", "PR tree"),
        Project {
            en: locale.0,
            snapshot: state.github.tree().await,
        },
    )
}

async fn project_map(
    locale: Locale,
    Query(selection): Query<Selection>,
) -> Result<Html<String>, Failure> {
    page(
        locale,
        locale.tr("Карта компонентов", "Component map"),
        Map::new(locale.0, &selection),
    )
}

#[derive(Template)]
#[template(path = "studio.html")]
struct Studio {
    en: bool,
    viewer: Option<Viewer>,
    oauth_ready: bool,
    articles: Vec<Article>,
}

async fn studio(
    State(state): State<AppState>,
    locale: Locale,
    jar: CookieJar,
) -> Result<Html<String>, Failure> {
    let viewer = auth::viewer(&state, &jar).await?;
    let articles = match &viewer {
        Some(viewer) => db::articles(&state.db, Some(viewer.github_id), true).await?,
        None => Vec::new(),
    };
    page(
        locale,
        locale.tr("Редактор", "Studio"),
        Studio {
            en: locale.0,
            viewer,
            articles,
            oauth_ready: state.oauth.is_some(),
        },
    )
}

#[derive(Deserialize)]
struct Draft {
    id: i64,
    revision: i64,
    title: String,
    body: String,
    language: String,
    csrf: String,
    #[serde(default)]
    intent: String,
}

#[derive(Template)]
#[template(path = "editor.html")]
struct Editor<'a> {
    en: bool,
    draft: Draft,
    csrf: &'a str,
    published: bool,
    preview: Option<String>,
    notice: &'static str,
}

fn editor(
    locale: Locale,
    viewer: &Viewer,
    draft: Draft,
    published: bool,
    preview: Option<String>,
    notice: &'static str,
) -> Result<Html<String>, Failure> {
    let title = if draft.id == 0 {
        locale.tr("Новая статья", "New article")
    } else {
        locale.tr("Редактирование", "Edit article")
    };
    page(
        locale,
        title,
        Editor {
            en: locale.0,
            draft,
            csrf: &viewer.csrf,
            published,
            preview,
            notice,
        },
    )
}

async fn require_viewer(state: &AppState, jar: &CookieJar) -> Result<Viewer, Failure> {
    auth::viewer(state, jar)
        .await?
        .ok_or_else(Failure::forbidden)
}

async fn require_founder(state: &AppState, jar: &CookieJar) -> Result<Viewer, Failure> {
    let viewer = require_viewer(state, jar).await?;
    if !viewer.founder() {
        return Err(Failure::forbidden());
    }
    Ok(viewer)
}

async fn owned(state: &AppState, viewer: &Viewer, id: i64) -> Result<Article, Failure> {
    db::article(&state.db, id)
        .await?
        .filter(|a| a.author_id == viewer.github_id)
        .ok_or_else(Failure::missing)
}

async fn new_article(
    State(state): State<AppState>,
    locale: Locale,
    jar: CookieJar,
) -> Result<Html<String>, Failure> {
    let viewer = require_viewer(&state, &jar).await?;
    let draft = Draft {
        id: 0,
        revision: 0,
        title: String::new(),
        body: String::new(),
        language: locale.tr("ru", "en").into(),
        csrf: String::new(),
        intent: String::new(),
    };
    editor(locale, &viewer, draft, false, None, "")
}

async fn edit_article(
    State(state): State<AppState>,
    locale: Locale,
    jar: CookieJar,
    Path(id): Path<i64>,
) -> Result<Html<String>, Failure> {
    let viewer = require_viewer(&state, &jar).await?;
    let article = owned(&state, &viewer, id).await?;
    let draft = Draft {
        id,
        revision: article.revision,
        title: article.title,
        body: article.body,
        language: article.language,
        csrf: String::new(),
        intent: String::new(),
    };
    editor(locale, &viewer, draft, article.published, None, "")
}

fn markdown(source: &str) -> String {
    let options =
        pulldown_cmark::Options::ENABLE_TABLES | pulldown_cmark::Options::ENABLE_STRIKETHROUGH;
    let parser = pulldown_cmark::Parser::new_ext(source, options).filter(|event| {
        !matches!(
            event,
            pulldown_cmark::Event::Html(_) | pulldown_cmark::Event::InlineHtml(_)
        )
    });
    let mut output = String::new();
    pulldown_cmark::html::push_html(&mut output, parser);
    ammonia::Builder::default().clean(&output).to_string()
}

fn validate(locale: Locale, draft: &Draft, title_required: bool) -> &'static str {
    if (title_required && draft.title.trim().is_empty()) || draft.title.trim().chars().count() > 160
    {
        locale.tr(
            "Введите заголовок до 160 символов.",
            "Enter a title of up to 160 characters.",
        )
    } else if draft.body.chars().count() > 50_000 {
        locale.tr(
            "Текст статьи должен быть не длиннее 50 000 символов.",
            "The article must be no longer than 50,000 characters.",
        )
    } else if !matches!(draft.language.as_str(), "ru" | "en") {
        locale.tr("Выберите язык статьи.", "Select the article language.")
    } else {
        ""
    }
}

async fn preview_article(
    State(state): State<AppState>,
    locale: Locale,
    jar: CookieJar,
    Form(draft): Form<Draft>,
) -> Result<Response, Failure> {
    let viewer = require_viewer(&state, &jar).await?;
    viewer.check_csrf(&draft.csrf)?;
    let published = if draft.id == 0 {
        false
    } else {
        owned(&state, &viewer, draft.id).await?.published
    };
    let notice = validate(locale, &draft, false);
    let preview = notice.is_empty().then(|| markdown(&draft.body));
    let code = if notice.is_empty() {
        StatusCode::OK
    } else {
        StatusCode::UNPROCESSABLE_ENTITY
    };
    Ok((
        code,
        editor(locale, &viewer, draft, published, preview, notice)?,
    )
        .into_response())
}

async fn save_article(
    State(state): State<AppState>,
    locale: Locale,
    jar: CookieJar,
    Form(draft): Form<Draft>,
) -> Result<Response, Failure> {
    let viewer = require_viewer(&state, &jar).await?;
    viewer.check_csrf(&draft.csrf)?;
    let published = if draft.id == 0 {
        false
    } else {
        owned(&state, &viewer, draft.id).await?.published
    };
    let unpublish = draft.intent == "unpublish";
    let publish = draft.intent == "publish";
    let mut notice = if unpublish {
        ""
    } else {
        validate(locale, &draft, true)
    };
    if !matches!(draft.intent.as_str(), "" | "save" | "publish" | "unpublish")
        || (unpublish && !published)
    {
        notice = locale.tr("Действие недоступно.", "This action is unavailable.");
    } else if publish && draft.body.trim().is_empty() {
        notice = locale.tr(
            "Напишите текст перед публикацией.",
            "Write the article before publishing.",
        );
    }
    if !notice.is_empty() {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            editor(locale, &viewer, draft, published, None, notice)?,
        )
            .into_response());
    }

    let id: Option<i64> = if draft.id == 0 {
        sqlx::query_scalar(
            "INSERT INTO articles (author_id, title, body, language, published_at)
             SELECT github_id, ?, ?, ?, CASE WHEN ? THEN strftime('%Y-%m-%d %H:%M', 'now') END
             FROM maintainers WHERE github_id = ? AND role != 'inactive' RETURNING id",
        )
        .bind(draft.title.trim())
        .bind(&draft.body)
        .bind(&draft.language)
        .bind(publish)
        .bind(viewer.github_id)
        .fetch_optional(&state.db)
        .await?
    } else if unpublish {
        sqlx::query_scalar(
            "UPDATE articles SET published_at = NULL, revision = revision + 1,
                updated_at = strftime('%Y-%m-%d %H:%M', 'now')
             WHERE id = ? AND revision = ? AND author_id = ?
               AND EXISTS (SELECT 1 FROM maintainers WHERE github_id = author_id AND role != 'inactive')
             RETURNING id",
        )
        .bind(draft.id).bind(draft.revision).bind(viewer.github_id).fetch_optional(&state.db).await?
    } else {
        sqlx::query_scalar(
            "UPDATE articles SET title = ?, body = ?, language = ?, revision = revision + 1,
                updated_at = strftime('%Y-%m-%d %H:%M', 'now'),
                published_at = CASE WHEN ? AND published_at IS NULL THEN strftime('%Y-%m-%d %H:%M', 'now') ELSE published_at END
             WHERE id = ? AND revision = ? AND author_id = ?
               AND EXISTS (SELECT 1 FROM maintainers WHERE github_id = author_id AND role != 'inactive')
             RETURNING id",
        )
        .bind(draft.title.trim()).bind(&draft.body).bind(&draft.language).bind(publish)
        .bind(draft.id).bind(draft.revision).bind(viewer.github_id).fetch_optional(&state.db).await?
    };
    let Some(id) = id else {
        if auth::viewer(&state, &jar).await?.is_none() {
            return Err(Failure::forbidden());
        }
        return Err(Failure::conflict());
    };
    Ok(Redirect::to(&format!("/studio/articles/{id}")).into_response())
}

#[derive(Template)]
#[template(path = "manage.html")]
struct Manage<'a> {
    en: bool,
    members: Vec<Maintainer>,
    csrf: &'a str,
}

async fn manage(
    State(state): State<AppState>,
    locale: Locale,
    jar: CookieJar,
) -> Result<Html<String>, Failure> {
    let viewer = require_founder(&state, &jar).await?;
    page(
        locale,
        locale.tr("Управление мейнтейнерами", "Manage maintainers"),
        Manage {
            en: locale.0,
            members: db::maintainers(&state.db).await?,
            csrf: &viewer.csrf,
        },
    )
}

#[derive(Deserialize)]
struct AddMember {
    csrf: String,
    login: String,
}

async fn add_maintainer(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(input): Form<AddMember>,
) -> Result<Redirect, Failure> {
    let viewer = require_founder(&state, &jar).await?;
    viewer.check_csrf(&input.csrf)?;
    let login = input.login.trim();
    if login.is_empty()
        || login.len() > 39
        || !login
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(Failure::invalid_user());
    }
    let person = state
        .github
        .identity(login)
        .await
        .map_err(|_| Failure::github())?
        .ok_or_else(Failure::invalid_user)?;
    sqlx::query(
        "INSERT INTO maintainers VALUES (?, ?, ?, 'maintainer')
         ON CONFLICT(github_id) DO UPDATE SET login = excluded.login, avatar_url = excluded.avatar_url,
         role = CASE WHEN maintainers.role = 'founder' THEN 'founder' ELSE 'maintainer' END",
    ).bind(person.id).bind(&person.login).bind(person.avatar()).execute(&state.db).await?;
    Ok(Redirect::to("/studio/maintainers"))
}

#[derive(Deserialize)]
struct RemoveMember {
    csrf: String,
}

async fn remove_maintainer(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    jar: CookieJar,
    Form(input): Form<RemoveMember>,
) -> Result<Redirect, Failure> {
    let viewer = require_founder(&state, &jar).await?;
    viewer.check_csrf(&input.csrf)?;
    if id == viewer.github_id {
        return Err(Failure::forbidden());
    }
    let mut transaction = state.db.begin().await?;
    let changed = sqlx::query(
        "UPDATE maintainers SET role = 'inactive' WHERE github_id = ? AND role = 'maintainer'",
    )
    .bind(id)
    .execute(&mut *transaction)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(Failure::missing());
    }
    sqlx::query("DELETE FROM sessions WHERE github_id = ?")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(Redirect::to("/studio/maintainers"))
}
