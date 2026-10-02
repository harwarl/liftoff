use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::{interval, sleep, timeout, Instant, MissedTickBehavior};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::event::{self, Frame, Graduation};

const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
const LIMIT_BACKOFF: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// We ping this often so a quiet chain still produces traffic (the pong).
const PING_EVERY: Duration = Duration::from_secs(20);
/// No frame at all (pongs included) for this long means the socket is dead.
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// A session that lived this long counts as healthy and resets the backoff.
const HEALTHY_SESSION: Duration = Duration::from_secs(60);

const SUBSCRIBE: &str = r#"{"method":"subscribeNewToken"}"#;
/// Cap refusals: the server closes with 1013 right after sending one of these.
const LIMIT_CODES: [&str; 3] = ["CONNECTION_LIMIT", "SERVER_AT_CAPACITY", "DEMO_LIMIT"];

#[derive(Default)]
struct SessionEnd {
    throttled: bool,
}

pub async fn run(url: String, tx: mpsc::Sender<Graduation>) {
    let mut backoff = MIN_BACKOFF;
    loop {
        let started = Instant::now();
        let end = session(&url, &tx).await;
        if tx.is_closed() {
            return;
        }
        if started.elapsed() >= HEALTHY_SESSION {
            backoff = MIN_BACKOFF;
        }
        let wait = if end.throttled {
            LIMIT_BACKOFF
        } else {
            let w = backoff;
            backoff = (backoff * 2).min(MAX_BACKOFF);
            w
        };
        info!(wait_s = wait.as_secs(), "reconnecting");
        sleep(wait).await;
    }
}

async fn session(url: &str, tx: &mpsc::Sender<Graduation>) -> SessionEnd {
    let mut end = SessionEnd::default();

    let ws = match timeout(CONNECT_TIMEOUT, tokio_tungstenite::connect_async(url)).await {
        Ok(Ok((ws, _))) => ws,
        Ok(Err(e)) => {
            warn!(error = %e, "ws connect failed");
            return end;
        }
        Err(_) => {
            warn!("ws connect timed out");
            return end;
        }
    };
    info!("ws connected");
    let (mut write, mut read) = ws.split();

    // Subscribe on open, as in the docs' quick start; the server acks with `subscribed`.
    if let Err(e) = write.send(Message::Text(SUBSCRIBE.into())).await {
        warn!(error = %e, "subscribe send failed");
        return end;
    }

    let mut ping = interval(PING_EVERY);
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    ping.tick().await; // first tick fires immediately
    let mut last_frame = Instant::now();

    loop {
        let msg = tokio::select! {
            m = read.next() => m,
            _ = ping.tick() => {
                if last_frame.elapsed() >= IDLE_TIMEOUT {
                    warn!("no frames for {}s; forcing reconnect", IDLE_TIMEOUT.as_secs());
                    return end;
                }
                if let Err(e) = write.send(Message::Ping(Vec::new())).await {
                    warn!(error = %e, "ping failed");
                    return end;
                }
                continue;
            }
        };

        let msg = match msg {
            None => {
                warn!("ws stream ended");
                return end;
            }
            Some(Err(e)) => {
                warn!(error = %e, "ws read error");
                return end;
            }
            Some(Ok(m)) => m,
        };
        last_frame = Instant::now();

        let text = match msg {
            Message::Text(t) => t,
            Message::Close(frame) => {
                if let Some(f) = &frame {
                    end.throttled |= f.code == CloseCode::Again;
                }
                warn!(?frame, "ws closed by server");
                return end;
            }
            // Pongs (and server pings, which tungstenite answers) only count as activity.
            _ => continue,
        };

        let frame = match event::parse(&text) {
            Ok(f) => f,
            Err(e) => {
                debug!(error = %e, frame = %text, "unparseable frame");
                continue;
            }
        };

        match frame {
            Frame::Subscribed => info!("subscribed to new tokens"),
            Frame::ConnectionStatus(v) => info!(status = %v, "connectionStatus"),
            Frame::Error(c) => {
                let code = c.code.as_deref().unwrap_or("?");
                if LIMIT_CODES.contains(&code) {
                    end.throttled = true;
                }
                warn!(code, detail = c.message.as_deref().unwrap_or_default(), "upstream error");
            }
            Frame::Notice(c) => match c.code.as_deref() {
                Some("NO_API_KEY") => {
                    warn!("PumpDev says no API key: launches still stream, but metered per IP")
                }
                code => info!(?code, detail = c.message.as_deref().unwrap_or_default(), "upstream notice"),
            },
            Frame::Account(kind, v) => info!(%v, "{kind} frame"),
            Frame::Graduation(g) => {
                if tx.send(g).await.is_err() {
                    return end;
                }
            }
            Frame::Ignored => {}
        }
    }
}
