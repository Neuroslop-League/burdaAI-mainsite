# burdaAI

The Neuroslop League website for [burdaAI](https://github.com/Neuroslop-League/burdaAI), built with Rust, Axum, and Askama. Public pages include project articles, maintainer profiles and GitHub activity, an interactive component map, and a tree of pull requests targeting the model repository's default branch. SQLite stores articles, sessions, and maintainer roles.

## Run locally

From the repository root, run:

    cargo run --locked

Open [the local site](http://127.0.0.1:3000/). On first startup, the server creates `burdaai.db` and applies its migrations. Set `DATABASE_PATH` to use a different SQLite file or `BIND_ADDR` to change the server address.

Public pages do not require sign-in. The pull request tree and maintainer activity use the GitHub REST API. You can set `GITHUB_API_TOKEN` to a token with read access to the public repositories for a higher rate limit. Responses are cached in memory for 15 minutes. If a refresh fails, the site shows the latest successful response from the current server process.

## GitHub sign-in, personal accounts, and the article editor

Create a local [GitHub OAuth App](https://github.com/settings/developers) with `http://127.0.0.1:3000/` as its homepage and `http://127.0.0.1:3000/auth/github/callback` as its callback URL. Set the client ID and client secret in a local `.env` file:

    GITHUB_CLIENT_ID=
    GITHUB_CLIENT_SECRET=
    GITHUB_CALLBACK_URL=http://127.0.0.1:3000/auth/github/callback
    # GITHUB_API_TOKEN=
    # DATABASE_PATH=burdaai.db
    # BIND_ADDR=127.0.0.1:3000

Restart the server and open [My account](http://127.0.0.1:3000/account) from the footer. Anyone who signs in with GitHub can set a display name, save published articles, and sign out on all devices. Maintainers can open [Studio](http://127.0.0.1:3000/studio) to draft, preview, and publish their own Markdown articles. The founder, **RUdolf1517**, manages the maintainer roster. Saving a published article updates its public version immediately. Unpublishing returns it to drafts.

## Checks

    cargo fmt --check
    cargo test --locked

Integration tests use a temporary SQLite database and a mock GitHub HTTP server. Use your local OAuth app to test real GitHub sign-in.

## License

The site code is available under the [MIT License](LICENSE). The bundled Press Start 2P font is distributed under the [SIL Open Font License 1.1](static/assets/OFL.txt).
