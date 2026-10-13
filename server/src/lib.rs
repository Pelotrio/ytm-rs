//! ytm-rs Jam relay. One Durable Object per room relays small JSON control
//! messages between a host and its guests; audio never passes through here.
//! See README.md for the protocol.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use worker::*;

const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
const MAX_MEMBERS: usize = 16;
const MAX_FRAME: usize = 256 * 1024;
const GRACE_MS: i64 = 5 * 60 * 1000;
const BURST: f64 = 20.0;
const RATE_PER_SEC: f64 = 5.0;

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    if url.path() == "/" {
        return Response::ok("ytm-rs jam server");
    }
    let Some(raw) = url.path().strip_prefix("/jam/") else {
        return Response::error("not found", 404);
    };
    let code = raw.to_ascii_uppercase();
    if code.len() != 6 || !code.bytes().all(|b| ALPHABET.contains(&b)) {
        return Response::error("bad room code", 400);
    }
    let upgrade = req.headers().get("Upgrade")?.unwrap_or_default();
    if !upgrade.eq_ignore_ascii_case("websocket") {
        return Response::error("expected a websocket upgrade", 426);
    }
    let stub = env.durable_object("ROOMS")?.id_from_name(&code)?.get_stub()?;
    stub.fetch_with_request(req).await
}

/// Room-wide settings, kept in storage under "meta".
#[derive(Serialize, Deserialize, Default)]
struct Meta {
    host: String,
    control: bool,
    /// Join counter, so "earliest joined" is unambiguous.
    seq: u32,
    #[serde(default)]
    empty_since: Option<f64>,
}

/// Per-socket data, kept in the socket attachment so it survives hibernation.
#[derive(Serialize, Deserialize)]
struct Member {
    id: String,
    name: String,
    joined: u32,
    // token bucket for rate limiting
    tokens: f64,
    last: f64,
    /// Set once we've processed this socket's departure.
    #[serde(default)]
    gone: bool,
}

#[durable_object]
pub struct Room {
    state: State,
}

impl DurableObject for Room {
    fn new(state: State, _env: Env) -> Self {
        // Keepalives are answered by the runtime without waking us up.
        if let Ok(pair) = WebSocketRequestResponsePair::new("ping", "pong") {
            state.set_websocket_auto_response(&pair);
        }
        Self { state }
    }

    async fn fetch(&self, req: Request) -> Result<Response> {
        let url = req.url()?;
        let param = |k: &str| url.query_pairs().find(|(n, _)| n == k).map(|(_, v)| v.into_owned());
        let create = param("create");
        let pair = WebSocketPair::new()?;
        if !matches!(create.as_deref(), Some("0" | "1")) || param("v").is_some_and(|v| v != "1") {
            return self.reject(pair, "bad", "expected create=0|1 and v=1");
        }

        let storage = self.state.storage();
        let now = now();
        let members = self.members(None);
        let mut meta = match storage.get::<Meta>("meta").await? {
            _ if create.as_deref() == Some("1") => {
                if !members.is_empty() {
                    return self.reject(pair, "taken", "that room code is in use");
                }
                // Nobody is here, so whatever an abandoned room left behind can go.
                storage.delete_all().await?;
                Meta::default()
            }
            Some(m) => {
                let expired = members.is_empty()
                    && m.empty_since.is_some_and(|t| now - t >= GRACE_MS as f64);
                if expired {
                    return self.reject(pair, "not_found", "no such room");
                }
                if members.len() >= MAX_MEMBERS {
                    return self.reject(pair, "full", "the room is full");
                }
                m
            }
            None => return self.reject(pair, "not_found", "no such room"),
        };

        meta.seq += 1;
        let me = Member {
            id: random_id(),
            name: clean_name(&param("name").unwrap_or_default()),
            joined: meta.seq,
            tokens: BURST,
            last: now,
            gone: false,
        };
        // First one in (or back into an emptied room) hosts.
        if members.is_empty() {
            meta.host = me.id.clone();
        }
        meta.empty_since = None;
        storage.put("meta", &meta).await?;
        storage.delete_alarm().await?;

        let ws = pair.server;
        self.state.accept_web_socket(&ws);
        ws.serialize_attachment(&me)?;

        let mut list = member_list(&members);
        list.push(json!({ "id": me.id, "name": me.name }));
        let stored = |s: Option<String>| s.and_then(|s| serde_json::from_str::<Value>(&s).ok());
        send(&ws, &json!({
            "t": "welcome", "you": me.id, "host": meta.host, "control": meta.control,
            "members": list, "now": now as u64,
            "state": stored(storage.get("state").await?),
            "tick": stored(storage.get("tick").await?),
        }));
        // The newcomer already has the list in its welcome.
        let update = json!({ "t": "members", "host": meta.host, "members": list });
        for (other, _) in &members {
            send(other, &update);
        }
        Response::from_websocket(pair.client)
    }

    async fn websocket_message(&self, ws: WebSocket, msg: WebSocketIncomingMessage) -> Result<()> {
        let Some(mut me) = ws.deserialize_attachment::<Member>()? else { return Ok(()) };
        if me.gone {
            return Ok(());
        }
        let now = now();
        me.tokens = (me.tokens + (now - me.last) / 1000.0 * RATE_PER_SEC).min(BURST);
        me.last = now;
        let limited = me.tokens < 1.0;
        if !limited {
            me.tokens -= 1.0;
        }
        ws.serialize_attachment(&me)?;
        if limited {
            return error(&ws, "rate", "slow down");
        }

        let WebSocketIncomingMessage::String(text) = msg else {
            return error(&ws, "bad", "binary frames are not supported");
        };
        if text.len() > MAX_FRAME {
            return error(&ws, "bad", "frame too large");
        }
        let Ok(Value::Object(mut m)) = serde_json::from_str::<Value>(&text) else {
            return error(&ws, "bad", "expected a JSON object");
        };
        let t = m.remove("t").and_then(|t| t.as_str().map(String::from)).unwrap_or_default();

        let storage = self.state.storage();
        let Some(mut meta) = storage.get::<Meta>("meta").await? else { return Ok(()) };
        let is_host = meta.host == me.id;
        if matches!(t.as_str(), "state" | "tick" | "control" | "end") && !is_host {
            return error(&ws, "denied", "only the host can do that");
        }

        match t.as_str() {
            "ping" => send(&ws, &json!({ "t": "pong", "c": m.get("c"), "s": now as u64 })),
            "state" => {
                if !valid_state(&m) {
                    return error(&ws, "bad", "malformed state");
                }
                m.insert("ts".into(), (now as u64).into());
                // A full state also resets the playhead late joiners see.
                let tick = json!({ "vid": m["vid"], "pos": m["pos"], "playing": m["playing"], "ts": now as u64 });
                storage.put("state", serde_json::to_string(&m)?).await?;
                storage.put("tick", tick.to_string()).await?;
                m.insert("t".into(), "state".into());
                self.broadcast(&Value::Object(m), Some(&ws));
            }
            "tick" => {
                if !valid_tick(&m) {
                    return error(&ws, "bad", "malformed tick");
                }
                m.insert("ts".into(), (now as u64).into());
                storage.put("tick", serde_json::to_string(&m)?).await?;
                m.insert("t".into(), "tick".into());
                self.broadcast(&Value::Object(m), Some(&ws));
            }
            "control" => {
                let Some(on) = m.get("on").and_then(Value::as_bool) else {
                    return error(&ws, "bad", "control needs on: bool");
                };
                meta.control = on;
                storage.put("meta", &meta).await?;
                self.broadcast(&json!({ "t": "control", "on": on }), None);
            }
            "end" => {
                storage.delete_alarm().await?;
                storage.delete_all().await?;
                for (other, mut member) in self.members(None) {
                    member.gone = true;
                    let _ = other.serialize_attachment(&member);
                    send(&other, &json!({ "t": "ended" }));
                    let _ = other.close(Some(1000), Some("ended"));
                }
            }
            "req" => {
                let cmd = m.remove("cmd").unwrap_or(Value::Null);
                let Some(kind) = valid_cmd(&cmd) else {
                    return error(&ws, "bad", "malformed req");
                };
                let gated = matches!(kind, "toggle" | "skip" | "prev" | "seek" | "jump");
                if gated && !meta.control && !is_host {
                    return error(&ws, "denied", "the host has guest control off");
                }
                let fwd = json!({ "t": "req", "from": me.id, "name": me.name, "cmd": cmd });
                if let Some((host, _)) = self.members(None).iter().find(|(_, x)| x.id == meta.host) {
                    send(host, &fwd);
                }
            }
            "bye" => {
                self.leave(&ws).await?;
                let _ = ws.close(Some(1000), Some("bye"));
            }
            _ => return error(&ws, "bad", "unknown message type"),
        }
        Ok(())
    }

    async fn websocket_close(&self, ws: WebSocket, _code: usize, _reason: String, _clean: bool) -> Result<()> {
        self.leave(&ws).await?;
        // Complete the close handshake; harmless if the runtime already did.
        let _ = ws.close(Some(1000), Some("bye"));
        Ok(())
    }

    async fn websocket_error(&self, ws: WebSocket, _error: Error) -> Result<()> {
        self.leave(&ws).await
    }

    async fn alarm(&self) -> Result<Response> {
        // Nobody came back during the grace period: forget the room.
        if self.members(None).is_empty() {
            self.state.storage().delete_all().await?;
        }
        Response::ok("ok")
    }
}

impl Room {
    /// Refuse a connection: accept it just long enough to say why, then close.
    /// Hibernation-accepted with no attachment, so it never counts as a member
    /// and the runtime cleans up quietly if the client drops mid-close.
    fn reject(&self, pair: WebSocketPair, code: &str, msg: &str) -> Result<Response> {
        let close_code = match code {
            "not_found" => 4004,
            "taken" => 4009,
            "full" => 4029,
            _ => 4000,
        };
        self.state.accept_web_socket(&pair.server);
        send(&pair.server, &json!({ "t": "error", "code": code, "msg": msg }));
        pair.server.close(Some(close_code), Some(code))?;
        Response::from_websocket(pair.client)
    }

    /// Live members in join order, optionally leaving one socket out.
    fn members(&self, skip: Option<&WebSocket>) -> Vec<(WebSocket, Member)> {
        let mut out: Vec<_> = self
            .state
            .get_websockets()
            .into_iter()
            .filter(|w| Some(w) != skip)
            .filter_map(|w| {
                let m = w.deserialize_attachment::<Member>().ok().flatten()?;
                (!m.gone).then_some((w, m))
            })
            .collect();
        out.sort_by_key(|(_, m)| m.joined);
        out
    }

    fn broadcast(&self, msg: &Value, skip: Option<&WebSocket>) {
        for (ws, _) in self.members(skip) {
            send(&ws, msg);
        }
    }

    /// Runs once per socket, whether it said bye, closed, or errored.
    async fn leave(&self, ws: &WebSocket) -> Result<()> {
        let Some(mut me) = ws.deserialize_attachment::<Member>()? else { return Ok(()) };
        if me.gone {
            return Ok(());
        }
        me.gone = true;
        let _ = ws.serialize_attachment(&me);

        let storage = self.state.storage();
        let Some(mut meta) = storage.get::<Meta>("meta").await? else { return Ok(()) };
        let rest = self.members(Some(ws));
        if rest.is_empty() {
            meta.empty_since = Some(now());
            storage.put("meta", &meta).await?;
            storage.set_alarm(GRACE_MS).await?;
            return Ok(());
        }
        if meta.host == me.id {
            meta.host = rest[0].1.id.clone();
            storage.put("meta", &meta).await?;
        }
        let update = json!({ "t": "members", "host": meta.host, "members": member_list(&rest) });
        for (other, _) in &rest {
            send(other, &update);
        }
        Ok(())
    }
}

fn now() -> f64 {
    Date::now().as_millis() as f64
}

fn send(ws: &WebSocket, msg: &Value) {
    // A socket that's mid-close can refuse sends; that's fine.
    let _ = ws.send_with_str(msg.to_string());
}

fn error(ws: &WebSocket, code: &str, msg: &str) -> Result<()> {
    send(ws, &json!({ "t": "error", "code": code, "msg": msg }));
    Ok(())
}

fn member_list(members: &[(WebSocket, Member)]) -> Vec<Value> {
    members.iter().map(|(_, m)| json!({ "id": m.id, "name": m.name })).collect()
}

fn random_id() -> String {
    (0..8)
        .map(|_| ALPHABET[(js_sys::Math::random() * ALPHABET.len() as f64) as usize] as char)
        .collect()
}

/// Trimmed, control characters stripped, at most 32 chars, "Guest" if empty.
fn clean_name(raw: &str) -> String {
    let name: String = raw.chars().filter(|c| !c.is_control()).collect();
    let name: String = name.trim().chars().take(32).collect();
    match name.trim_end() {
        "" => "Guest".into(),
        n => n.into(),
    }
}

fn is_num(v: Option<&Value>) -> bool {
    v.is_some_and(Value::is_number)
}

fn valid_tick(m: &Map<String, Value>) -> bool {
    m.get("vid").is_some_and(Value::is_string)
        && is_num(m.get("pos"))
        && m.get("playing").is_some_and(Value::is_boolean)
}

fn valid_state(m: &Map<String, Value>) -> bool {
    valid_tick(m)
        && m.get("queue").and_then(Value::as_array).is_some_and(|q| q.iter().all(Value::is_object))
        && is_num(m.get("index"))
        && m.get("by").is_some_and(Value::is_object)
}

/// Returns the command kind if the command is well formed.
fn valid_cmd(cmd: &Value) -> Option<&str> {
    let kind = cmd.get("k")?.as_str()?;
    let ok = match kind {
        "add" | "next" => cmd.get("track").is_some_and(Value::is_object),
        "seek" => is_num(cmd.get("pos")),
        "jump" => is_num(cmd.get("index")),
        "toggle" | "skip" | "prev" => true,
        _ => false,
    };
    ok.then_some(kind)
}
