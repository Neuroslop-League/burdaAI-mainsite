use std::{path::PathBuf, time::Duration};

use sqlx::{
    FromRow, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};

pub async fn open(path: PathBuf) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

#[derive(FromRow)]
pub struct Maintainer {
    pub github_id: i64,
    pub login: String,
    pub avatar_url: String,
    pub role: String,
}

impl Maintainer {
    pub fn founder(&self) -> bool {
        self.role == "founder"
    }

    pub fn active(&self) -> bool {
        self.role != "inactive"
    }
}

pub async fn maintainers(db: &SqlitePool) -> Result<Vec<Maintainer>, sqlx::Error> {
    sqlx::query_as(
        "SELECT * FROM maintainers WHERE role != 'inactive' ORDER BY role, login COLLATE NOCASE",
    )
    .fetch_all(db)
    .await
}

pub async fn maintainer(db: &SqlitePool, id: i64) -> Result<Option<Maintainer>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM maintainers WHERE github_id = ?")
        .bind(id)
        .fetch_optional(db)
        .await
}

#[derive(FromRow)]
pub struct MaintainerArticle {
    pub author_id: i64,
    pub id: i64,
    pub title: String,
    pub language: String,
}

pub async fn maintainer_articles(db: &SqlitePool) -> Result<Vec<MaintainerArticle>, sqlx::Error> {
    sqlx::query_as(
        "SELECT a.author_id, a.id, a.title, a.language
         FROM maintainers m JOIN articles a ON a.id = (
             SELECT id FROM articles
             WHERE author_id = m.github_id AND published_at IS NOT NULL
             ORDER BY published_at DESC, id DESC LIMIT 1
         )
         WHERE m.role != 'inactive'",
    )
    .fetch_all(db)
    .await
}

#[derive(FromRow)]
pub struct Article {
    pub id: i64,
    pub author_id: i64,
    pub author: String,
    pub title: String,
    pub body: String,
    pub language: String,
    pub date: String,
    pub published: bool,
    pub revision: i64,
}

const ARTICLES: &str = "SELECT a.id, a.author_id, m.login AS author, a.title, a.body, a.language,
    COALESCE(a.published_at, a.updated_at) AS date, a.published_at IS NOT NULL AS published, a.revision
    FROM articles a JOIN maintainers m ON m.github_id = a.author_id";

pub async fn articles(
    db: &SqlitePool,
    author: Option<i64>,
    include_drafts: bool,
) -> Result<Vec<Article>, sqlx::Error> {
    let query = format!(
        "{ARTICLES} WHERE (? IS NULL OR a.author_id = ?) AND (? OR a.published_at IS NOT NULL)
         ORDER BY date DESC, a.id DESC"
    );
    sqlx::query_as(&query)
        .bind(author)
        .bind(author)
        .bind(include_drafts)
        .fetch_all(db)
        .await
}

pub async fn article(db: &SqlitePool, id: i64) -> Result<Option<Article>, sqlx::Error> {
    sqlx::query_as(&format!("{ARTICLES} WHERE a.id = ?"))
        .bind(id)
        .fetch_optional(db)
        .await
}
