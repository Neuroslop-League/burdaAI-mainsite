use std::{env, time::Duration};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use url::Url;
use uuid::Uuid;

use crate::{
    github::Identity,
    web::{AppState, Failure},
};

const SESSION: &str = "burda_session";
const FLOW: &str = "burda_oauth";
const SESSION_MAX: i64 = 8 * 60 * 60;
const FLOW_MAX: i64 = 10 * 60;

#[derive(Clone)]
pub struct OAuth {
    client: reqwest::Client,
    id: String,
    secret: String,
    callback: Url,
    pub(crate) authorize_url: Url,
    pub(crate) token_url: Url,
    pub(crate) user_url: Url,
}

impl OAuth {
    pub fn from_env() -> Result<Option<Self>, Box<dyn std::error::Error + Send + Sync>> {
        let id = env::var("GITHUB_CLIENT_ID").ok().filter(|v| !v.is_empty());
        let secret = env::var("GITHUB_CLIENT_SECRET")
            .ok()
            .filter(|v| !v.is_empty());
        match (id, secret) {
            (None, None) => Ok(None),
            (Some(id), Some(secret)) => {
                let callback = env::var("GITHUB_CALLBACK_URL")
                    .unwrap_or_else(|_| "http://127.0.0.1:3000/auth/github/callback".into());
                let callback = Url::parse(&callback)?;
                let loopback = matches!(
                    callback.host_str(),
                    Some("127.0.0.1" | "localhost" | "[::1]")
                );
                if !(callback.scheme() == "https" || callback.scheme() == "http" && loopback)
                    || callback.path() != "/auth/github/callback"
                    || callback.query().is_some()
                    || callback.fragment().is_some()
                    || !callback.username().is_empty()
                    || callback.password().is_some()
                {
                    return Err("GITHUB_CALLBACK_URL must use HTTPS or local HTTP and end in /auth/github/callback".into());
                }
                Ok(Some(Self::new(id, secret, callback)?))
            }
            _ => Err("Set both GITHUB_CLIENT_ID and GITHUB_CLIENT_SECRET".into()),
        }
    }

    pub(crate) fn new(id: String, secret: String, callback: Url) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: reqwest::Client::builder()
                .user_agent("burdaAI-site")
                .timeout(Duration::from_secs(8))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            id,
            secret,
            callback,
            authorize_url: "https://github.com/login/oauth/authorize".parse().unwrap(),
            token_url: "https://github.com/login/oauth/access_token"
                .parse()
                .unwrap(),
            user_url: "https://api.github.com/user".parse().unwrap(),
        })
    }

    fn secure(&self) -> bool {
        self.callback.scheme() == "https"
    }
}

#[derive(sqlx::FromRow)]
pub struct Viewer {
    pub github_id: i64,
    pub login: String,
    pub avatar_url: String,
    pub display_name: String,
    pub role: String,
    pub csrf: String,
}

impl Viewer {
    pub fn founder(&self) -> bool {
        self.role == "founder"
    }

    pub fn maintainer(&self) -> bool {
        matches!(self.role.as_str(), "founder" | "maintainer")
    }

    pub fn name(&self) -> &str {
        if self.display_name.is_empty() {
            &self.login
        } else {
            &self.display_name
        }
    }

    pub fn check_csrf(&self, input: &str) -> Result<(), Failure> {
        if self.csrf.as_bytes().ct_eq(input.as_bytes()).into() {
            Ok(())
        } else {
            Err(Failure::forbidden())
        }
    }
}

fn random() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn hash(value: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(value.as_bytes()))
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn cookie(name: &'static str, value: String, age: i64, secure: bool) -> Cookie<'static> {
    Cookie::build((name, value))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .max_age(time::Duration::seconds(age))
        .build()
}

pub fn has_session(jar: &CookieJar) -> bool {
    jar.get(SESSION).is_some()
}

pub async fn viewer(state: &AppState, jar: &CookieJar) -> Result<Option<Viewer>, Failure> {
    let Some(session) = jar.get(SESSION) else {
        return Ok(None);
    };
    sqlx::query_as(
        "SELECT a.github_id, a.login, a.avatar_url, a.display_name, s.csrf,
                COALESCE(NULLIF(m.role, 'inactive'), 'reader') AS role
         FROM sessions s JOIN accounts a USING (github_id)
         LEFT JOIN maintainers m USING (github_id)
         WHERE token_hash = ? AND expires_at > ?",
    )
    .bind(hash(session.value()))
    .bind(now())
    .fetch_optional(&state.db)
    .await
    .map_err(Into::into)
}

#[derive(Deserialize)]
pub struct Start {
    #[serde(default)]
    next: Destination,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Destination {
    #[default]
    Studio,
    Account,
}

impl Destination {
    fn key(&self) -> &'static str {
        match self {
            Self::Studio => "studio",
            Self::Account => "account",
        }
    }
}

pub async fn start(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(start): Query<Start>,
) -> Result<Response, Failure> {
    let oauth = state
        .oauth
        .as_ref()
        .ok_or_else(Failure::oauth_unavailable)?;
    let csrf_state = random();
    let verifier = random();
    sqlx::query("DELETE FROM oauth_flows WHERE expires_at <= ?")
        .bind(now())
        .execute(&state.db)
        .await?;
    sqlx::query("INSERT INTO oauth_flows (state_hash, verifier, expires_at, destination) VALUES (?, ?, ?, ?)")
        .bind(hash(&csrf_state))
        .bind(&verifier)
        .bind(now() + FLOW_MAX)
        .bind(start.next.key())
        .execute(&state.db)
        .await?;
    let mut url = oauth.authorize_url.clone();
    url.query_pairs_mut().extend_pairs([
        ("client_id", oauth.id.as_str()),
        ("redirect_uri", oauth.callback.as_str()),
        ("state", csrf_state.as_str()),
        ("code_challenge", hash(&verifier).as_str()),
        ("code_challenge_method", "S256"),
    ]);
    Ok((
        jar.add(cookie(FLOW, csrf_state, FLOW_MAX, oauth.secure())),
        Redirect::to(url.as_str()),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct Callback {
    code: Option<String>,
    state: Option<String>,
}

#[derive(Deserialize)]
struct Token {
    access_token: String,
}

pub async fn callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<Callback>,
) -> Result<Response, Failure> {
    let oauth = state
        .oauth
        .as_ref()
        .ok_or_else(Failure::oauth_unavailable)?;
    let (Some(code), Some(oauth_state), Some(flow)) = (query.code, query.state, jar.get(FLOW))
    else {
        return Err(Failure::invalid_login());
    };
    if !bool::from(flow.value().as_bytes().ct_eq(oauth_state.as_bytes())) {
        return Err(Failure::invalid_login());
    }
    let (verifier, destination): (String, String) = sqlx::query_as(
        "DELETE FROM oauth_flows WHERE state_hash = ? AND expires_at > ? RETURNING verifier, destination",
    )
    .bind(hash(&oauth_state))
    .bind(now())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(Failure::invalid_login)?;

    let token: Token = oauth
        .client
        .post(oauth.token_url.clone())
        .header("Accept", "application/json")
        .form(&[
            ("client_id", oauth.id.as_str()),
            ("client_secret", oauth.secret.as_str()),
            ("redirect_uri", oauth.callback.as_str()),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|_| Failure::github())?
        .error_for_status()
        .map_err(|_| Failure::github())?
        .json()
        .await
        .map_err(|_| Failure::invalid_login())?;

    let identity: Identity = oauth
        .client
        .get(oauth.user_url.clone())
        .bearer_auth(token.access_token)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| Failure::github())?
        .error_for_status()
        .map_err(|_| Failure::github())?
        .json()
        .await
        .map_err(|_| Failure::github())?;

    let mut transaction = state.db.begin().await?;
    let active = sqlx::query(
        "UPDATE maintainers SET login = ?, avatar_url = ? WHERE github_id = ? AND role != 'inactive'",
    )
    .bind(&identity.login).bind(identity.avatar()).bind(identity.id)
    .execute(&mut *transaction).await?.rows_affected();
    if active == 0 && destination == "studio" {
        return Err(Failure::forbidden());
    }
    sqlx::query(
        "INSERT INTO accounts (github_id, login, avatar_url) VALUES (?, ?, ?)
         ON CONFLICT(github_id) DO UPDATE SET login = excluded.login, avatar_url = excluded.avatar_url",
    )
    .bind(identity.id)
    .bind(&identity.login)
    .bind(identity.avatar())
    .execute(&mut *transaction)
    .await?;

    sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
        .bind(now())
        .execute(&mut *transaction)
        .await?;
    if let Some(old) = jar.get(SESSION) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(hash(old.value()))
            .execute(&mut *transaction)
            .await?;
    }
    let token = random();
    sqlx::query("INSERT INTO sessions VALUES (?, ?, ?, ?)")
        .bind(hash(&token))
        .bind(identity.id)
        .bind(random())
        .bind(now() + SESSION_MAX)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;

    Ok((
        jar.remove(cookie(FLOW, String::new(), 0, oauth.secure()))
            .add(cookie(SESSION, token, SESSION_MAX, oauth.secure())),
        Redirect::to(if destination == "account" {
            "/account"
        } else {
            "/studio"
        }),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct Logout {
    csrf: String,
    #[serde(default)]
    all: bool,
    #[serde(default)]
    account: bool,
}

pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::Form(input): axum::Form<Logout>,
) -> Result<Response, Failure> {
    let Some(session) = jar.get(SESSION) else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    let viewer = viewer(&state, &jar).await?;
    if let Some(viewer) = &viewer {
        viewer.check_csrf(&input.csrf)?;
    }
    if let Some(viewer) = viewer.filter(|_| input.all) {
        sqlx::query("DELETE FROM sessions WHERE github_id = ?")
            .bind(viewer.github_id)
            .execute(&state.db)
            .await?;
    } else {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(hash(session.value()))
            .execute(&state.db)
            .await?;
    }
    let secure = state.oauth.as_ref().is_some_and(|oauth| oauth.secure());
    Ok((
        jar.remove(cookie(SESSION, String::new(), 0, secure)),
        Redirect::to(if input.account { "/account" } else { "/" }),
    )
        .into_response())
}
