use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use axum::{
    Form, Json, Router,
    body::{Body, to_bytes},
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, Request, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use url::Url;

use crate::{auth::OAuth, db, github::GitHub, web};

const FOUNDER: &str = "132206435";
const WRITER: &str = "5678";

struct Fixture {
    app: Router,
    state: web::AppState,
    db_path: std::path::PathBuf,
    _directory: tempfile::TempDir,
    fail_api: Arc<AtomicBool>,
}

impl Fixture {
    async fn new() -> Self {
        let fail_api = Arc::new(AtomicBool::new(false));
        let fake = Router::new()
            .route("/login/token", post(mock_token))
            .route("/user", get(mock_user))
            .route(
                "/users/Writer",
                get(|| async { Json(json!({"login": "Writer", "id": 5678})) }),
            )
            .route(
                "/repos/Neuroslop-League/{repo}",
                get(|| async { Json(json!({"default_branch": "main"})) }),
            )
            .route("/repos/Neuroslop-League/{repo}/commits", get(mock_commits))
            .route("/repos/Neuroslop-League/{repo}/pulls", get(mock_pulls))
            .with_state(fail_api.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base: Url = format!("http://{}", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        tokio::spawn(async move { axum::serve(listener, fake).await.unwrap() });

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = db::open(db_path.clone()).await.unwrap();
        let mut github = GitHub::new(None).unwrap();
        github.base = base.clone();
        let mut oauth = OAuth::new(
            "example-client".into(),
            "example-secret".into(),
            "http://127.0.0.1:3000/auth/github/callback"
                .parse()
                .unwrap(),
        )
        .unwrap();
        oauth.authorize_url = base.join("login/authorize").unwrap();
        oauth.token_url = base.join("login/token").unwrap();
        oauth.user_url = base.join("user").unwrap();
        let state = web::AppState::new(db, github, Some(oauth));
        Self {
            app: web::app(state.clone()),
            state,
            db_path,
            _directory: dir,
            fail_api,
        }
    }

    async fn request(&self, path: &str, cookie: &str, form: Option<&[(&str, &str)]>) -> Response {
        let mut request = Request::builder().uri(path).header(header::COOKIE, cookie);
        let body = if let Some(form) = form {
            request = request
                .method("POST")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
            Body::from(
                url::form_urlencoded::Serializer::new(String::new())
                    .extend_pairs(form)
                    .finish(),
            )
        } else {
            Body::empty()
        };
        self.app
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap()
    }

    async fn get(&self, path: &str, status: StatusCode) -> String {
        let response = self.request(path, "", None).await;
        assert_eq!(response.status(), status);
        rendered(response).await
    }

    async fn sign_in(&self, handled_as: &str, status: StatusCode) -> String {
        self.sign_in_from(handled_as, status, false).await
    }

    async fn sign_in_from(&self, handled_as: &str, status: StatusCode, account: bool) -> String {
        let path = if account {
            "/auth/github?next=account"
        } else {
            "/auth/github"
        };
        let start = self.request(path, "", None).await;
        assert_eq!(start.status(), StatusCode::SEE_OTHER);
        let flow = received_cookie(&start, "burda_oauth");
        let redirect = Url::parse(start.headers()[header::LOCATION].to_str().unwrap()).unwrap();
        let parameters: std::collections::HashMap<_, _> =
            redirect.query_pairs().into_owned().collect();
        assert_eq!(parameters["code_challenge_method"], "S256");
        let bad_callback = self
            .request(
                "/auth/github/callback?code=test&state=incorrect",
                &flow,
                None,
            )
            .await;
        assert_eq!(bad_callback.status(), StatusCode::BAD_REQUEST);
        let callback = format!(
            "/auth/github/callback?code={handled_as}.{}&state={}",
            parameters["code_challenge"], parameters["state"]
        );
        let response = self.request(&callback, &flow, None).await;
        assert_eq!(response.status(), status);
        let replay = self.request(&callback, &flow, None).await;
        assert_eq!(replay.status(), StatusCode::BAD_REQUEST);
        if status == StatusCode::SEE_OTHER {
            assert_eq!(
                response.headers()[header::LOCATION],
                if account { "/account" } else { "/studio" },
            );
            received_cookie(&response, "burda_session")
        } else {
            String::new()
        }
    }

    async fn csrf(&self, session: &str) -> String {
        let html = rendered(self.request("/account", session, None).await).await;
        html.split_once("name=\"csrf\" value=\"")
            .unwrap()
            .1
            .split('"')
            .next()
            .unwrap()
            .into()
    }
}

fn received_cookie(response: &Response, name: &str) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|value| {
            let cookie = value.to_str().ok()?.split(';').next()?;
            cookie
                .starts_with(&format!("{name}="))
                .then(|| cookie.to_owned())
        })
        .unwrap()
}

async fn rendered(response: Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), 4 * 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

async fn mock_token(Form(values): Form<std::collections::HashMap<String, String>>) -> Response {
    let (id, expected) = values["code"].split_once('.').unwrap();
    let actual = URL_SAFE_NO_PAD.encode(Sha256::digest(values["code_verifier"].as_bytes()));
    assert_eq!(actual, expected);
    Json(json!({"access_token": id})).into_response()
}

async fn mock_user(headers: HeaderMap) -> Json<serde_json::Value> {
    let bearer = headers[header::AUTHORIZATION].to_str().unwrap();
    let (id, login) = match bearer {
        "Bearer 132206435" => (132206435, "RUdolf1517"),
        "Bearer 5678" => (5678, "Writer"),
        _ => (999, "Outsider"),
    };
    Json(json!({"id": id, "login": login}))
}

async fn mock_commits(
    Path(repo): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Response {
    assert_eq!(query["sha"], "main");
    let page = &query["page"];
    let entry = json!({
        "sha": format!("fake-{page}"),
        "author": {"id": 132206435},
        "commit": {
            "message": format!("Commit {page} from {repo}\nMessage body"),
            "committer": {"date": chrono::Utc::now().to_rfc3339()}
        }
    });
    let mut result = Json(json!([entry])).into_response();
    if page == "1" {
        result.headers_mut().insert(
            header::LINK,
            "<https://api.github.com/example?page=2>; rel=\"next\""
                .parse()
                .unwrap(),
        );
    }
    result
}

async fn mock_pulls(
    State(unavailable): State<Arc<AtomicBool>>,
    Path(repo): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Response {
    if unavailable.load(Ordering::Relaxed) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    assert_eq!(query["state"], "all");

    let date = chrono::Utc::now().to_rfc3339();
    let merged = json!({
        "number": 25, "title": format!("PR from {repo}"), "state": "closed",
        "created_at": date, "merged_at": date,
        "draft": false, "user": {"id": 132206435}, "head": {"ref": "codex/runtime"}
    });
    if let Some(base) = query.get("base") {
        assert_eq!(base, "main");
        assert_eq!(repo, "burdaAI");
        if query["page"] == "2" {
            let open = json!({
                "number": 24, "title": "Update > architecture", "state": "open",
                "created_at": date, "draft": false, "user": null, "head": {"ref": "feature/architecture"}
            });
            return Json(json!([open])).into_response();
        }
        let mut response = Json(json!([merged])).into_response();
        response.headers_mut().insert(
            header::LINK,
            HeaderValue::from_static("<https://api.github.com/example?page=2>; rel=\"next\""),
        );
        return response;
    }
    Json(json!([merged])).into_response()
}

fn article_form<'a>(
    csrf: &'a str,
    (id, revision): (&'a str, &'a str),
    title: &'a str,
    body: &'a str,
    intent: &'a str,
) -> [(&'static str, &'a str); 7] {
    [
        ("csrf", csrf),
        ("id", id),
        ("revision", revision),
        ("title", title),
        ("body", body),
        ("language", "en"),
        ("intent", intent),
    ]
}

#[tokio::test]
async fn editorial_workflow_and_roles() {
    let fixture = Fixture::new().await;
    assert!(
        fixture
            .get("/articles", StatusCode::OK)
            .await
            .contains("Опубликованных статей пока нет.")
    );
    fixture.get("/studio/new", StatusCode::FORBIDDEN).await;
    fixture.sign_in("999", StatusCode::FORBIDDEN).await;
    let founder_cookie = fixture.sign_in(FOUNDER, StatusCode::SEE_OTHER).await;
    let founder_csrf = fixture.csrf(&founder_cookie).await;
    assert_eq!(
        fixture
            .request(
                "/studio/maintainers",
                &founder_cookie,
                Some(&[("csrf", "bad"), ("login", "Writer")])
            )
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fixture
            .request(
                "/studio/maintainers",
                &founder_cookie,
                Some(&[("csrf", &founder_csrf), ("login", "Writer")])
            )
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    assert!(
        fixture
            .get("/maintainers", StatusCode::OK)
            .await
            .contains("Writer")
    );

    let writer_cookie = fixture.sign_in(WRITER, StatusCode::SEE_OTHER).await;
    let writer_csrf = fixture.csrf(&writer_cookie).await;
    assert_eq!(
        fixture
            .request("/studio/maintainers", &writer_cookie, None)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let body = "**Weights**\n\n[bad](javascript:alert%281%29)\n<script>alert(1)</script>";
    let draft = article_form(&writer_csrf, ("0", "0"), "A draft", body, "save");
    let preview = fixture
        .request("/studio/preview", &writer_cookie, Some(&draft))
        .await;
    assert_eq!(preview.status(), StatusCode::OK);
    let preview = rendered(preview).await;
    assert!(preview.contains("<strong>Weights</strong>"));
    assert!(!preview.contains("<script>alert(1)</script>"));
    assert!(!preview.contains("href=\"javascript:"));
    let response = fixture
        .request("/studio/save", &writer_cookie, Some(&draft))
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()[header::LOCATION], "/studio/articles/1");
    fixture.get("/articles/1", StatusCode::NOT_FOUND).await;
    assert_eq!(
        fixture
            .request("/studio/articles/1", &founder_cookie, None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let foreign = article_form(&founder_csrf, ("1", "1"), "Overwrite", body, "publish");
    assert_eq!(
        fixture
            .request("/studio/save", &founder_cookie, Some(&foreign))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    let invalid = article_form(&writer_csrf, ("1", "1"), "Original title", " ", "publish");
    let error = fixture
        .request("/studio/save", &writer_cookie, Some(&invalid))
        .await;
    assert_eq!(error.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let html = rendered(error).await;
    assert!(html.contains("Напишите текст перед публикацией") && html.contains("<textarea"));
    let publish = article_form(
        &writer_csrf,
        ("1", "1"),
        "Published & safe",
        body,
        "publish",
    );
    assert_eq!(
        fixture
            .request("/studio/save", &writer_cookie, Some(&publish))
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let public = fixture.get("/articles/1", StatusCode::OK).await;
    assert!(public.contains("<strong>Weights</strong>"));
    assert!(public.contains("Published &#38; safe") || public.contains("Published &amp; safe"));
    assert_eq!(
        fixture
            .request("/studio/save", &writer_cookie, Some(&publish))
            .await
            .status(),
        StatusCode::CONFLICT
    );

    let hide = article_form(&writer_csrf, ("1", "2"), "", "", "unpublish");
    assert_eq!(
        fixture
            .request("/studio/save", &writer_cookie, Some(&hide))
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    fixture.get("/articles/1", StatusCode::NOT_FOUND).await;
    let publish_again = article_form(&writer_csrf, ("1", "3"), "Updated title", body, "publish");
    assert_eq!(
        fixture
            .request("/studio/save", &writer_cookie, Some(&publish_again))
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    let edit_live = article_form(
        &writer_csrf,
        ("1", "4"),
        "Updated title",
        "**Updated**",
        "save",
    );
    assert_eq!(
        fixture
            .request("/studio/save", &writer_cookie, Some(&edit_live))
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    assert!(
        fixture
            .get("/articles/1", StatusCode::OK)
            .await
            .contains("<strong>Updated</strong>")
    );

    for (id, status) in [
        (FOUNDER, StatusCode::FORBIDDEN),
        (WRITER, StatusCode::SEE_OTHER),
    ] {
        let response = fixture
            .request(
                &format!("/studio/maintainers/{id}/remove"),
                &founder_cookie,
                Some(&[("csrf", &founder_csrf)]),
            )
            .await;
        assert_eq!(response.status(), status);
    }
    assert_eq!(
        fixture
            .request("/studio/new", &writer_cookie, None)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let logout = fixture
        .request(
            "/auth/logout",
            &founder_cookie,
            Some(&[("csrf", &founder_csrf)]),
        )
        .await;
    assert_eq!(logout.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        fixture
            .request("/studio/new", &founder_cookie, None)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );

    fixture.state.db.close().await;
    let reopened = db::open(fixture.db_path.clone()).await.unwrap();
    let article = db::article(&reopened, 1).await.unwrap().unwrap();
    assert!(article.published && article.author == "Writer");
    assert!(
        !db::maintainer(&reopened, 5678)
            .await
            .unwrap()
            .unwrap()
            .active()
    );
    assert_eq!(db::maintainers(&reopened).await.unwrap().len(), 1);
}

#[tokio::test]
async fn public_pages_and_github_integration() {
    let fixture = Fixture::new().await;
    let ru = fixture.get("/", StatusCode::OK).await;
    let en = fixture.get("/?lang=en", StatusCode::OK).await;
    assert!(ru.contains("Я бурда и ты бурда, получается хуйня."));
    assert!(en.contains("burda together, burmalda forever."));
    assert!(!ru.contains("Путь запроса") && !en.contains("How a prompt flows"));
    assert!(!ru.contains("data-filter="));
    for node in ["model", "training", "runtime"] {
        assert!(en.contains(&format!("/project/map?node={node}&amp;lang=en")));
    }
    let map = fixture
        .get("/project/map?lang=en&node=runtime", StatusCode::OK)
        .await;
    assert!(map.contains("Rust runtime"));
    assert!(map.contains("data-map-active=\"runtime\""));
    assert!(!map.contains("github.com/Neuroslop-League/burdaAI/pull/"));
    assert!(map.contains("/docs/inference-bundle.md"));
    assert!(
        fixture
            .get("/static/site.js", StatusCode::OK)
            .await
            .contains("paintArtwork")
    );

    let tree = fixture.get("/project/tree?lang=en", StatusCode::OK).await;
    assert!(tree.contains("https://github.com/Neuroslop-League/burdaAI/tree/main"));
    assert!(tree.contains("https://github.com/Neuroslop-League/burdaAI/pull/25"));
    assert!(tree.contains("https://github.com/Neuroslop-League/burdaAI/pull/24"));
    assert!(tree.contains("codex/runtime") && tree.contains("feature/architecture"));
    assert!(tree.contains("data-state=\"merged\"") && tree.contains("data-state=\"open\""));
    let profile = fixture
        .get(&format!("/maintainers/{FOUNDER}?lang=en"), StatusCode::OK)
        .await;
    for repo in ["burdaAI", "burdaAI-mainsite"] {
        assert!(profile.contains(&format!("Commit 2 from {repo}")));
        assert!(profile.contains(&format!("PR from {repo}")));
    }
    assert!(profile.contains("Merged"));

    let directory = fixture.get("/maintainers?lang=en", StatusCode::OK).await;
    assert!(directory.contains("maintainer-card--founder"));
    for zone in ["runtime", "tokenizer", "model"] {
        assert!(directory.contains(&format!("/project/map?node={zone}&amp;lang=en")));
    }
    assert!(directory.contains("Recent contribution"));
    assert!(directory.contains("/pull/25") || directory.contains("/commit/fake-"));

    sqlx::query(
        "INSERT INTO articles (author_id, title, body, language, published_at) VALUES
         (?, 'Older article', '', 'en', '2026-01-01'),
         (?, 'Latest public article', '', 'en', '2026-01-02'),
         (?, 'Private draft', '', 'en', NULL)",
    )
    .bind(FOUNDER)
    .bind(FOUNDER)
    .bind(FOUNDER)
    .execute(&fixture.state.db)
    .await
    .unwrap();
    let directory = fixture.get("/maintainers", StatusCode::OK).await;
    assert!(directory.contains("Последняя статья"));
    assert!(directory.contains("Latest public article"));
    assert!(!directory.contains("Older article") && !directory.contains("Private draft"));

    fixture.fail_api.store(true, Ordering::Relaxed);
    assert!(
        fixture
            .get("/project/tree?lang=en", StatusCode::OK)
            .await
            .contains("feature/architecture")
    );
}

#[tokio::test]
async fn account_bookmarks_profile_and_session_boundaries() {
    let fixture = Fixture::new().await;
    let guest = fixture.get("/account?lang=en", StatusCode::OK).await;
    assert!(guest.contains("My account") && guest.contains("/auth/github?next=account"));
    assert_eq!(
        fixture
            .request("/auth/github?next=https%3A%2F%2Fexample.com", "", None)
            .await
            .status(),
        StatusCode::BAD_REQUEST,
    );

    let reader = fixture
        .sign_in_from("999", StatusCode::SEE_OTHER, true)
        .await;
    let other_device = fixture
        .sign_in_from("999", StatusCode::SEE_OTHER, true)
        .await;
    let founder = fixture.sign_in(FOUNDER, StatusCode::SEE_OTHER).await;
    let csrf = fixture.csrf(&reader).await;

    let reader_page = fixture.request("/account?lang=en", &reader, None).await;
    assert_eq!(reader_page.headers()[header::CACHE_CONTROL], "no-store");
    let html = rendered(reader_page).await;
    assert!(html.contains("Outsider") && html.contains("Reader"));
    assert!(!html.contains("Open studio"));
    assert_eq!(
        fixture.request("/studio/new", &reader, None).await.status(),
        StatusCode::FORBIDDEN,
    );

    let profile = [
        ("csrf", csrf.as_str()),
        ("display_name", "  Pixel Reader  "),
    ];
    let bad_profile = [("csrf", "wrong"), ("display_name", "Spoof")];
    for (form, status) in [
        (&bad_profile, StatusCode::FORBIDDEN),
        (&profile, StatusCode::SEE_OTHER),
    ] {
        assert_eq!(
            fixture
                .request("/account/profile", &reader, Some(form))
                .await
                .status(),
            status
        );
    }
    let html = rendered(
        fixture
            .request("/account?saved=true&lang=en", &other_device, None)
            .await,
    )
    .await;
    assert!(html.contains("Pixel Reader") && html.contains("Profile saved."));

    sqlx::query(
        "INSERT INTO articles (author_id, title, body, language, published_at) VALUES
         (?, 'Public article', '', 'en', '2026-01-01'),
         (?, 'Private draft', '', 'en', NULL)",
    )
    .bind(FOUNDER)
    .bind(FOUNDER)
    .execute(&fixture.state.db)
    .await
    .unwrap();
    let save = [
        ("csrf", csrf.as_str()),
        ("intent", "save"),
        ("from_article", "true"),
    ];
    let bad_csrf = [("csrf", "wrong"), ("intent", "save")];
    for (path, session, form, status) in [
        ("/account/bookmarks/1", "", &save[..], StatusCode::FORBIDDEN),
        (
            "/account/bookmarks/1",
            reader.as_str(),
            &bad_csrf[..],
            StatusCode::FORBIDDEN,
        ),
        (
            "/account/bookmarks/2",
            reader.as_str(),
            &save[..],
            StatusCode::NOT_FOUND,
        ),
        (
            "/account/bookmarks/1",
            reader.as_str(),
            &save[..],
            StatusCode::SEE_OTHER,
        ),
        (
            "/account/bookmarks/1",
            reader.as_str(),
            &save[..],
            StatusCode::SEE_OTHER,
        ),
    ] {
        let response = fixture.request(path, session, Some(form)).await;
        assert_eq!(response.status(), status);
        if status == StatusCode::SEE_OTHER {
            assert_eq!(
                response.headers()[header::LOCATION],
                "/articles/1#article-bookmark"
            );
        }
    }

    let article_page = fixture.request("/articles/1?lang=en", &reader, None).await;
    assert_eq!(article_page.headers()[header::CACHE_CONTROL], "no-store");
    assert!(
        rendered(article_page)
            .await
            .contains("Remove from saved articles")
    );
    let saved = rendered(fixture.request("/account?lang=en", &reader, None).await).await;
    assert_eq!(saved.matches("href=\"/articles/1\"").count(), 1);
    assert!(!saved.contains("Private draft"));
    let founder_page = rendered(fixture.request("/account?lang=en", &founder, None).await).await;
    assert!(
        founder_page.contains("Manage maintainers") && !founder_page.contains("Public article")
    );

    let remove = [("csrf", csrf.as_str()), ("intent", "remove")];
    assert_eq!(
        fixture
            .request("/account/bookmarks/1", &reader, Some(&remove))
            .await
            .status(),
        StatusCode::SEE_OTHER,
    );
    let account = rendered(fixture.request("/account", &reader, None).await).await;
    assert!(!account.contains("Public article"));

    let exit = [
        ("csrf", csrf.as_str()),
        ("all", "true"),
        ("account", "true"),
    ];
    let response = fixture.request("/auth/logout", &reader, Some(&exit)).await;
    assert_eq!(response.headers()[header::LOCATION], "/account");
    for session in [&reader, &other_device] {
        let page = rendered(fixture.request("/account?lang=en", session, None).await).await;
        assert!(page.contains("Continue with GitHub") && !page.contains("Pixel Reader"));
    }
    assert!(
        rendered(fixture.request("/account?lang=en", &founder, None).await)
            .await
            .contains("Manage maintainers")
    );
}
