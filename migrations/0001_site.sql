CREATE TABLE maintainers (
    github_id INTEGER PRIMARY KEY CHECK (github_id > 0),
    login TEXT NOT NULL,
    avatar_url TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('founder', 'maintainer', 'inactive'))
);

INSERT INTO maintainers VALUES
    (132206435, 'RUdolf1517', 'https://avatars.githubusercontent.com/u/132206435?v=4', 'founder');

CREATE TABLE articles (
    id INTEGER PRIMARY KEY,
    author_id INTEGER NOT NULL REFERENCES maintainers(github_id),
    title TEXT NOT NULL CHECK (length(title) BETWEEN 1 AND 160),
    body TEXT NOT NULL CHECK (length(body) <= 50000),
    language TEXT NOT NULL CHECK (language IN ('ru', 'en')),
    published_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M', 'now')),
    revision INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX articles_public ON articles(published_at DESC, id DESC);
CREATE INDEX articles_author ON articles(author_id, updated_at DESC);

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    github_id INTEGER NOT NULL REFERENCES maintainers(github_id),
    csrf TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE TABLE oauth_flows (
    state_hash TEXT PRIMARY KEY,
    verifier TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE INDEX sessions_expiry ON sessions(expires_at);
CREATE INDEX oauth_flows_expiry ON oauth_flows(expires_at);
