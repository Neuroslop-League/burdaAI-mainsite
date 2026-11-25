use std::{
    collections::BTreeMap,
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{DateTime, Days, Utc};
use reqwest::header::{ACCEPT, LINK};
use serde::{Deserialize, de::DeserializeOwned};
use tokio::sync::Mutex;
use url::Url;

const ORG: &str = "Neuroslop-League";
pub const REPOSITORIES: [&str; 2] = ["burdaAI", "burdaAI-mainsite"];
const CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const RETRY_DELAY: Duration = Duration::from_secs(60);
type Fetch<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone)]
pub struct GitHub {
    client: reqwest::Client,
    token: Option<String>,
    pub(crate) base: Url,
    tree: Arc<Mutex<Cache<Tree>>>,
    activity: Arc<Mutex<Cache<Vec<Activity>>>>,
}

impl GitHub {
    pub fn new(token: Option<String>) -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: reqwest::Client::builder()
                .user_agent("burdaAI-site")
                .timeout(Duration::from_secs(8))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            token,
            base: "https://api.github.com".parse().unwrap(),
            tree: Arc::new(Mutex::new(Cache::new())),
            activity: Arc::new(Mutex::new(Cache::new())),
        })
    }

    fn url(&self, segments: &[&str]) -> Url {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .unwrap()
            .pop_if_empty()
            .extend(segments);
        url
    }

    fn repo_url(&self, repo: &str, segments: &[&str]) -> Url {
        let mut path = vec!["repos", ORG, repo];
        path.extend_from_slice(segments);
        self.url(&path)
    }

    async fn fetch<T: DeserializeOwned>(&self, url: Url) -> Fetch<(T, bool)> {
        let mut request = self
            .client
            .get(url)
            .header(ACCEPT, "application/vnd.github+json");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(format!("GitHub HTTP {}", response.status()).into());
        }
        let has_next = response
            .headers()
            .get(LINK)
            .and_then(|header| header.to_str().ok())
            .is_some_and(|links| links.split(',').any(|link| link.contains("rel=\"next\"")));
        Ok((response.json().await?, has_next))
    }

    pub async fn identity(&self, login: &str) -> Fetch<Option<Identity>> {
        let lookup = self.fetch(self.url(&["users", login])).await;
        match lookup {
            Ok((identity, _)) => Ok(Some(identity)),
            Err(error) if error.to_string() == "GitHub HTTP 404 Not Found" => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub async fn tree(&self) -> Snapshot<Tree> {
        cached(&self.tree, self.load_tree()).await
    }

    pub async fn activity(&self) -> Snapshot<Vec<Activity>> {
        cached(&self.activity, self.load_activity()).await
    }

    async fn load_tree(&self) -> Fetch<Tree> {
        let repo = REPOSITORIES[0];
        let (details, _): (Repository, _) = self.fetch(self.repo_url(repo, &[])).await?;
        let (tip, _): (CommitTip, _) = self
            .fetch(self.repo_url(repo, &["commits", &details.default_branch]))
            .await?;
        let mut recursive = self.repo_url(repo, &["git", "trees", &tip.commit.tree.sha]);
        recursive.query_pairs_mut().append_pair("recursive", "1");
        let (response, _): (GitTree, _) = self.fetch(recursive).await?;
        let mut root = Node::default();

        if response.truncated {
            let mut pending = vec![(String::new(), tip.commit.tree.sha)];
            while let Some((prefix, sha)) = pending.pop() {
                let (subtree, _): (GitTree, _) = self
                    .fetch(self.repo_url(repo, &["git", "trees", &sha]))
                    .await?;
                if subtree.truncated {
                    return Err("GitHub truncated a non-recursive subtree".into());
                }
                for entry in subtree.tree {
                    let path = format!("{prefix}{}", entry.path);
                    let folder = entry.kind == "tree";
                    root.insert(&path, folder);
                    if folder {
                        pending.push((format!("{path}/"), entry.sha));
                    }
                }
            }
        } else {
            for entry in response.tree {
                root.insert(&entry.path, entry.kind == "tree");
            }
        }
        let mut rows = Vec::new();
        root.rows("", &tip.sha, &mut rows);
        Ok(Tree {
            branch: details.default_branch,
            sha: tip.sha,
            rows,
        })
    }

    async fn load_activity(&self) -> Fetch<Vec<Activity>> {
        let cutoff = Utc::now().checked_sub_days(Days::new(90)).unwrap();
        let (mut model, website) = tokio::try_join!(
            self.repository_activity(REPOSITORIES[0], cutoff),
            self.repository_activity(REPOSITORIES[1], cutoff),
        )?;
        model.extend(website);
        model.sort_by_key(|event| std::cmp::Reverse(event.at));
        Ok(model)
    }

    async fn repository_activity(
        &self,
        repo: &'static str,
        cutoff: DateTime<Utc>,
    ) -> Fetch<Vec<Activity>> {
        let (details, _): (Repository, _) = self.fetch(self.repo_url(repo, &[])).await?;
        let mut events = Vec::new();
        for page in 1.. {
            let mut url = self.repo_url(repo, &["commits"]);
            url.query_pairs_mut().extend_pairs([
                ("sha", details.default_branch.as_str()),
                ("since", cutoff.to_rfc3339().as_str()),
                ("per_page", "100"),
                ("page", &page.to_string()),
            ]);
            let (commits, more): (Vec<GitCommit>, _) = self.fetch(url).await?;
            for commit in commits {
                if let (Some(author), Some(when)) = (commit.author, commit.commit.committer)
                    && let Some(at) = parse_date(&when.date)
                    && at >= cutoff
                {
                    events.push(Activity {
                        author_id: author.id,
                        repo,
                        kind: "commit",
                        state: "",
                        at,
                        date: at.format("%Y-%m-%d").to_string(),
                        title: commit
                            .commit
                            .message
                            .lines()
                            .next()
                            .unwrap_or_default()
                            .into(),
                        url: web_url(repo, &["commit", &commit.sha]),
                    });
                }
            }
            if !more {
                break;
            }
        }
        for page in 1.. {
            let mut url = self.repo_url(repo, &["pulls"]);
            url.query_pairs_mut().extend_pairs([
                ("state", "all"),
                ("sort", "created"),
                ("direction", "desc"),
                ("per_page", "100"),
                ("page", &page.to_string()),
            ]);
            let (pulls, more): (Vec<GitPull>, _) = self.fetch(url).await?;
            let mut reached_cutoff = false;
            for pull in pulls {
                let Some(at) = parse_date(&pull.created_at) else {
                    continue;
                };
                if at < cutoff {
                    reached_cutoff = true;
                    break;
                }
                let Some(author) = pull.user else { continue };
                let state = if pull.merged_at.is_some() {
                    "merged"
                } else if pull.draft {
                    "draft"
                } else if pull.state == "open" {
                    "open"
                } else {
                    "closed"
                };
                events.push(Activity {
                    author_id: author.id,
                    repo,
                    kind: "pr",
                    state,
                    at,
                    date: at.format("%Y-%m-%d").to_string(),
                    title: pull.title,
                    url: web_url(repo, &["pull", &pull.number.to_string()]),
                });
            }
            if !more || reached_cutoff {
                break;
            }
        }
        Ok(events)
    }
}

fn parse_date(date: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(date)
        .ok()
        .map(|date| date.to_utc())
}

fn web_url(repo: &str, segments: &[&str]) -> String {
    let mut url: Url = "https://github.com".parse().unwrap();
    url.path_segments_mut()
        .unwrap()
        .pop_if_empty()
        .extend([ORG, repo])
        .extend(segments);
    url.into()
}

#[derive(Deserialize)]
pub struct Identity {
    pub id: i64,
    pub login: String,
}

impl Identity {
    pub fn avatar(&self) -> String {
        format!("https://avatars.githubusercontent.com/u/{}?v=4", self.id)
    }
}

#[derive(Deserialize)]
struct Repository {
    default_branch: String,
}

#[derive(Deserialize)]
struct CommitTip {
    sha: String,
    commit: CommitTree,
}

#[derive(Deserialize)]
struct CommitTree {
    tree: TreeRef,
}

#[derive(Deserialize)]
struct TreeRef {
    sha: String,
}

#[derive(Deserialize)]
struct GitTree {
    truncated: bool,
    tree: Vec<GitTreeEntry>,
}

#[derive(Deserialize)]
struct GitTreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    sha: String,
}

#[derive(Default)]
struct Node {
    folder: bool,
    children: BTreeMap<String, Node>,
}

impl Node {
    fn insert(&mut self, path: &str, folder: bool) {
        let mut parent = self;
        let mut parts = path.split('/').peekable();
        while let Some(segment) = parts.next() {
            parent = parent.children.entry(segment.into()).or_default();
            parent.folder |= parts.peek().is_some() || folder;
        }
    }

    fn rows(&self, prefix: &str, commit: &str, output: &mut Vec<TreeRow>) {
        let mut entries: Vec<_> = self.children.iter().collect();
        entries.sort_by_key(|(name, child)| (!child.folder, *name));
        for (name, child) in entries {
            let path = format!("{prefix}{name}");
            let kind = if child.folder { "tree" } else { "blob" };
            let mut segments = vec![kind, commit];
            segments.extend(path.split('/'));
            output.push(TreeRow {
                kind,
                label: name.clone(),
                url: web_url(REPOSITORIES[0], &segments),
            });
            if child.folder {
                child.rows(&format!("{path}/"), commit, output);
                output.push(TreeRow {
                    kind: "end",
                    label: String::new(),
                    url: String::new(),
                });
            }
        }
    }
}

pub struct TreeRow {
    pub kind: &'static str,
    pub label: String,
    pub url: String,
}

pub struct Tree {
    pub branch: String,
    pub sha: String,
    pub rows: Vec<TreeRow>,
}

#[derive(Deserialize)]
struct GitCommit {
    sha: String,
    author: Option<GitAuthor>,
    commit: GitCommitInfo,
}

#[derive(Deserialize)]
struct GitAuthor {
    id: i64,
}

#[derive(Deserialize)]
struct GitCommitInfo {
    message: String,
    committer: Option<GitDate>,
}

#[derive(Deserialize)]
struct GitDate {
    date: String,
}

#[derive(Deserialize)]
struct GitPull {
    number: u64,
    title: String,
    state: String,
    created_at: String,
    merged_at: Option<String>,
    draft: bool,
    user: Option<GitAuthor>,
}

pub struct Activity {
    pub author_id: i64,
    pub repo: &'static str,
    pub kind: &'static str,
    pub state: &'static str,
    at: DateTime<Utc>,
    pub date: String,
    pub title: String,
    pub url: String,
}

struct Cache<T> {
    value: Option<Arc<T>>,
    updated: String,
    retry_at: Instant,
    stale: bool,
}

impl<T> Cache<T> {
    fn new() -> Self {
        Self {
            value: None,
            updated: String::new(),
            retry_at: Instant::now(),
            stale: false,
        }
    }
}

pub struct Snapshot<T> {
    pub value: Option<Arc<T>>,
    pub updated: String,
    pub stale: bool,
}

async fn cached<T>(lock: &Mutex<Cache<T>>, load: impl Future<Output = Fetch<T>>) -> Snapshot<T> {
    let mut cache = lock.lock().await;
    if Instant::now() >= cache.retry_at {
        match tokio::time::timeout(Duration::from_secs(15), load).await {
            Ok(Ok(value)) => {
                cache.value = Some(Arc::new(value));
                cache.updated = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
                cache.retry_at = Instant::now() + CACHE_TTL;
                cache.stale = false;
            }
            _ => {
                cache.retry_at = Instant::now() + RETRY_DELAY;
                cache.stale = true;
            }
        }
    }
    Snapshot {
        value: cache.value.clone(),
        updated: cache.updated.clone(),
        stale: cache.stale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cache_retains_last_success_and_reports_unavailable_cold_cache() {
        let cache = Mutex::new(Cache::new());
        let ok = cached(&cache, async { Ok(17) }).await;
        assert_eq!(ok.value.as_deref(), Some(&17));
        assert!(!ok.stale);
        cache.lock().await.retry_at = Instant::now();
        let stale = cached(&cache, async { Err("offline".into()) }).await;
        assert_eq!(stale.value.as_deref(), Some(&17));
        assert_eq!(stale.updated, ok.updated);
        assert!(stale.stale);
        let cold = cached(&Mutex::new(Cache::<i32>::new()), async {
            Err("offline".into())
        })
        .await;
        assert!(cold.stale && cold.value.is_none());
    }
}
