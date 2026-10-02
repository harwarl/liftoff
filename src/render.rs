pub const EXPLORER: &str = "https://rh-scan.com";
pub const FOMO: &str = "https://fomo.family/tokens/robinhood";

pub struct Info<'a> {
    pub token: &'a str,
    pub name: Option<&'a str>,
    pub symbol: Option<&'a str>,
    pub market_cap: Option<f64>,
    pub chart_url: &'a str,
}

pub fn message(i: &Info) -> String {
    let name = i.name.filter(|s| !s.is_empty());
    let symbol = i.symbol.filter(|s| !s.is_empty());

    let mut out = match symbol {
        Some(s) => format!("🎓 <b>${}</b> graduated\n", esc(&truncate(s, 32))),
        None => "🎓 <b>Token</b> graduated\n".to_string(),
    };
    if let Some(n) = name.filter(|n| Some(*n) != symbol) {
        out += &format!("{}\n", esc(&truncate(n, 64)));
    }
    out += &format!("<code>{}</code>\n", esc(i.token));
    if let Some(mc) = i.market_cap {
        out += &format!("\nMC: <b>{}</b>\n", usd(mc));
    }
    let t = esc(i.token);
    out += &format!(
        "\n<a href=\"{}\">📈 Chart</a> · <a href=\"{FOMO}/{t}\">Fomo</a> · <a href=\"{EXPLORER}/token/{t}\">rh-scan ↗</a>",
        esc(i.chart_url)
    );
    out
}

/// `$950`, `$4.21K`, `$1.5M`, `$2.04B`.
fn usd(v: f64) -> String {
    let (n, suffix) = match v {
        v if v >= 1e9 => (v / 1e9, "B"),
        v if v >= 1e6 => (v / 1e6, "M"),
        v if v >= 1e3 => (v / 1e3, "K"),
        v => return format!("${v:.0}"),
    };
    let s = format!("{n:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("${s}{suffix}")
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max - 1).chain(['…']).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHART: &str = "https://dexscreener.com/robinhood/0xpair";

    #[test]
    fn full() {
        let i = Info {
            token: "0xabc",
            name: Some("Reso <Coin>"),
            symbol: Some("RESO"),
            market_cap: Some(4207.0),
            chart_url: CHART,
        };
        assert_eq!(
            message(&i),
            "🎓 <b>$RESO</b> graduated\nReso &lt;Coin&gt;\n<code>0xabc</code>\n\nMC: <b>$4.21K</b>\n\n\
             <a href=\"https://dexscreener.com/robinhood/0xpair\">📈 Chart</a> · \
             <a href=\"https://fomo.family/tokens/robinhood/0xabc\">Fomo</a> · \
             <a href=\"https://rh-scan.com/token/0xabc\">rh-scan ↗</a>"
        );
    }

    #[test]
    fn bare() {
        let i = Info { token: "0xabc", name: None, symbol: None, market_cap: None, chart_url: CHART };
        assert_eq!(
            message(&i),
            "🎓 <b>Token</b> graduated\n<code>0xabc</code>\n\n\
             <a href=\"https://dexscreener.com/robinhood/0xpair\">📈 Chart</a> · \
             <a href=\"https://fomo.family/tokens/robinhood/0xabc\">Fomo</a> · \
             <a href=\"https://rh-scan.com/token/0xabc\">rh-scan ↗</a>"
        );
    }

    #[test]
    fn usd_format() {
        assert_eq!(usd(950.4), "$950");
        assert_eq!(usd(4207.0), "$4.21K");
        assert_eq!(usd(1_500_000.0), "$1.5M");
        assert_eq!(usd(2_000_000_000.0), "$2B");
    }
}
