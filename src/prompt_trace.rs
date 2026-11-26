use std::time::Duration;

use axum::{Form, Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use tokenizers::Tokenizer;
use tokio::sync::OnceCell;

const TOKENIZER_URL: &str = "https://raw.githubusercontent.com/Neuroslop-League/burdaAI/2270079a60cbd5daf087ffce35b1f64e0c043ffc/artifacts/c02-v1/tokenizer.json";
// The burdaAI Rust generator prepends this ID before passing the prompt to the decoder.
const BOS: u32 = 1;

static TOKENIZER: OnceCell<Tokenizer> = OnceCell::const_new();

#[derive(Deserialize)]
pub struct Prompt {
    prompt: String,
}

#[derive(Serialize)]
pub struct Token {
    id: u32,
    piece: String,
    text: String,
}

#[derive(Serialize)]
struct Choice {
    id: u32,
    piece: String,
    output: String,
}

#[derive(Serialize)]
pub struct Trace {
    tokens: Vec<Token>,
    decoded: String,
    choices: Vec<Choice>,
}

pub async fn tokenize(Form(Prompt { prompt }): Form<Prompt>) -> Result<Json<Trace>, StatusCode> {
    if prompt.trim().is_empty() || prompt.encode_utf16().count() > 400 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let tokenizer = TOKENIZER
        .get_or_try_init(|| async {
            let bytes = reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .build()?
                .get(TOKENIZER_URL)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            Tokenizer::from_bytes(bytes)
        })
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    trace(tokenizer, &prompt)
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn trace(tokenizer: &Tokenizer, prompt: &str) -> tokenizers::Result<Trace> {
    let encoding = tokenizer.encode(prompt, false)?;
    let bos = tokenizer.id_to_token(BOS).unwrap_or_else(|| "<bos>".into());
    let mut tokens = vec![Token {
        id: BOS,
        text: bos.clone(),
        piece: bos,
    }];
    for (&id, piece) in encoding.get_ids().iter().zip(encoding.get_tokens()) {
        tokens.push(Token {
            id,
            piece: piece.clone(),
            text: tokenizer.decode(&[id], false)?,
        });
    }
    let decoded = tokenizer.decode(encoding.get_ids(), true)?;
    // These are selectable examples from the vocabulary, independent of model scores.
    let mut choices = Vec::new();
    let mut sequence = encoding.get_ids().to_vec();
    for piece in [".", "!", "?"] {
        let encoded = tokenizer.encode(piece, false)?;
        if let &[id] = encoded.get_ids() {
            sequence.push(id);
            choices.push(Choice {
                id,
                piece: piece.into(),
                output: tokenizer.decode(&sequence, true)?,
            });
            sequence.pop();
        }
    }
    Ok(Trace {
        tokens,
        decoded,
        choices,
    })
}
