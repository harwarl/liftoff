use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Graduation {
    pub token: Option<String>,
    pub block: Option<Value>,
    pub tx_hash: Option<String>,
}

/// `error` / `notice` / `auth` / `quota` frames. Docs: switch on `code`, show `message`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Control {
    pub code: Option<String>,
    pub message: Option<String>,
}

/// A parsed WS text frame.
pub enum Frame {
    Graduation(Graduation),
    Subscribed,
    ConnectionStatus(Value),
    Error(Control),
    Notice(Control),
    /// `auth` (tier changed) and `quota` (trade allowance spent); informational only.
    Account(&'static str, Value),
    /// `connected`, `newToken`, `tokenMeta`, `tokenTrade` and anything unknown.
    Ignored,
}

pub fn parse(text: &str) -> anyhow::Result<Frame> {
    let v: Value = serde_json::from_str(text)?;
    let kind = v.get("type").and_then(Value::as_str).unwrap_or_default();
    Ok(match kind {
        "graduation" => Frame::Graduation(serde_json::from_value(v)?),
        "subscribed" => Frame::Subscribed,
        "connectionStatus" => Frame::ConnectionStatus(v),
        "error" => Frame::Error(serde_json::from_value(v)?),
        "notice" => Frame::Notice(serde_json::from_value(v)?),
        "auth" => Frame::Account("auth", v),
        "quota" => Frame::Account("quota", v),
        _ => Frame::Ignored,
    })
}
