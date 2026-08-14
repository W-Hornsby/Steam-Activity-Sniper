use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use eframe::egui;
use serde::Deserialize;
use tiny_http::{Header, Method, Response, Server, StatusCode};

use crate::db::{Database, GameData};

pub const PORT: u16 = 8765;

pub enum Event {
    Hello(String),
    Callback(CallbackPayload),
    Log(String),
}

#[derive(Clone, Debug, Deserialize)]
pub struct CallbackPayload {
    #[serde(default)]
    pub page: String,
    pub url: String,
    pub time: i64,
    #[serde(default)]
    pub manual: bool,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub persona: Option<String>,
    #[serde(default)]
    pub two_weeks_total: Option<f64>,
    #[serde(default)]
    pub games: Vec<RawGame>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RawGame {
    #[serde(default)]
    pub appid: Option<u32>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub hours_2weeks: Option<f64>,
    #[serde(default)]
    pub hours_record: Option<f64>,
}

pub fn user_key_from_url(url: &str) -> Option<String> {
    let idx = url.find("steamcommunity.com")?;
    let rest = &url[idx + "steamcommunity.com".len()..];
    let seg = if let Some(r) = rest.strip_prefix("/id/") {
        r
    } else if let Some(r) = rest.strip_prefix("/profiles/") {
        r
    } else {
        return None;
    };
    let end = seg
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(seg.len());
    let key = seg[..end].to_string();
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

fn cors_headers() -> Vec<Header> {
    let mut out = Vec::new();
    for (name, value) in [
        ("Access-Control-Allow-Origin", "*"),
        ("Access-Control-Allow-Headers", "Content-Type"),
        ("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
    ] {
        if let Ok(h) = Header::from_bytes(name.as_bytes(), value.as_bytes()) {
            out.push(h);
        }
    }
    out
}

fn respond(request: tiny_http::Request, status: u16, body: &str) {
    let mut response = Response::from_string(body.to_string())
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes(b"Content-Type", b"application/json").unwrap());
    for h in cors_headers() {
        response.add_header(h);
    }
    let _ = request.respond(response);
}

fn handle(
    db: &Arc<Mutex<Database>>,
    tx: &Sender<Event>,
    ctx: &egui::Context,
    mut request: tiny_http::Request,
) {
    let url_path = request.url().to_string();
    let method = request.method().clone();

    match method {
        Method::Options => respond(request, 200, "{}"),
        Method::Get => {
            if url_path == "/status" {
                respond(request, 200, r#"{"state":"running"}"#);
            } else {
                respond(request, 404, r#"{"error":"not found"}"#);
            }
        }
        Method::Post => {
            let mut body = String::new();
            let _ = request.as_reader().read_to_string(&mut body);
            if url_path == "/hello" {
                let _ = tx.send(Event::Hello(body.clone()));
                ctx.request_repaint();
                respond(request, 200, r#"{"ok":true}"#);
            } else if url_path == "/callback" {
                match serde_json::from_str::<CallbackPayload>(&body) {
                    Ok(payload) => {
                        if !payload.disabled {
                            if let Some(key) = user_key_from_url(&payload.url) {
                                let games: Vec<GameData> = payload
                                    .games
                                    .iter()
                                    .map(|g| GameData {
                                        appid: g.appid,
                                        name: g.name.clone().unwrap_or_default(),
                                        hours_2weeks: g.hours_2weeks,
                                        hours_record: g.hours_record,
                                    })
                                    .collect();
                                let mut db_guard = db.lock().unwrap();
                                db_guard.apply_callback(
                                    &key,
                                    &payload.url,
                                    payload.persona.as_deref(),
                                    &games,
                                    payload.two_weeks_total,
                                    payload.time,
                                );
                                let _ = db_guard.save();
                            }
                        }
                        let _ = tx.send(Event::Callback(payload));
                        ctx.request_repaint();
                        respond(request, 200, r#"{"ok":true}"#);
                    }
                    Err(e) => {
                        let _ = tx.send(Event::Log(format!("Bad callback payload: {e}")));
                        respond(request, 400, r#"{"error":"bad payload"}"#);
                    }
                }
            } else {
                respond(request, 404, r#"{"error":"not found"}"#);
            }
        }
        _ => respond(request, 405, r#"{"error":"method not allowed"}"#),
    }
}

pub fn run(db: Arc<Mutex<Database>>, tx: Sender<Event>, ctx: egui::Context) {
    let server = match Server::http(format!("127.0.0.1:{PORT}")) {
        Ok(s) => s,
        Err(e) => {
            let _ = tx.send(Event::Log(format!(
                "Failed to bind 127.0.0.1:{PORT}: {e} (is another instance running?)"
            )));
            return;
        }
    };
    let _ = tx.send(Event::Log(format!(
        "Listener running on http://127.0.0.1:{PORT}"
    )));
    for request in server.incoming_requests() {
        handle(&db, &tx, &ctx, request);
    }
}

#[cfg(test)]
mod tests {
    use super::user_key_from_url;

    #[test]
    fn extracts_numeric_profile() {
        assert_eq!(
            user_key_from_url("https://steamcommunity.com/profiles/76561198077713381/"),
            Some("76561198077713381".to_string())
        );
    }

    #[test]
    fn extracts_custom_url() {
        assert_eq!(
            user_key_from_url("https://steamcommunity.com/id/gaben/"),
            Some("gaben".to_string())
        );
    }

    #[test]
    fn ignores_query_and_fragment() {
        assert_eq!(
            user_key_from_url("https://steamcommunity.com/id/name?tab=games#top"),
            Some("name".to_string())
        );
    }

    #[test]
    fn rejects_non_profile_pages() {
        assert_eq!(user_key_from_url("https://steamcommunity.com/market/"), None);
        assert_eq!(user_key_from_url("https://google.com/profiles/x/"), None);
    }
}
