//! Market cap + chart link from DexScreener's public API (no key, 300 req/min).

use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;
use tokio::time::sleep;
use tracing::debug;

const API: &str = "https://api.dexscreener.com/tokens/v1/robinhood";
pub const CHART: &str = "https://dexscreener.com/robinhood";
/// The pool is created by the graduation tx itself, so DexScreener may need a few
/// seconds to index it. Total wait is the sum of these before posting without a cap.
const RETRY_DELAYS: [u64; 5] = [3, 5, 7, 10, 15];

#[derive(Debug, Default)]
pub struct Market {
    pub market_cap: Option<f64>,
    pub chart_url: Option<String>,
    pub name: Option<String>,
    pub symbol: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pair {
    url: Option<String>,
    market_cap: Option<f64>,
    fdv: Option<f64>,
    liquidity: Option<Liquidity>,
    base_token: Option<BaseToken>,
}

#[derive(Deserialize)]
struct Liquidity {
    usd: Option<f64>,
}

#[derive(Deserialize)]
struct BaseToken {
    address: Option<String>,
    name: Option<String>,
    symbol: Option<String>,
}

#[derive(Clone)]
pub struct DexScreener {
    client: reqwest::Client,
}

impl DexScreener {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("http client");
        Self { client }
    }

    /// Polls until the pair is indexed with a market cap, or the retries run out.
    pub async fn market(&self, token: &str) -> Market {
        let mut best = Market::default();
        for (i, delay) in std::iter::once(0).chain(RETRY_DELAYS).enumerate() {
            sleep(Duration::from_secs(delay)).await;
            match self.fetch(token).await {
                Ok(m) if m.market_cap.is_some() => return m,
                Ok(m) => best = m,
                Err(e) => debug!(%token, attempt = i, error = %e, "dexscreener lookup failed"),
            }
        }
        best
    }

    async fn fetch(&self, token: &str) -> Result<Market> {
        let pairs: Vec<Pair> = self
            .client
            .get(format!("{API}/{token}"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        // Only pairs where our token is the base, deepest liquidity first.
        let best = pairs
            .into_iter()
            .filter(|p| {
                p.base_token
                    .as_ref()
                    .and_then(|b| b.address.as_deref())
                    .is_some_and(|a| a.eq_ignore_ascii_case(token))
            })
            .max_by(|a, b| liq(a).total_cmp(&liq(b)));

        Ok(match best {
            Some(p) => Market {
                market_cap: p.market_cap.or(p.fdv).filter(|v| *v > 0.0),
                chart_url: p.url,
                name: p.base_token.as_ref().and_then(|b| b.name.clone()),
                symbol: p.base_token.and_then(|b| b.symbol),
            },
            None => Market::default(),
        })
    }
}

fn liq(p: &Pair) -> f64 {
    p.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0)
}
