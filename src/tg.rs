use std::time::Duration;

use anyhow::{bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info, warn};

const SPACING: Duration = Duration::from_secs(3);
const MAX_ATTEMPTS: u32 = 5;

#[derive(Debug)]
pub struct Post {
    pub token: String,
    pub text: String,
    /// One row of inline URL buttons: (label, url).
    pub buttons: Vec<(String, String)>,
}

pub struct Tg {
    client: reqwest::Client,
    /// `None` means dry-run: posts are logged, not sent.
    target: Option<(String, String)>,
}

#[derive(Deserialize)]
struct ApiResponse {
    ok: bool,
    description: Option<String>,
    parameters: Option<ApiParams>,
}

#[derive(Deserialize)]
struct ApiParams {
    retry_after: Option<u64>,
}

impl Tg {
    pub fn new(bot_token: Option<String>, chat_id: Option<String>) -> Self {
        let target = match (bot_token, chat_id) {
            (Some(t), Some(c)) => Some((format!("https://api.telegram.org/bot{t}"), c)),
            _ => None,
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("http client");
        Self { client, target }
    }

    pub fn is_dry_run(&self) -> bool {
        self.target.is_none()
    }

    async fn deliver(&self, post: &Post) -> Result<()> {
        let Some((base, chat_id)) = &self.target else {
            let buttons: Vec<String> = post.buttons.iter().map(|(l, u)| format!("[{l}]({u})")).collect();
            info!(token = %post.token, "[dry-run] would post:\n{}\n{}", post.text, buttons.join(" "));
            return Ok(());
        };


        let body = json!({
            "chat_id": chat_id,
            "text": post.text,
            "parse_mode": "HTML",
            "link_preview_options": { "is_disabled": true },
            "reply_markup": {
                "inline_keyboard": [post.buttons.iter().map(|(l, u)| json!({ "text": l, "url": u })).collect::<Vec<_>>()],
            },
        });
        self.call(base, "sendMessage", &body).await
    }

    async fn call(&self, base: &str, method: &str, body: &Value) -> Result<()> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let res = self.client.post(format!("{base}/{method}")).json(body).send().await;
            let wait = match res {
                Ok(resp) => {
                    let status = resp.status();
                    let api: ApiResponse = match resp.json().await {
                        Ok(a) => a,
                        Err(e) if status.is_server_error() => {
                            warn!(%status, error = %e, "telegram server error");
                            ApiResponse { ok: false, description: None, parameters: None }
                        }
                        Err(e) => bail!("{method}: bad response ({status}): {e}"),
                    };
                    if api.ok {
                        return Ok(());
                    }
                    let desc = api.description.unwrap_or_default();
                    if status.as_u16() == 429 {
                        let secs = api.parameters.and_then(|p| p.retry_after).unwrap_or(5);
                        warn!(retry_after = secs, "telegram rate limited");
                        Duration::from_secs(secs)
                    } else if status.is_server_error() {
                        Duration::from_secs(2u64.pow(attempt))
                    } else {
                        bail!("{method}: {status} {desc}");
                    }
                }
                Err(e) => {
                    // Strip the URL so the bot token never lands in logs.
                    warn!(error = %e.without_url(), "telegram request failed");
                    Duration::from_secs(2u64.pow(attempt))
                }
            };
            if attempt >= MAX_ATTEMPTS {
                bail!("{method}: giving up after {attempt} attempts");
            }
            sleep(wait).await;
        }
    }
}

/// Single send queue with fixed spacing between posts.
pub async fn run(tg: Tg, mut rx: mpsc::Receiver<Post>) {
    while let Some(post) = rx.recv().await {
        match tg.deliver(&post).await {
            Ok(()) if tg.is_dry_run() => {}
            Ok(()) => info!(token = %post.token, "posted graduation"),
            Err(e) => error!(token = %post.token, error = %e, "failed to post graduation"),
        }
        sleep(SPACING).await;
    }
}
