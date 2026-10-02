use std::collections::HashSet;

use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::dexscreener::{self, DexScreener};
use crate::erc20::Erc20;
use crate::event::Graduation;
use crate::render;
use crate::tg::Post;

pub struct Handler {
    posts: mpsc::Sender<Post>,
    erc20: Erc20,
    dex: DexScreener,
    /// Addresses already handled this run, so a reconnect replay can't double-post.
    seen: HashSet<String>,
}

impl Handler {
    pub fn new(posts: mpsc::Sender<Post>, erc20: Erc20, dex: DexScreener) -> Self {
        Self { posts, erc20, dex, seen: HashSet::new() }
    }

    pub async fn run(mut self, mut rx: mpsc::Receiver<Graduation>) {
        while let Some(g) = rx.recv().await {
            let Some(token) = g.token.as_deref().map(|t| t.trim().to_ascii_lowercase()) else { continue };
            if !self.seen.insert(token.clone()) {
                debug!(%token, "duplicate graduation dropped");
                continue;
            }
            // Own task: the DexScreener wait for one token must not hold up the next.
            tokio::spawn(prepare(token, g, self.erc20.clone(), self.dex.clone(), self.posts.clone()));
        }
    }

    /// Runs one token through the same path as a live graduation; drops the sender when done.
    pub async fn test_post(self, token: String) {
        let token = token.trim().to_ascii_lowercase();
        prepare(token, Graduation::default(), self.erc20, self.dex, self.posts).await;
    }
}

async fn prepare(token: String, g: Graduation, erc20: Erc20, dex: DexScreener, posts: mpsc::Sender<Post>) {
    let ((name, symbol), market) = tokio::join!(erc20.name_symbol(&token), dex.market(&token));
    let name = name.or(market.name);
    let symbol = symbol.or(market.symbol);
    let chart_url = market.chart_url.unwrap_or_else(|| format!("{}/{token}", dexscreener::CHART));

    info!(
        %token,
        ?symbol,
        market_cap = ?market.market_cap,
        block = ?g.block,
        tx = %g.tx_hash.as_deref().map(|t| format!("{}/tx/{t}", render::EXPLORER)).unwrap_or_default(),
        "graduation"
    );

    let text = render::message(&render::Info {
        token: &token,
        name: name.as_deref(),
        symbol: symbol.as_deref(),
        market_cap: market.market_cap,
        chart_url: &chart_url,
    });
    if posts.send(Post { token, text }).await.is_err() {
        warn!("telegram sender is gone");
    }
}
