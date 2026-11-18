// The two local WhatsApp gateways Coucou can talk to (port of WhatsAppProvider.swift). Each call
// goes to the URL set in Settings (127.0.0.1 by default) and nowhere else: no redirect, no proxy,
// no cookie, every answer read with a ceiling. Nothing of a message is logged.

use std::time::Duration;

use reqwest::{Method, Url};
use serde_json::{json, Value};

use super::gateway::{self, gowa, openwa, Kind, Link, Status};
use super::model::{self, Message};

/// Ceiling for one answer of the gateway (a QR PNG is about 2 KB, a page of messages a few).
const MAX_ANSWER: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaError {
    /// Nothing answers at that URL.
    Unreachable,
    /// Wrong user:pass (GOWA) or API key (OpenWA).
    Unauthorized,
    /// The gateway answered with an error (its own message, never a message's content).
    Refused(String),
    /// WhatsApp isn't linked yet.
    NotLoggedIn,
}

/// Percent-encodes one path segment (letters, digits and `-._~@` kept).
pub fn path_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~@".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The gateway's address as typed: http(s), a host, no `user:pass@`, nothing after the path.
pub fn parse_base(raw: &str) -> Option<Url> {
    let url = Url::parse(raw.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return None;
    }
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    Some(url)
}

/// `base`'s path + `path`, the query added; None when the result left the gateway.
pub fn endpoint(base: &Url, path: &str, query: &[(&str, String)]) -> Option<Url> {
    let mut url = base.clone();
    let prefix = base.path().trim_matches('/');
    let full = if prefix.is_empty() { path.to_string() } else { format!("/{prefix}{path}") };
    url.set_path(&full);
    url.set_query(None);
    url.set_fragment(None);
    if !query.is_empty() {
        let mut pairs = url.query_pairs_mut();
        for (k, v) in query {
            pairs.append_pair(k, v);
        }
    }
    (url.host_str() == base.host_str() && url.port_or_known_default() == base.port_or_known_default()).then_some(url)
}

/// Convert GOWA filesystem paths to its served media route, including Windows paths.
fn media_route(raw: &str) -> Option<String> {
    let normalized = raw.replace('\\', "/");
    if normalized.contains(['%', '?', '#']) || normalized.chars().any(char::is_control)
        || normalized.split('/').any(|p| p == "..") { return None; }
    let path = normalized.strip_prefix("./").unwrap_or(&normalized).trim_start_matches('/');
    let route = if path.starts_with("statics/media/") { path }
        else { let i = path.rfind("/statics/media/")?; &path[i+1..] };
    (!route.ends_with('/') && !route.split('/').any(|p| p == ".")).then(|| format!("/{route}"))
}

#[cfg(test)] mod media_route_tests {
    use super::*;
    #[test] fn windows_and_relative_gateway_paths() {
        for input in [r"statics\media\user\photo.jpg", r"C:\GOWA\statics\media\user\photo.jpg", "./statics/media/user/photo.jpg", "/opt/gowa/statics/media/user/photo.jpg"] {
            assert_eq!(media_route(input).as_deref(), Some("/statics/media/user/photo.jpg"));
        }
    }
    #[test] fn rejects_traversal_and_non_media_paths() {
        for input in ["statics/media/../secret", "statics/media/%2e%2e/secret", "C:/Windows/secret", "statics/media/a?token=secret", "statics/media/"] {
            assert!(media_route(input).is_none());
        }
    }
}

/// GOWA deliberately serves non-image attachments as application/octet-stream.
/// Recover only known media signatures, never trust a filename as a media type.
fn media_mime(header: &str, bytes: &[u8]) -> Option<&'static str> {
    let mime = header.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let declared = match mime.as_str() {
        "image/jpeg" => Some("image/jpeg"), "image/png" => Some("image/png"),
        "image/webp" => Some("image/webp"), "image/gif" => Some("image/gif"),
        "audio/ogg" | "application/ogg" => Some("audio/ogg"),
        "audio/mpeg" => Some("audio/mpeg"), "audio/mp4" => Some("audio/mp4"),
        "audio/wav" => Some("audio/wav"), "audio/webm" => Some("audio/webm"),
        "video/mp4" => Some("video/mp4"), "video/webm" => Some("video/webm"),
        "video/ogg" => Some("video/ogg"), _ => None,
    };
    if declared.is_some() { return declared; }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { return Some("image/png"); }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) { return Some("image/jpeg"); }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") { return Some("image/gif"); }
    if bytes.starts_with(b"RIFF") {
        match bytes.get(8..12) {
            Some(b"WEBP") => return Some("image/webp"),
            Some(b"WAVE") => return Some("audio/wav"), _ => {},
        }
    }
    // ISO BMFF: verify the complete ftyp box and its MP4 brands. HEIC/AVIF also
    // use ftyp, so the marker alone is not sufficient to identify a video.
    if bytes.len() >= 16 && bytes.get(4..8) == Some(b"ftyp") {
        let size = u32::from_be_bytes(bytes[..4].try_into().ok()?) as usize;
        if (16..=4096).contains(&size) && size <= bytes.len() && size % 4 == 0 {
            if matches!(bytes.get(8..12), Some(b"M4A ") | Some(b"M4B ")) { return Some("audio/mp4"); }
            for brand in bytes[8..size].chunks_exact(4).enumerate().filter_map(|(i, b)| (i != 1).then_some(b)) {
                if matches!(brand, b"isom" | b"mp41" | b"mp42" | b"avc1" | b"M4V " | b"MSNV" | b"dash")
                    || (brand.starts_with(b"iso") && brand[3].is_ascii_digit()) {
                    return Some("video/mp4");
                }
            }
        }
    }
    let head = &bytes[..bytes.len().min(4096)];
    if head.starts_with(b"\x1a\x45\xdf\xa3") && head.windows(7).any(|w| w == b"\x42\x82\x84webm") {
        return Some("video/webm");
    }
    if head.starts_with(b"OggS") {
        if head.windows(7).any(|w| w == b"\x80theora") { return Some("video/ogg"); }
        if head.windows(8).any(|w| w == b"OpusHead") || head.windows(7).any(|w| w == b"\x01vorbis") {
            return Some("audio/ogg");
        }
    }
    if bytes.len() >= 10 && bytes.starts_with(b"ID3") { return Some("audio/mpeg"); }
    if bytes.len() >= 4 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0
        && bytes[1] & 0x06 != 0 && bytes[2] & 0xf0 != 0xf0 && bytes[2] & 0xf0 != 0 {
        return Some("audio/mpeg");
    }
    None
}

/// HTTP to the gateway: never a redirect, never a proxy, nothing kept between calls.
pub struct Http {
    base: Url,
    headers: Vec<(&'static str, String)>,
    client: reqwest::Client,
}

impl Http {
    pub fn new(base: Url, headers: Vec<(&'static str, String)>) -> Result<Http, WaError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| WaError::Unreachable)?;
        Ok(Http { base, headers, client })
    }

    pub async fn call(&self, method: Method, path: &str, query: &[(&str, String)], body: Option<Value>, timeout: Duration) -> Result<(Vec<u8>, u16), WaError> {
        let url = endpoint(&self.base, path, query).ok_or(WaError::Unreachable)?;
        let mut req = self.client.request(method, url.clone()).timeout(timeout).header("Accept", "application/json");
        for (k, v) in &self.headers {
            req = req.header(*k, v);
        }
        if let Some(body) = body {
            req = req.header("Content-Type", "application/json").body(body.to_string());
        }
        let response = req.send().await.map_err(|_| WaError::Unreachable)?;
        // Second lock: an answer from anywhere but the gateway is not one.
        if response.url().host_str() != url.host_str() || response.url().port_or_known_default() != url.port_or_known_default() {
            return Err(WaError::Unreachable);
        }
        let code = response.status().as_u16();
        let data = crate::net::read_capped(response, MAX_ANSWER).await.map_err(|_| WaError::Unreachable)?;
        Ok((data, code))
    }

    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<(Vec<u8>, u16), WaError> {
        self.call(Method::GET, path, query, None, Duration::from_secs(10)).await
    }
}

fn basic_auth(credential: &str) -> String {
    format!("Basic {}", gateway::base64(credential.as_bytes()))
}

/// Either gateway, behind one face.
pub enum Provider {
    Gowa { http: Http, device: tokio::sync::Mutex<Option<String>> },
    Openwa { http: Http, session: tokio::sync::Mutex<Option<String>> },
}

/// A QR to show: a `data:image/png;base64,…` URL and how long it stays valid.
pub struct Qr {
    pub data_url: String,
    pub seconds: u64,
}

impl Provider {
    pub fn new(kind: Kind, base: Url, credential: Option<String>) -> Result<Provider, WaError> {
        let credential = credential.filter(|c| !c.is_empty());
        Ok(match kind {
            Kind::Gowa => {
                let headers = credential.filter(|c| c.contains(':')).map(|c| vec![("Authorization", basic_auth(&c))]).unwrap_or_default();
                Provider::Gowa { http: Http::new(base, headers)?, device: Default::default() }
            }
            Kind::Openwa => {
                let headers = credential.map(|c| vec![("X-API-Key", c)]).unwrap_or_default();
                Provider::Openwa { http: Http::new(base, headers)?, session: Default::default() }
            }
        })
    }

    // ── GOWA ──

    fn gowa_check(data: &[u8], code: u16) -> Result<(), WaError> {
        if code == 401 && gowa::error_code(data).as_deref() != Some("AUTHENTICATION_ERROR") {
            return Err(WaError::Unauthorized);
        }
        if code == 401 {
            return Err(WaError::NotLoggedIn);
        }
        if !(200..300).contains(&code) {
            return Err(WaError::Refused(gowa::error_code(data).unwrap_or_else(|| format!("HTTP {code}"))));
        }
        Ok(())
    }

    async fn gowa_device(http: &Http, slot: &tokio::sync::Mutex<Option<String>>) -> Result<(String, Status), WaError> {
        let (data, code) = http.get("/devices", &[]).await?;
        Self::gowa_check(&data, code)?;
        if let Some((id, status)) = gowa::device(&data) {
            *slot.lock().await = Some(id.clone());
            return Ok((id, status));
        }
        let (made, made_code) = http.call(Method::POST, "/devices", &[], Some(json!({"device_id": "coucou"})), Duration::from_secs(10)).await?;
        Self::gowa_check(&made, made_code)?;
        *slot.lock().await = Some("coucou".into());
        Ok(("coucou".into(), Status { link: Link::NeedsQr, me: None }))
    }

    async fn gowa_id(http: &Http, slot: &tokio::sync::Mutex<Option<String>>) -> Result<String, WaError> {
        if let Some(id) = slot.lock().await.clone() {
            return Ok(id);
        }
        Ok(Self::gowa_device(http, slot).await?.0)
    }

    async fn gowa_scoped(http: &Http, slot: &tokio::sync::Mutex<Option<String>>, method: Method, path: &str, query: &[(&str, String)], body: Option<Value>) -> Result<Vec<u8>, WaError> {
        let id = Self::gowa_id(http, slot).await?;
        let mut q: Vec<(&str, String)> = query.to_vec();
        q.push(("device_id", id));
        let (data, code) = http.call(method, path, &q, body, Duration::from_secs(10)).await?;
        Self::gowa_check(&data, code)?;
        Ok(data)
    }

    // ── OpenWA ──

    fn openwa_check(data: &[u8], code: u16) -> Result<(), WaError> {
        if code == 401 || code == 403 {
            return Err(WaError::Unauthorized);
        }
        if !(200..300).contains(&code) {
            return Err(WaError::Refused(openwa::error_message(data).unwrap_or_else(|| format!("HTTP {code}"))));
        }
        Ok(())
    }

    async fn openwa_start(http: &Http, id: &str, fallback: openwa::Session) -> Result<openwa::Session, WaError> {
        let (started, code) = http.call(Method::POST, &format!("/api/sessions/{}/start", path_component(id)), &[], None, Duration::from_secs(10)).await?;
        Self::openwa_check(&started, code)?;
        Ok(serde_json::from_slice::<Value>(&started).ok().as_ref().and_then(openwa::session).unwrap_or(fallback))
    }

    async fn openwa_session(http: &Http, slot: &tokio::sync::Mutex<Option<String>>) -> Result<openwa::Session, WaError> {
        let (data, code) = http.get("/api/sessions", &[]).await?;
        Self::openwa_check(&data, code)?;
        let all = openwa::sessions(&data);
        let known = slot.lock().await.clone();
        let pick = all
            .iter()
            .find(|s| s.status == "ready")
            .or_else(|| all.iter().find(|s| Some(&s.id) == known.as_ref()))
            .or_else(|| all.iter().find(|s| s.name == "coucou"))
            .or(all.first())
            .cloned();
        if let Some(s) = pick {
            *slot.lock().await = Some(s.id.clone());
            if openwa::needs_start(&s.status) {
                return Self::openwa_start(http, &s.id.clone(), s).await;
            }
            return Ok(s);
        }
        let (made, made_code) = http.call(Method::POST, "/api/sessions", &[], Some(json!({"name": "coucou"})), Duration::from_secs(10)).await?;
        Self::openwa_check(&made, made_code)?;
        let s = serde_json::from_slice::<Value>(&made)
            .ok()
            .as_ref()
            .and_then(openwa::session)
            .ok_or_else(|| WaError::Refused("session".into()))?;
        *slot.lock().await = Some(s.id.clone());
        Self::openwa_start(http, &s.id.clone(), s).await
    }

    async fn openwa_id(http: &Http, slot: &tokio::sync::Mutex<Option<String>>) -> Result<String, WaError> {
        if let Some(id) = slot.lock().await.clone() {
            return Ok(id);
        }
        Ok(Self::openwa_session(http, slot).await?.id)
    }

    async fn openwa_scoped(http: &Http, slot: &tokio::sync::Mutex<Option<String>>, method: Method, path: &str, query: &[(&str, String)], body: Option<Value>) -> Result<Vec<u8>, WaError> {
        let id = Self::openwa_id(http, slot).await?;
        let (data, code) = http.call(method, &format!("/api/sessions/{}{path}", path_component(&id)), query, body, Duration::from_secs(10)).await?;
        Self::openwa_check(&data, code)?;
        Ok(data)
    }

    // ── The face ──

    /// The link to WhatsApp (sets up the gateway's device or session the first time).
    pub async fn status(&self) -> Result<Status, WaError> {
        match self {
            Provider::Gowa { http, device } => Ok(Self::gowa_device(http, device).await?.1),
            Provider::Openwa { http, session } => Ok(openwa::status(&Self::openwa_session(http, session).await?)),
        }
    }

    /// The QR to scan; None when there is none to show (already linked, not ready yet).
    pub async fn qr(&self) -> Result<Option<Qr>, WaError> {
        match self {
            Provider::Gowa { http, device } => {
                let id = Self::gowa_id(http, device).await?;
                // GOWA answers once WhatsApp has handed it the first code: a few seconds.
                let (data, code) = http.call(Method::GET, &format!("/devices/{}/login", path_component(&id)), &[], None, Duration::from_secs(45)).await?;
                if gowa::error_code(&data).as_deref() == Some("ALREADY_LOGGED_IN") {
                    return Ok(None);
                }
                Self::gowa_check(&data, code)?;
                let Some((path, seconds)) = gowa::qr_path(&data) else { return Ok(None) };
                // The PNG is served by the gateway itself: fetched from the configured URL,
                // whatever host the link names.
                let (png, png_code) = http.get(&path, &[]).await?;
                if png_code != 200 || !png.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                    return Ok(None);
                }
                // qr_duration is half the code's life: asking again sooner restarts the pairing.
                Ok(Some(Qr { data_url: gateway::png_data_url(&png), seconds: (seconds * 2).saturating_sub(5).max(20) }))
            }
            Provider::Openwa { http, session } => {
                let id = Self::openwa_id(http, session).await?;
                let (data, code) = http.get(&format!("/api/sessions/{}/qr", path_component(&id)), &[]).await?;
                if code == 400 {
                    return Ok(None); // not started, already linked, or not ready yet
                }
                Self::openwa_check(&data, code)?;
                Ok(openwa::qr_data_url(&data).map(|data_url| Qr { data_url, seconds: 15 }))
            }
        }
    }

    /// Tells the gateway to post new messages to Coucou's local webhook, signed with `secret`.
    pub async fn register_webhook(&self, url: &str, secret: &str) -> Result<(), WaError> {
        match self {
            Provider::Gowa { http, device } => {
                let id = Self::gowa_id(http, device).await?;
                let body = json!({"webhook_url": url, "webhook_secret": secret, "webhook_events": "message"});
                let (data, code) = http.call(Method::PATCH, &format!("/devices/{}/webhook", path_component(&id)), &[], Some(body), Duration::from_secs(10)).await?;
                Self::gowa_check(&data, code)
            }
            Provider::Openwa { http, session } => {
                let id = path_component(&Self::openwa_id(http, session).await?);
                let body = json!({"url": url, "events": ["message.received", "message.sent"], "secret": secret});
                let (list, list_code) = http.get(&format!("/api/sessions/{id}/webhooks"), &[]).await?;
                Self::openwa_check(&list, list_code)?;
                let hooks: Vec<Value> = serde_json::from_slice(&list).unwrap_or_default();
                let mine = hooks.iter().find(|h| h.get("url").and_then(Value::as_str) == Some(url)).and_then(|h| h.get("id")).and_then(Value::as_str);
                let (data, code) = match mine {
                    Some(hook) => http.call(Method::PUT, &format!("/api/sessions/{id}/webhooks/{}", path_component(hook)), &[], Some(body), Duration::from_secs(10)).await?,
                    None => http.call(Method::POST, &format!("/api/sessions/{id}/webhooks"), &[], Some(body), Duration::from_secs(10)).await?,
                };
                Self::openwa_check(&data, code)
            }
        }
    }

    /// Messages of every chat since `since_ms` (the polling fallback), oldest first.
    pub async fn recent_messages(&self, since_ms: i64, me: Option<&str>) -> Result<Vec<Message>, WaError> {
        let mut out = match self {
            Provider::Gowa { http, device } => {
                let chats = gowa::chats(&Self::gowa_scoped(http, device, Method::GET, "/chats", &[("limit", "20".into())], None).await?);
                let mut out = Vec::new();
                // Only the chats that moved since then (newest first from the gateway).
                for (jid, name, _) in chats.into_iter().take(8).filter(|c| c.2 > since_ms) {
                    let path = format!("/chat/{}/messages", path_component(&jid));
                    let data = Self::gowa_scoped(http, device, Method::GET, &path, &[("limit", "10".into()), ("start_time", gateway::iso_utc(since_ms))], None).await?;
                    out.extend(gowa::messages(&data, me).into_iter().map(|mut m| {
                        if m.chat_name.is_none() && !name.is_empty() && model::is_group(&jid) {
                            m.chat_name = Some(name.clone());
                        }
                        m
                    }));
                }
                out
            }
            Provider::Openwa { http, session } => {
                let q = [("since", since_ms.to_string()), ("limit", "50".into()), ("inlineMedia", "false".into())];
                openwa::messages(&Self::openwa_scoped(http, session, Method::GET, "/messages", &q, None).await?, me)
            }
        };
        out.sort_by_key(|m| m.date);
        Ok(out)
    }

    /// Download on explicit request. Only the configured GOWA origin may serve bytes.
    pub async fn media(&self, chat: &str, message: &str) -> Result<String, WaError> {
        let Provider::Gowa { http, device } = self else {
            return Err(WaError::Refused("Media viewing currently requires GOWA".into()));
        };
        let id = Self::gowa_id(http, device).await?;
        let (meta, code) = http.call(Method::GET,
            &format!("/message/{}/download", path_component(message)),
            &[("phone", chat.into()), ("device_id", id)], None, Duration::from_secs(45)).await?;
        Self::gowa_check(&meta, code)?;
        let value: Value = serde_json::from_slice(&meta).map_err(|_| WaError::Unreachable)?;
        let result = &value["results"];
        let raw = result["file_path"].as_str().unwrap_or("");
        // Filesystem paths are mapped to a fixed route on the configured gateway only.
        let path = media_route(raw).ok_or_else(|| WaError::Refused("Unsupported media path".into()))?;
        let url = endpoint(&http.base, &path, &[]).ok_or(WaError::Unreachable)?;
        let mut req = http.client.get(url).timeout(Duration::from_secs(45));
        for (key, value) in &http.headers { req = req.header(*key, value); }
        let response = req.send().await.map_err(|_| WaError::Unreachable)?;
        if !response.status().is_success() { return Err(WaError::Unreachable); }
        let declared = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_owned();
        let bytes = crate::net::read_capped(response, 24 * 1024 * 1024).await.map_err(|_| WaError::Unreachable)?;
        let mime = media_mime(&declared, &bytes).ok_or_else(|| WaError::Refused("Unsupported media format".into()))?;
        Ok(format!("data:{mime};base64,{}", gateway::base64(&bytes)))
    }

    /// The latest `limit` messages of one chat, oldest first.
    pub async fn messages(&self, chat: &str, limit: usize, me: Option<&str>) -> Result<Vec<Message>, WaError> {
        let mut out = match self {
            Provider::Gowa { http, device } => {
                let path = format!("/chat/{}/messages", path_component(chat));
                gowa::messages(&Self::gowa_scoped(http, device, Method::GET, &path, &[("limit", limit.to_string())], None).await?, me)
            }
            Provider::Openwa { http, session } => {
                let q = [("chatId", openwa::chat_id(chat)), ("limit", limit.to_string()), ("inlineMedia", "false".into())];
                openwa::messages(&Self::openwa_scoped(http, session, Method::GET, "/messages", &q, None).await?, me)
            }
        };
        out.sort_by_key(|m| m.date);
        Ok(out)
    }

    /// Sends a text to an existing chat; the new message's id when the gateway gives it.
    pub async fn send(&self, text: &str, chat: &str) -> Result<Option<String>, WaError> {
        match self {
            Provider::Gowa { http, device } => {
                let data = Self::gowa_scoped(http, device, Method::POST, "/send/message", &[], Some(json!({"phone": chat, "message": text}))).await?;
                Ok(gowa::sent_id(&data))
            }
            Provider::Openwa { http, session } => {
                let data = Self::openwa_scoped(http, session, Method::POST, "/messages/send-text", &[], Some(json!({"chatId": openwa::chat_id(chat), "text": text}))).await?;
                Ok(serde_json::from_slice::<Value>(&data).ok().and_then(|v| v.get("messageId").and_then(Value::as_str).map(str::to_string)))
            }
        }
    }

    /// Marks the chat read up to `last_incoming` (the newest message received in it).
    pub async fn mark_read(&self, chat: &str, last_incoming: Option<&str>) -> Result<(), WaError> {
        match self {
            Provider::Gowa { http, device } => {
                let Some(id) = last_incoming else { return Ok(()) };
                let path = format!("/message/{}/read", path_component(id));
                Self::gowa_scoped(http, device, Method::POST, &path, &[], Some(json!({"phone": chat}))).await.map(|_| ())
            }
            Provider::Openwa { http, session } => {
                Self::openwa_scoped(http, session, Method::POST, "/chats/read", &[], Some(json!({"chatId": openwa::chat_id(chat)}))).await.map(|_| ())
            }
        }
    }

    /// A group's name, when the messages don't carry it.
    pub async fn group_name(&self, chat: &str) -> Option<String> {
        if !model::is_group(chat) {
            return None;
        }
        match self {
            Provider::Gowa { http, device } => {
                let data = Self::gowa_scoped(http, device, Method::GET, "/group/info", &[("group_id", chat.to_string())], None).await.ok()?;
                gowa::group_name(&data)
            }
            Provider::Openwa { http, session } => {
                let data = Self::openwa_scoped(http, session, Method::GET, &format!("/groups/{}", path_component(chat)), &[], None).await.ok()?;
                let v: Value = serde_json::from_slice(&data).ok()?;
                v.get("name").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_stay_on_the_gateway() {
        assert_eq!(path_component("1203@g.us"), "1203@g.us");
        assert_eq!(path_component("a/b?c#d e"), "a%2Fb%3Fc%23d%20e");
        assert_eq!(path_component("../x"), "..%2Fx");
        let base = parse_base("http://127.0.0.1:3000").unwrap();
        assert_eq!(endpoint(&base, "/devices", &[]).unwrap().as_str(), "http://127.0.0.1:3000/devices");
        let q = endpoint(&base, "/chats", &[("limit", "20".into()), ("device_id", "a&b=c".into())]).unwrap();
        assert_eq!(q.as_str(), "http://127.0.0.1:3000/chats?limit=20&device_id=a%26b%3Dc");
        let prefixed = parse_base("http://127.0.0.1:3000/gowa/").unwrap();
        assert_eq!(endpoint(&prefixed, "/devices", &[]).unwrap().as_str(), "http://127.0.0.1:3000/gowa/devices");
        assert!(parse_base("ftp://127.0.0.1").is_none());
        assert!(parse_base("http://user:pass@127.0.0.1:3000").is_none());
        assert!(parse_base("not a url").is_none());
        assert_eq!(basic_auth("user:pass"), "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn credentials_go_where_each_gateway_wants_them() {
        let base = parse_base("http://127.0.0.1:3000").unwrap();
        let Provider::Gowa { http, .. } = Provider::new(Kind::Gowa, base.clone(), Some("admin:secret".into())).unwrap() else { panic!() };
        assert_eq!(http.headers, vec![("Authorization", "Basic YWRtaW46c2VjcmV0".to_string())]);
        // A GOWA credential without "user:" is not basic auth: nothing is sent.
        let Provider::Gowa { http, .. } = Provider::new(Kind::Gowa, base.clone(), Some("justakey".into())).unwrap() else { panic!() };
        assert!(http.headers.is_empty());
        let Provider::Openwa { http, .. } = Provider::new(Kind::Openwa, base, Some("k-123".into())).unwrap() else { panic!() };
        assert_eq!(http.headers, vec![("X-API-Key", "k-123".to_string())]);
    }

    #[test]
    fn errors_are_read_like_the_mac() {
        assert_eq!(Provider::gowa_check(br#"{"code":"x"}"#, 401), Err(WaError::Unauthorized));
        assert_eq!(Provider::gowa_check(br#"{"code":"AUTHENTICATION_ERROR"}"#, 401), Err(WaError::NotLoggedIn));
        assert_eq!(Provider::gowa_check(br#"{"code":"INVALID_JID"}"#, 400), Err(WaError::Refused("INVALID_JID".into())));
        assert_eq!(Provider::gowa_check(b"", 500), Err(WaError::Refused("HTTP 500".into())));
        assert_eq!(Provider::gowa_check(b"", 200), Ok(()));
        assert_eq!(Provider::openwa_check(b"", 403), Err(WaError::Unauthorized));
        assert_eq!(Provider::openwa_check(br#"{"message":"Session not found"}"#, 404), Err(WaError::Refused("Session not found".into())));
    }
}

#[cfg(test)]
mod media_format_tests {
    use super::*;

    const MP4: &[u8] = b"\0\0\0\x18ftypisom\0\0\x02\0isommp42";

    #[test]
    fn gowa_binary_video_and_voice_attachments_get_playable_types() {
        assert_eq!(media_mime("application/octet-stream", MP4), Some("video/mp4"));
        assert_eq!(media_mime("video/x-m4v", MP4), Some("video/mp4"));
        assert_eq!(media_mime("", b"\x1a\x45\xdf\xa3\x87\x42\x82\x84webm"), Some("video/webm"));
        assert_eq!(media_mime("application/octet-stream", b"OggS\0voice-packet:OpusHead"), Some("audio/ogg"));
        assert_eq!(media_mime(" Video/MP4; charset=binary ", MP4), Some("video/mp4"));
        assert_eq!(media_mime("audio/mp4", MP4), Some("audio/mp4"));
    }

    #[test]
    fn binary_fallback_rejects_documents_and_incomplete_or_image_ftyp_boxes() {
        for bytes in [b"<html>not a video</html>".as_slice(), b"%PDF-1.7", b"", &MP4[..12],
            b"\0\0\0\x18ftypavif\0\0\0\0avifmif1", b"\0\0\xff\xffftypisom\0\0\0\0"] {
            assert_eq!(media_mime("application/octet-stream", bytes), None);
        }
    }

    #[tokio::test]
    async fn download_maps_windows_path_and_sniffs_binary_video_from_gateway() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move {
            for (expected, content_type, body) in [
                ("/message/VIDEO1/download?", "application/json", br#"{"code":"SUCCESS","results":{"file_path":"statics\\media\\video.mp4"}}"#.as_slice()),
                ("/statics/media/video.mp4 ", "application/octet-stream", MP4),
            ] {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buf = [0; 1024];
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&buf[..n]);
                    if request.windows(4).any(|w| w == b"\r\n\r\n") { break; }
                }
                assert!(String::from_utf8_lossy(&request).contains(expected));
                let header = format!("HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                socket.write_all(header.as_bytes()).await.unwrap();
                socket.write_all(body).await.unwrap();
            }
        });
        let provider = Provider::Gowa { http: Http::new(base, vec![]).unwrap(), device: tokio::sync::Mutex::new(Some("test-device".into())) };
        let result = tokio::time::timeout(Duration::from_secs(5), provider.media("test@s.whatsapp.net", "VIDEO1")).await.unwrap().unwrap();
        assert_eq!(result, format!("data:video/mp4;base64,{}", gateway::base64(MP4)));
        server.await.unwrap();
    }
}
