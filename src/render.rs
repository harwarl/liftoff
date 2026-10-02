pub const EXPLORER: &str = "https://rh-scan.com";
pub const PONS: &str = "https://www.ponsfamily.com/launchpad";
pub const FOMO: &str = "https://fomo.family/tokens/robinhood";

pub struct Info<'a> {
    pub token: &'a str,
    pub name: Option<&'a str>,
    pub symbol: Option<&'a str>,
    pub market_cap: Option<f64>,
    pub chart_url: &'a str,
}

pub struct Rendered {
    pub text: String,
    /// One row of inline URL buttons: (label, url).
    pub buttons: Vec<(String, String)>,
}

pub fn message(i: &Info) -> Rendered {
    let name = i.name.filter(|s| !s.is_empty());
    let symbol = i.symbol.filter(|s| !s.is_empty());

    let mut text = "🎓 <b>GRADUATED · PONS</b>\n".to_string();
    let title = match (symbol, name) {
        (Some(s), Some(n)) if n != s => Some(format!("<b>${}</b> — {}", esc(&truncate(s, 32)), esc(&truncate(n, 64)))),
        (Some(s), _) => Some(format!("<b>${}</b>", esc(&truncate(s, 32)))),
        (None, Some(n)) => Some(format!("<b>{}</b>", esc(&truncate(n, 64)))),
        (None, None) => None,
    };
    if let Some(t) = title {
        text += &format!("{t}\n");
    }
    // Pons graduates when the bonding curve is drained (`createGraduatedPool`, §8.1).
    text += "Rule: curve exhausted\n";
    text += &format!("CA: <code>{}</code>\n", esc(i.token));
    if let Some(mc) = i.market_cap {
        text += &format!("MC: <b>{}</b>\n", usd(mc));
    }

    let buttons = vec![
        ("📈 Chart".to_string(), i.chart_url.to_string()),
        ("Pons".to_string(), format!("{PONS}/{}", i.token)),
        ("Fomo".to_string(), format!("{FOMO}/{}", i.token)),
    ];
    Rendered { text: text.trim_end().to_string(), buttons }
}

/// `$950`, `$4.21k`, `$123k`, `$1.5m`, `$2b`.
fn usd(v: f64) -> String {
    let (n, suffix) = match v {
        v if v >= 1e9 => (v / 1e9, "b"),
        v if v >= 1e6 => (v / 1e6, "m"),
        v if v >= 1e3 => (v / 1e3, "k"),
        v => return format!("${v:.0}"),
    };
    // 3 significant figures: 4.21k, 42.1k, 123k.
    let decimals = if n >= 100.0 { 0 } else if n >= 10.0 { 1 } else { 2 };
    let s = format!("{n:.decimals$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { &s };
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
        let r = message(&Info {
            token: "0xabc",
            name: Some("Reso <Coin>"),
            symbol: Some("RESO"),
            market_cap: Some(123_456.0),
            chart_url: CHART,
        });
        assert_eq!(
            r.text,
            "🎓 <b>GRADUATED · PONS</b>\n<b>$RESO</b> — Reso &lt;Coin&gt;\nRule: curve exhausted\n\
             CA: <code>0xabc</code>\nMC: <b>$123k</b>"
        );
        assert_eq!(
            r.buttons,
            vec![
                ("📈 Chart".to_string(), CHART.to_string()),
                ("Pons".to_string(), "https://www.ponsfamily.com/launchpad/0xabc".to_string()),
                ("Fomo".to_string(), "https://fomo.family/tokens/robinhood/0xabc".to_string()),
            ]
        );
    }

    #[test]
    fn same_name_no_mc() {
        let r = message(&Info { token: "0xabc", name: Some("DUCK"), symbol: Some("DUCK"), market_cap: None, chart_url: CHART });
        assert_eq!(r.text, "🎓 <b>GRADUATED · PONS</b>\n<b>$DUCK</b>\nRule: curve exhausted\nCA: <code>0xabc</code>");
    }

    #[test]
    fn bare() {
        let r = message(&Info { token: "0xabc", name: None, symbol: None, market_cap: None, chart_url: CHART });
        assert_eq!(r.text, "🎓 <b>GRADUATED · PONS</b>\nRule: curve exhausted\nCA: <code>0xabc</code>");
    }

    #[test]
    fn usd_format() {
        assert_eq!(usd(950.4), "$950");
        assert_eq!(usd(4207.0), "$4.21k");
        assert_eq!(usd(42_100.0), "$42.1k");
        assert_eq!(usd(123_456.0), "$123k");
        assert_eq!(usd(1_500_000.0), "$1.5m");
        assert_eq!(usd(2_000_000_000.0), "$2b");
    }
}
