//! One-shot `name()` / `symbol()` reads for graduations with no cached metadata.

use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

const NAME: &str = "0x06fdde03";
const SYMBOL: &str = "0x95d89b41";

#[derive(Clone)]
pub struct Erc20 {
    client: reqwest::Client,
    rpc_url: String,
}

impl Erc20 {
    pub fn new(rpc_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("http client");
        Self { client, rpc_url }
    }

    pub async fn name_symbol(&self, token: &str) -> (Option<String>, Option<String>) {
        let (name, symbol) = tokio::join!(self.call_string(token, NAME), self.call_string(token, SYMBOL));
        (name.ok().flatten(), symbol.ok().flatten())
    }

    async fn call_string(&self, token: &str, selector: &str) -> Result<Option<String>> {
        let body = json!({
            "jsonrpc": "2.0", "id": 1, "method": "eth_call",
            "params": [{ "to": token, "data": selector }, "latest"],
        });
        let v: Value = self.client.post(&self.rpc_url).json(&body).send().await?.json().await?;
        let hex = v["result"].as_str().ok_or_else(|| anyhow!("eth_call: {}", v["error"]))?;
        Ok(decode_string(&hex_bytes(hex)?))
    }
}

fn hex_bytes(s: &str) -> Result<Vec<u8>> {
    let s = s.trim_start_matches("0x");
    (0..s.len() / 2 * 2)
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(Into::into))
        .collect()
}

/// ABI `string`, or the legacy `bytes32` form some tokens return.
fn decode_string(b: &[u8]) -> Option<String> {
    let word = |at: usize| -> Option<usize> {
        let w = b.get(at..at + 32)?;
        if w[..24].iter().any(|&x| x != 0) {
            return None;
        }
        Some(u64::from_be_bytes(w[24..].try_into().ok()?) as usize)
    };
    let raw = if b.len() == 32 {
        let end = b.iter().position(|&x| x == 0).unwrap_or(32);
        &b[..end]
    } else {
        let off = word(0)?;
        let len = word(off)?;
        b.get(off + 32..off.checked_add(32)?.checked_add(len)?)?
    };
    let s = String::from_utf8_lossy(raw).trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_string() {
        let hex = "0x0000000000000000000000000000000000000000000000000000000000000020\
                   0000000000000000000000000000000000000000000000000000000000000004\
                   5245534f00000000000000000000000000000000000000000000000000000000";
        assert_eq!(decode_string(&hex_bytes(hex).unwrap()).as_deref(), Some("RESO"));
    }

    #[test]
    fn bytes32() {
        let hex = "0x5245534f00000000000000000000000000000000000000000000000000000000";
        assert_eq!(decode_string(&hex_bytes(hex).unwrap()).as_deref(), Some("RESO"));
    }
}
