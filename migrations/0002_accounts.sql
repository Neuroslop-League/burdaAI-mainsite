CREATE TABLE accounts (
    github_id INTEGER PRIMARY KEY CHECK (github_id > 0),
    login TEXT NOT NULL,
    avatar_url TEXT NOT NULL,
    display_name TEXT NOT NULL DEFAULT '' CHECK (length(display_name) <= 60)
);

INSERT INTO accounts (github_id, login, avatar_url)
SELECT github_id, login, avatar_url FROM maintainers;

CREATE TABLE account_sessions (
    token_hash TEXT PRIMARY KEY,
    github_id INTEGER NOT NULL REFERENCES accounts(github_id) ON DELETE CASCADE,
    csrf TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);

INSERT INTO account_sessions SELECT * FROM sessions;
DROP TABLE sessions;
ALTER TABLE account_sessions RENAME TO sessions;
CREATE INDEX sessions_expiry ON sessions(expires_at);
CREATE INDEX sessions_account ON sessions(github_id);

ALTER TABLE oauth_flows ADD COLUMN destination TEXT NOT NULL DEFAULT 'studio'
    CHECK (destination IN ('studio', 'account'));

CREATE TABLE bookmarks (
    github_id INTEGER NOT NULL REFERENCES accounts(github_id) ON DELETE CASCADE,
    article_id INTEGER NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    saved_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (github_id, article_id)
);
