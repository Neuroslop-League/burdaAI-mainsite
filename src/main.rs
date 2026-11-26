mod auth;
mod db;
mod github;
mod project_map;
mod prompt_trace;
mod web;

use std::{env, error::Error, net::SocketAddr, path::PathBuf};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    dotenvy::dotenv().ok();
    let address: SocketAddr = env::var("BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".into())
        .parse()?;
    let state = web::AppState::new(
        db::open(PathBuf::from(
            env::var("DATABASE_PATH").unwrap_or_else(|_| "burdaai.db".into()),
        ))
        .await?,
        github::GitHub::new(
            env::var("GITHUB_API_TOKEN")
                .ok()
                .filter(|token| !token.is_empty()),
        )?,
        auth::OAuth::from_env()?,
    );
    let listener = tokio::net::TcpListener::bind(address).await?;
    eprintln!("burdaAI · http://{}", listener.local_addr()?);
    axum::serve(listener, web::app(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
