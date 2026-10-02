# Grad_tokens — Pons Graduation Bot

A Telegram bot that watches the Pons launchpad on Robinhood Chain and posts **only graduated tokens** to a channel. It is a short-lived bot, so it is built to be simple rather than bulletproof.

---

## 1. Scope

**In**

- Listen for Pons graduations via PumpDev's WebSocket.
- Post one message per graduated token to a configured channel.

**Out**

- Storing tokens. Nothing is persisted: no database, no launch-metadata cache.
- Trading, sniping, price tracking.
- Own RPC listener / on-chain backstop.

---

## 2. Data source

|              |                                                                            |
| ------------ | -------------------------------------------------------------------------- |
| Feed         | PumpDev PONS WebSocket                                                     |
| Endpoint     | `wss://rhc.pumpdev.io/ws?key=<PUMPDEV_KEY>`                                |
| Subscription | `{"method":"subscribeNewToken"}`                                           |
| Delivers     | `newToken`, `graduation`, `tokenMeta`                                      |
| Cost         | Free and unmetered (trades are the only metered stream; we don't use them) |
| Chain        | Robinhood Chain, ID 4663, explorer rh-scan.com                             |
| RPC          | `https://rpc.mainnet.chain.robinhood.com` (public, keyless, rate limited)  |

The key is optional for these events, but use one: keyless connections are metered per IP and can be closed with `DEMO_LIMIT`.

### Events we use

**`graduation`**: confirmed only, and bare:

```json
{
  "type": "graduation",
  "status": "confirmed",
  "token": "0x...",
  "block": 1234567,
  "txHash": "0x..."
}
```

### Events we ignore

`newToken`, `tokenMeta`, `tokenTrade`, plus control frames other than those listed in §7. `subscribeNewToken` is still the subscription, because it's the one that delivers `graduation`.

---

## 3. Flow

```
 PumpDev WS ─ graduation ─► seen.insert(token)? else drop
                            spawn: name/symbol = eth_call name(), symbol()   ┐ in parallel,
                                   market cap + chart = DexScreener          ┘ best effort
                                   render ─► Telegram queue
```

- Dedup on **token address**, in memory only, for the life of the process. A reconnect may replay, and a token only graduates once. After a restart the set is empty; PumpDev has no replay, so this is fine.
- Lookups happen at post time, each graduation in its own task so one slow lookup can't delay another.
- DexScreener (`https://api.dexscreener.com/tokens/v1/robinhood/<token>`, no key): the graduation tx creates the pool, so indexing can lag. Retry at +3/+5/+7/+10/+15s (~40s max), then post without market cap. Market cap is `marketCap`, else `fdv`, from the deepest-liquidity pair where the token is base.
- Name/symbol: RPC first, DexScreener's `baseToken` as fallback; neither → address alone.

---

## 4. Storage

None. The only in-memory state is the dedup set above.

---

## 5. Message

Telegram `sendMessage`, `parse_mode=HTML` (less escaping than MarkdownV2), link previews off.

```
🎓 <b>$SYMBOL</b> graduated
Name                  ← omitted if same as symbol or unknown
<code>0xTokenAddress</code>

MC: <b>$4.21K</b>      ← omitted if DexScreener has no cap yet

📈 Chart · Fomo · rh-scan ↗
```

- No name/symbol from RPC: header becomes `🎓 <b>Token</b> graduated`.
- Wording is "graduated": `graduation` fires on pool creation (§8.1).
- Links: DexScreener pair page (falls back to `dexscreener.com/robinhood/<token>`), Fomo (`fomo.family/tokens/robinhood/<token>`), rh-scan token page. Add a Pons token page link once the URL pattern is confirmed.

---

## 6. Telegram setup

1. Create the bot with @BotFather and copy the token.
2. Add the bot to the channel as **admin** with "Post messages".
3. Get the channel ID (`-100…`): forward a channel post to @userinfobot, or call `getUpdates` after posting.
4. Graduations are low-volume, but send through a single queue with ~3s spacing to stay well under per-chat limits. On `429`, sleep for `retry_after`.

---

## 7. Connection handling

- **Connect**: open the socket, wait for `connected`, send `subscribeNewToken`, expect a `subscribed` ack.
- **Reconnect**: exponential backoff (1s → 30s cap), resubscribe on every reconnect.
- **Keepalive**: if nothing (including control frames) arrives for 60s, force a reconnect.
- **Control frames**:
  - `connectionStatus`: log it. Tells a quiet chain apart from a dead upstream.
  - `error` with `CONNECTION_LIMIT` / `SERVER_AT_CAPACITY` / `DEMO_LIMIT`: socket closes with 1013; back off longer (60s).
  - `notice` / `NO_API_KEY`: warn that the key is missing.
- **Gap**: events during downtime are lost; PumpDev has no replay. Acceptable for this bot.

---

## 8. Verify before going live

1. **Which phase `graduation` is.** Run the listener for a while, take 2–3 graduation `txHash` values and open them on rh-scan.com.
   - Shows `LaunchSwept` / `CurveCompleted` → curve drained, pool not live yet.
   - Shows `PoolGraduated` / `PoolRegistered` → pool live and tradeable.
   - **Checked 2026-10-01 (1 sample):** tx `0x7aa03e…b5b5` calls `createGraduatedPool(address)` (`0x2f53ef2f`), so `graduation` fires on pool creation and the pool is live. Wording stays "graduated" (`GRADUATION_ON_SWEEP=false`); no second "pool live" message is needed.
2. ~~**Metadata hit rate.**~~ Dropped with the cache.
3. **Pons URL pattern** for token pages, for the link in §5.

---

## 9. Stack

| Concern   | Crate                                                                   |
| --------- | ----------------------------------------------------------------------- |
| Runtime   | `tokio`                                                                 |
| WebSocket | `tokio-tungstenite` (rustls)                                            |
| JSON      | `serde`, `serde_json`                                                   |
| HTTP      | `reqwest` (rustls): Telegram `sendMessage`, RPC `eth_call`, DexScreener |
| Config    | `dotenvy`                                                               |
| Logs      | `tracing`, `tracing-subscriber`                                         |

### Layout

```
grad_tokens/
├── Cargo.toml
├── .env.example
└── src/
    ├── main.rs      # wiring, config, task spawn
    ├── feed.rs      # WS connect/reconnect, frame → Graduation
    ├── event.rs     # serde type for graduation + control frames
    ├── handler.rs   # in-memory dedup, name/symbol lookup, render
    ├── erc20.rs     # name()/symbol() via eth_call
    ├── dexscreener.rs # market cap + chart URL
    ├── render.rs    # → HTML message
    └── tg.rs        # send queue, 3s spacing, 429 handling
```

Tasks: `feed` → `mpsc<Graduation>` → `handler` (dedup + lookup) → `mpsc<Post>` → `tg` sender.

### Config (`.env`)

```
PUMPDEV_KEY=
TG_BOT_TOKEN=
TG_CHANNEL_ID=-100...
RPC_URL=                     # optional, defaults to the public RPC
RUST_LOG=info
```

---

## 10. Deploy

- Any small always-on VPS. One binary, no state on disk.
- Run under `systemd` with `Restart=always`, or in Docker with `restart: unless-stopped`.
- Logs to journald. Watch for repeated reconnects or long `connectionStatus` outages.

---

## 11. Open decisions

- Include every quote asset (ETH, USDG, tokenized stocks), or only ETH pairs?
- Any filters (creator blacklist, minimum dev buy) or post every graduation?
- ~~Post a second "pool live" message, if §8.1 shows `graduation` fires on the sweep?~~ Not needed: it fires on pool creation (§8.1).
