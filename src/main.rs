mod dexscreener;
mod erc20;
mod event;
mod feed;
mod handler;
mod render;
mod tg;

use anyhow::{bail, Result};
use tokio::sync::mpsc;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

const DEFAULT_WS: &str = "wss://rhc.pumpdev.io/ws";
/// Free, keyless, rate limited; plenty for a couple of reads per graduation.
const DEFAULT_RPC: &str = "https://rpc.mainnet.chain.robinhood.com";

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn ws_url() -> String {
    if let Some(url) = env("PUMP_DEV_URL") {
        return url;
    }
    match env("PUMPDEV_KEY") {
        Some(key) => format!("{DEFAULT_WS}?key={key}"),
        None => {
            warn!("no PUMPDEV_KEY: keyless connections are metered per IP and may be cut with DEMO_LIMIT");
            DEFAULT_WS.to_string()
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let url = ws_url();
    if !url.starts_with("wss://") && !url.starts_with("ws://") {
        bail!("PUMP_DEV_URL must be a ws:// or wss:// URL");
    }

    let tg = tg::Tg::new(env("TG_BOT_TOKEN"), env("TG_CHANNEL_ID"));
    if tg.is_dry_run() {
        warn!("TG_BOT_TOKEN / TG_CHANNEL_ID not set: dry-run, posts will only be logged");
    }
    let erc20 = erc20::Erc20::new(env("RPC_URL").unwrap_or_else(|| DEFAULT_RPC.into()));

    info!(
        host = url.split('?').next().unwrap_or_default(),
        "starting grad_tokens"
    );

    let (grad_tx, grad_rx) = mpsc::channel(256);
    let (post_tx, post_rx) = mpsc::channel(256);
    let handler = handler::Handler::new(post_tx, erc20, dexscreener::DexScreener::new());

    // `grad_tokens --test-post 0xToken`: render and send one post through the real path, then exit.
    if let Some(token) = std::env::args().skip_while(|a| a != "--test-post").nth(1) {
        let sender = tokio::spawn(tg::run(tg, post_rx));
        handler.test_post(token).await;
        sender.await?;
        return Ok(());
    }

    tokio::spawn(feed::run(url, grad_tx));
    tokio::spawn(tg::run(tg, post_rx));
    handler.run(grad_rx).await;
    Ok(())
}
