mod app;
mod db;
mod listener;

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use db::Database;
use eframe::egui;

fn main() -> eframe::Result {
    let db = Arc::new(Mutex::new(Database::load()));
    let (tx, rx) = mpsc::channel::<listener::Event>();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([900.0, 620.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Steam Activity Sniper",
        options,
        Box::new(move |cc| {
            let egui_ctx = cc.egui_ctx.clone();
            let listener_db = Arc::clone(&db);
            let listener_tx = tx.clone();
            std::thread::spawn(move || listener::run(listener_db, listener_tx, egui_ctx));
            Ok(Box::new(app::SniperApp::new(db, rx)))
        }),
    )
}
