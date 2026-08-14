use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const DB_FILE: &str = "steam_activity.json";
const MAX_HISTORY: usize = 20_000;
const DEDUP_WINDOW_MS: i64 = 10 * 60 * 1000;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GamePoint {
    pub ts: i64,
    #[serde(default)]
    pub hours_2weeks: Option<f64>,
    pub hours_record: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Game {
    pub appid: Option<u32>,
    pub name: String,
    pub hours_record: f64,
    #[serde(default)]
    pub hours_2weeks: Option<f64>,
    #[serde(default)]
    pub history: Vec<GamePoint>,
}

impl Game {
    pub fn key(&self) -> String {
        self.appid
            .map(|a| a.to_string())
            .unwrap_or_else(|| self.name.clone())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TwoWeekPoint {
    pub ts: i64,
    pub hours: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct User {
    pub key: String,
    pub url: String,
    #[serde(default)]
    pub persona: Option<String>,
    #[serde(default)]
    pub last_seen: i64,
    #[serde(default)]
    pub two_weeks_total: Option<f64>,
    #[serde(default)]
    pub two_weeks_history: Vec<TwoWeekPoint>,
    #[serde(default)]
    pub games: BTreeMap<String, Game>,
}

#[derive(Clone, Debug)]
pub struct GameData {
    pub appid: Option<u32>,
    pub name: String,
    pub hours_2weeks: Option<f64>,
    pub hours_record: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Database {
    #[serde(default)]
    pub users: BTreeMap<String, User>,
}

impl Database {
    pub fn load() -> Self {
        let path = PathBuf::from(DB_FILE);
        match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(db) => db,
                Err(e) => {
                    eprintln!("Failed to parse {DB_FILE}: {e}");
                    let _ = fs::rename(&path, "steam_activity.json.bak");
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = PathBuf::from(format!("{DB_FILE}.tmp"));
        fs::write(&tmp, text).map_err(|e| e.to_string())?;
        fs::rename(&tmp, PathBuf::from(DB_FILE)).map_err(|e| e.to_string())
    }

    pub fn apply_callback(
        &mut self,
        key: &str,
        url: &str,
        persona: Option<&str>,
        games: &[GameData],
        two_weeks_total: Option<f64>,
        ts: i64,
    ) {
        let user = self.users.entry(key.to_string()).or_insert_with(|| User {
            key: key.to_string(),
            url: url.to_string(),
            ..Default::default()
        });
        user.url = url.to_string();
        if let Some(p) = persona {
            if !p.is_empty() {
                user.persona = Some(p.to_string());
            }
        }
        if ts > user.last_seen {
            user.last_seen = ts;
        }

        if let Some(v) = two_weeks_total {
            user.two_weeks_total = Some(v);
            if let Some(last) = user.two_weeks_history.last_mut() {
                if last.hours == v && ts - last.ts < DEDUP_WINDOW_MS {
                    last.ts = ts;
                } else {
                    user.two_weeks_history.push(TwoWeekPoint { ts, hours: v });
                }
            } else {
                user.two_weeks_history.push(TwoWeekPoint { ts, hours: v });
            }
            if user.two_weeks_history.len() > MAX_HISTORY {
                let excess = user.two_weeks_history.len() - MAX_HISTORY;
                user.two_weeks_history.drain(..excess);
            }
        }

        for g in games {
            let gkey = g
                .appid
                .map(|a| a.to_string())
                .unwrap_or_else(|| g.name.clone());
            if gkey.is_empty() {
                continue;
            }
            let game = user.games.entry(gkey.clone()).or_insert_with(|| Game {
                appid: g.appid,
                name: g.name.clone(),
                ..Default::default()
            });
            if !g.name.is_empty() {
                game.name = g.name.clone();
            }
            if let Some(v) = g.hours_record {
                game.hours_record = v;
            }
            if let Some(v) = g.hours_2weeks {
                game.hours_2weeks = Some(v);
            }

            if let Some(last) = game.history.last_mut() {
                if last.hours_2weeks == game.hours_2weeks
                    && last.hours_record == game.hours_record
                    && ts - last.ts < DEDUP_WINDOW_MS
                {
                    last.ts = ts;
                    continue;
                }
            }
            game.history.push(GamePoint {
                ts,
                hours_2weeks: game.hours_2weeks,
                hours_record: game.hours_record,
            });
            if game.history.len() > MAX_HISTORY {
                let excess = game.history.len() - MAX_HISTORY;
                game.history.drain(..excess);
            }
        }
    }
}
