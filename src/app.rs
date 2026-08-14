use std::collections::BTreeMap;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Local, Utc};
use eframe::egui;
use egui::{Color32, ComboBox, RichText, ScrollArea};
use egui_plot::{Corner, Legend, Line, Plot, PlotBounds};

use crate::db::{Database, Game};
use crate::listener::{user_key_from_url, CallbackPayload, Event, PORT};

const SERIES_RECORD: &str = "Hours on record";
const SERIES_2WK: &str = "Hours past 2 weeks";
const MAX_LOG_LINES: usize = 300;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Timeframe {
    Hours24,
    Days7,
    Days30,
    Days90,
    All,
}

impl Timeframe {
    fn label(&self) -> &'static str {
        match self {
            Timeframe::Hours24 => "24 hours",
            Timeframe::Days7 => "7 days",
            Timeframe::Days30 => "30 days",
            Timeframe::Days90 => "90 days",
            Timeframe::All => "All time",
        }
    }

    fn window_secs(&self) -> Option<i64> {
        match self {
            Timeframe::Hours24 => Some(24 * 3600),
            Timeframe::Days7 => Some(7 * 86400),
            Timeframe::Days30 => Some(30 * 86400),
            Timeframe::Days90 => Some(90 * 86400),
            Timeframe::All => None,
        }
    }

    fn all() -> [Timeframe; 5] {
        [
            Timeframe::Hours24,
            Timeframe::Days7,
            Timeframe::Days30,
            Timeframe::Days90,
            Timeframe::All,
        ]
    }
}

pub struct SniperApp {
    db: Arc<Mutex<Database>>,
    rx: Receiver<Event>,
    selected_user: Option<String>,
    selected_game: Option<String>,
    timeframe: Timeframe,
    log: Vec<String>,
    connected: bool,
    recenter: bool,
    last_view_sig: Option<(Option<String>, Option<String>, Timeframe)>,
}

impl SniperApp {
    pub fn new(db: Arc<Mutex<Database>>, rx: Receiver<Event>) -> Self {
        let mut app = Self {
            db,
            rx,
            selected_user: None,
            selected_game: None,
            timeframe: Timeframe::All,
            log: Vec::new(),
            connected: false,
            recenter: true,
            last_view_sig: None,
        };
        {
            let db_guard = app.db.lock().unwrap();
            if let Some(most_recent) = db_guard.users.values().max_by_key(|u| u.last_seen) {
                app.selected_user = Some(most_recent.key.clone());
            }
        }
        app
    }

    fn poll_events(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Hello(url) => {
                    self.connected = true;
                    if let Some(key) = user_key_from_url(&url) {
                        self.logf(format!("Browser tab active: {key}"));
                        self.selected_user = Some(key);
                    }
                }
                Event::Callback(payload) => {
                    self.connected = true;
                    self.log_callback(&payload);
                }
                Event::Log(msg) => self.logf(msg),
            }
        }
    }

    fn logf(&mut self, msg: String) {
        let now = Local::now().format("%H:%M:%S").to_string();
        self.log.push(format!("[{now}] {msg}"));
        if self.log.len() > MAX_LOG_LINES {
            self.log.drain(..self.log.len() - MAX_LOG_LINES);
        }
    }

    fn log_callback(&mut self, p: &CallbackPayload) {
        if p.disabled {
            self.logf(format!("Monitoring toggled off on {}", p.page));
            return;
        }
        if let Some(key) = user_key_from_url(&p.url) {
            let persona = p
                .persona
                .as_deref()
                .map(|s| format!(" ({s})"))
                .unwrap_or_default();
            let manual = if p.manual { " (manual)" } else { "" };
            self.logf(format!(
                "Snapshot from {key}{persona}: {} game(s){manual}",
                p.games.len()
            ));
        } else {
            self.logf(format!("Snapshot from non-profile page: {}", p.url));
        }
    }

    fn render_top(&mut self, ui: &mut egui::Ui, users: &[(String, String)]) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("User").strong());
            ComboBox::from_id_salt("user_combo")
                .selected_text(self.selected_user.clone().unwrap_or_else(|| "—".to_string()))
                .show_ui(ui, |ui| {
                    for (key, label) in users {
                        ui.selectable_value(&mut self.selected_user, Some(key.clone()), label.clone());
                    }
                });

            let game_label = if let Some(gk) = &self.selected_game {
                gk.clone()
            } else {
                "All games — past 2 weeks".to_string()
            };
            ui.label(RichText::new("Game").strong());
            ComboBox::from_id_salt("game_combo")
                .selected_text(game_label)
                .show_ui(ui, |ui| {
                    let db = self.db.lock().unwrap();
                    let user = self.selected_user.as_ref().and_then(|k| db.users.get(k));
                    let mut games: Vec<(String, &Game)> = user
                        .map(|u| {
                            u.games
                                .values()
                                .map(|game| (game.key(), game))
                                .collect()
                        })
                        .unwrap_or_default();
                    games.sort_by(|a, b| {
                        b.1.hours_record
                            .partial_cmp(&a.1.hours_record)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    ui.selectable_value(
                        &mut self.selected_game,
                        None,
                        "All games — past 2 weeks",
                    );
                    for (key, game) in games {
                        ui.selectable_value(
                            &mut self.selected_game,
                            Some(key.clone()),
                            format!("{} — {:.1}h", game.name, game.hours_record),
                        );
                    }
                });

            ui.label(RichText::new("Timeframe").strong());
            ComboBox::from_id_salt("timeframe_combo")
                .selected_text(self.timeframe.label())
                .show_ui(ui, |ui| {
                    for tf in Timeframe::all() {
                        ui.selectable_value(&mut self.timeframe, tf, tf.label());
                    }
                });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (color, text) = if self.connected {
                    (Color32::from_rgb(39, 174, 96), "Connected")
                } else {
                    (Color32::from_rgb(127, 140, 141), "Waiting for browser")
                };
                ui.label(RichText::new(format!("● {text}")).color(color).strong());
                ui.label(RichText::new(format!("Listener 127.0.0.1:{PORT}")).weak());
            });
        });
    }

    fn render_sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.label(RichText::new("Top played games").strong());
        ui.separator();
        let db = self.db.lock().unwrap();
        let Some(user) = self.selected_user.as_ref().and_then(|k| db.users.get(k)) else {
            ui.label("No data for this user yet.");
            return;
        };

        let mut games: Vec<&Game> = user.games.values().collect();
        games.sort_by(|a, b| {
            b.hours_record
                .partial_cmp(&a.hours_record)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        ui.label(
            RichText::new(format!(
                "Persona: {}",
                user.persona.as_deref().unwrap_or("—")
            ))
            .weak(),
        );
        ui.label(RichText::new(format!("Last seen: {}", fmt_ts(user.last_seen))).weak());
        ui.add_space(6.0);

        let sum_2wk: f64 = games.iter().filter_map(|g| g.hours_2weeks).sum();
        let total_2wk = user.two_weeks_total.or_else(|| (sum_2wk > 0.0).then_some(sum_2wk));
        let total_record: f64 = games.iter().map(|g| g.hours_record).sum();
        ui.label(
            RichText::new(format!(
                "Total: {}h past 2 weeks · {total_record:.1}h on record",
                total_2wk.map(|v| format!("{v:.1}")).unwrap_or_else(|| "—".to_string())
            ))
            .weak(),
        );
        ui.add_space(6.0);

        ScrollArea::vertical().show(ui, |ui| {
            ui.selectable_value(
                &mut self.selected_game,
                None,
                "All games — past 2 weeks",
            );
            for game in games {
                let label = match game.hours_2weeks {
                    Some(v) if v > 0.0 => format!(
                        "{} — {:.1}h · {v:.1}h /2wk",
                        game.name, game.hours_record
                    ),
                    _ => format!("{} — {:.1}h", game.name, game.hours_record),
                };
                ui.selectable_value(&mut self.selected_game, Some(game.key()), label);
            }
        });
    }

    fn build_series(
        &self,
        cutoff_ts: Option<i64>,
    ) -> (Vec<[f64; 2]>, Vec<[f64; 2]>, Option<(String, Option<f64>, f64)>) {
        let db = self.db.lock().unwrap();
        let Some(user) = self.selected_user.as_ref().and_then(|k| db.users.get(k)) else {
            return (Vec::new(), Vec::new(), None);
        };

        let filter = |p: &crate::db::GamePoint| -> bool { cutoff_ts.map_or(true, |c| p.ts >= c) };

        match &self.selected_game {
            Some(gk) => {
                if let Some(game) = user.games.get(gk) {
                    let record: Vec<[f64; 2]> = game
                        .history
                        .iter()
                        .filter(|p| filter(p))
                        .map(|p| [p.ts as f64 / 1000.0, p.hours_record])
                        .collect();
                    let two_week: Vec<[f64; 2]> = game
                        .history
                        .iter()
                        .filter(|p| filter(p))
                        .filter_map(|p| p.hours_2weeks.map(|h| [p.ts as f64 / 1000.0, h]))
                        .collect();
                    (
                        record,
                        two_week,
                        Some((
                            game.name.clone(),
                            game.hours_2weeks,
                            game.hours_record,
                        )),
                    )
                } else {
                    (Vec::new(), Vec::new(), None)
                }
            }
            None => {
                let mut sums: BTreeMap<i64, (f64, f64)> = BTreeMap::new();
                for game in user.games.values() {
                    for p in game.history.iter().filter(|p| filter(p)) {
                        let e = sums.entry(p.ts).or_insert((0.0, 0.0));
                        if let Some(h) = p.hours_2weeks {
                            e.0 += h;
                        }
                        e.1 += p.hours_record;
                    }
                }
                let record: Vec<[f64; 2]> = sums
                    .iter()
                    .map(|(ts, (_, rec))| [*ts as f64 / 1000.0, *rec])
                    .collect();
                let two_week: Vec<[f64; 2]> = if !user.two_weeks_history.is_empty() {
                    user.two_weeks_history
                        .iter()
                        .filter(|p| cutoff_ts.map_or(true, |c| p.ts >= c))
                        .map(|p| [p.ts as f64 / 1000.0, p.hours])
                        .collect()
                } else {
                    sums.iter()
                        .filter(|(_, (tw, _))| *tw > 0.0)
                        .map(|(ts, (tw, _))| [*ts as f64 / 1000.0, *tw])
                        .collect()
                };
                let sum_2wk: f64 = user.games.values().filter_map(|g| g.hours_2weeks).sum();
                let total_2wk = user.two_weeks_total.or_else(|| (sum_2wk > 0.0).then_some(sum_2wk));
                let total_record: f64 = user.games.values().map(|g| g.hours_record).sum();
                (
                    record,
                    two_week,
                    Some(("All games".to_string(), total_2wk, total_record)),
                )
            }
        }
    }

    fn render_plot(&mut self, ui: &mut egui::Ui) {
        let cutoff_ts = self.timeframe.window_secs().map(|w| {
            let now = Local::now().timestamp();
            (now - w) * 1000
        });
        let (record, two_week, summary) = self.build_series(cutoff_ts);
        let recenter = std::mem::take(&mut self.recenter);

        if let Some((name, h2w, hrec)) = summary {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).strong().size(18.0));
                let sub = match h2w {
                    Some(h) => format!("— {h:.1}h past 2 weeks · {hrec:.1}h on record"),
                    None => format!("— {hrec:.1}h on record"),
                };
                ui.label(RichText::new(sub).size(15.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Recenter").clicked() {
                        self.recenter = true;
                    }
                });
            });
        }
        ui.separator();

        if record.is_empty() && two_week.is_empty() {
            ui.add_space(24.0);
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new(
                        "No data yet — open a Steam profile in your browser, enable the \
                         Tampermonkey toggle, and snapshots will appear here.",
                    )
                    .weak(),
                );
            });
            return;
        }

        let span_secs = self.timeframe.window_secs().unwrap_or(30 * 86400) as f64;
        let bounds = if recenter {
            recent_bounds(&record, &two_week, span_secs)
        } else {
            None
        };
        let available = (ui.available_height() - 8.0).max(220.0);
        Plot::new("main_plot")
            .height(available)
            .legend(Legend::default().position(Corner::LeftTop))
            .x_axis_formatter(|mark, _| fmt_ts(mark.value as i64))
            .y_axis_formatter(|mark, _| format!("{:.1}", mark.value))
            .show(ui, |plot_ui| {
                if !record.is_empty() {
                    plot_ui.line(
                        Line::new(SERIES_RECORD, record)
                            .color(Color32::from_rgb(39, 174, 96)),
                    );
                }
                if !two_week.is_empty() {
                    plot_ui.line(
                        Line::new(SERIES_2WK, two_week)
                            .color(Color32::from_rgb(241, 196, 15)),
                    );
                }
                if let Some(b) = bounds {
                    plot_ui.set_plot_bounds(b);
                }
            });
    }

    fn render_log(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Log")
            .default_open(true)
            .show(ui, |ui| {
                ScrollArea::vertical()
                    .max_height(150.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        let lines: Vec<String> = self.log.clone();
                        for line in lines {
                            ui.label(RichText::new(line).weak());
                        }
                    });
            });
    }
}

impl eframe::App for SniperApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let users: Vec<(String, String)> = {
            let db = self.db.lock().unwrap();
            let mut keys: Vec<String> = db.users.keys().cloned().collect();
            keys.sort_by(|a, b| {
                let la = db.users.get(a).map(|u| u.last_seen).unwrap_or(0);
                let lb = db.users.get(b).map(|u| u.last_seen).unwrap_or(0);
                lb.cmp(&la)
            });
            keys.into_iter()
                .map(|k| {
                    let label = db
                        .users
                        .get(&k)
                        .map(|u| match &u.persona {
                            Some(p) if !p.is_empty() => format!("{p} ({k})"),
                            _ => k.clone(),
                        })
                        .unwrap_or_else(|| k.clone());
                    (k, label)
                })
                .collect()
        };

        if self.selected_user.is_none() && !users.is_empty() {
            self.selected_user = Some(users[0].0.clone());
        }
        if let Some(k) = &self.selected_user {
            if !users.iter().any(|(key, _)| key == k) {
                self.selected_user = None;
            }
        }

        let game_valid = {
            let db = self.db.lock().unwrap();
            let user = self.selected_user.as_ref().and_then(|k| db.users.get(k));
            match (&self.selected_game, user) {
                (Some(gk), Some(u)) => u.games.contains_key(gk),
                (None, _) => true,
                _ => false,
            }
        };
        if !game_valid {
            self.selected_game = None;
        }

        let sig = (self.selected_user.clone(), self.selected_game.clone(), self.timeframe);
        if self.last_view_sig.as_ref() != Some(&sig) {
            self.recenter = true;
            self.last_view_sig = Some(sig);
        }

        egui::Panel::top("top_panel").show(ui, |ui| {
            ui.add_space(4.0);
            self.render_top(ui, &users);
            ui.add_space(4.0);
        });

        egui::Panel::bottom("log_panel")
            .resizable(true)
            .default_size(170.0)
            .min_size(60.0)
            .show(ui, |ui| {
                self.render_log(ui);
            });

        egui::Panel::right("games_panel")
            .resizable(true)
            .default_size(300.0)
            .size_range(220.0..=450.0)
            .show(ui, |ui| {
                self.render_sidebar(ui);
            });

        egui::CentralPanel::default().show(ui, |ui| {
            self.render_plot(ui);
        });
    }
}

fn fmt_ts(ts: i64) -> String {
    if ts <= 0 {
        return "—".to_string();
    }
    let secs = if ts >= 100_000_000_000 { ts / 1000 } else { ts };
    match DateTime::<Utc>::from_timestamp(secs, 0) {
        Some(dt) => dt.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string(),
        None => ts.to_string(),
    }
}

// View bounds ending at the present (the latest data point), spanning the
// requested window, with the y-axis fitted to the visible data.
fn recent_bounds(record: &[[f64; 2]], two_week: &[[f64; 2]], span_secs: f64) -> Option<PlotBounds> {
    let mut x_max = f64::NEG_INFINITY;
    for p in record.iter().chain(two_week.iter()) {
        if p[0] > x_max {
            x_max = p[0];
        }
    }
    if !x_max.is_finite() {
        return None;
    }
    let x_max = x_max.max(Local::now().timestamp() as f64);
    let x_min = x_max - span_secs;

    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for p in record.iter().chain(two_week.iter()) {
        if p[0] >= x_min {
            if p[1] < y_min {
                y_min = p[1];
            }
            if p[1] > y_max {
                y_max = p[1];
            }
        }
    }
    if !y_min.is_finite() {
        y_min = 0.0;
        y_max = 10.0;
    } else if y_max - y_min < 0.5 {
        let mid = (y_min + y_max) / 2.0;
        y_min = mid - 0.25;
        y_max = mid + 0.25;
    } else {
        let pad = (y_max - y_min) * 0.08;
        y_min = (y_min - pad).max(0.0);
        y_max += pad;
    }
    Some(PlotBounds::from_min_max([x_min, y_min], [x_max, y_max]))
}
