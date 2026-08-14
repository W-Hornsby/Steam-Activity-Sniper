# Steam Activity Sniper

Monitors Steam profiles via Tampermonkey and graphs each user's top played games over time. The Rust app is **Tampermonkey-exclusive** — the browser extension is required; it reads game data from the real logged-in browser session, so no scraping/rate-limiting is involved.

## Installation

1. Install Tampermonkey in your browser (tested on Chrome).
2. Add the `tampermonkey.user.js` script to Tampermonkey.
3. Run the app:
   - **From source:** `cargo run --release`
   - **Installer:** build one with [cargo-packager](https://github.com/crabnebula-dev/cargo-packager) (`cargo install cargo-packager`, then `cargo packager`). The NSIS installer is written to `target\release\steam_activity_sniper_<version>_x64-setup.exe` and installs per-user (no admin required) to `%LOCALAPPDATA%\Steam Activity Sniper`.

   The app starts a local listener on `http://127.0.0.1:8765` and opens the GUI.

4. Open a Steam profile (`https://steamcommunity.com/profiles/<id>/` or `/id/<custom>/`) and enable the sniper with the toggle in the top right of the page. The tab will snapshot and auto-refresh every 5 minutes; every snapshot is sent to the app.

5. The app auto-selects the most recently active user. Use the dropdown to switch between users (one per profile tab). Select a game (or "All games — past 2 weeks" as an umbrella) and a timeframe (24 hours / 7 days / 30 days / 90 days / all time) for the graph. The right panel lists each user's top played games with hours on record and hours past 2 weeks; click one to plot it. The view follows incoming snapshots — use the Recenter button to bring the newest point back to the center of the graph.

## Data

All data is logged to `steam_activity.json` in the OS app-data directory (`%APPDATA%\SteamActivitySniper\` on Windows), so installed copies work from Program Files without admin rights and data survives reinstallation. It stores, per user (keyed by the URL segment after `/profiles/` or `/id/`, e.g. `76561198077713381` or `frvncis`):

- persona name and last-seen timestamp
- each game in their top played list: name, appid, hours in the past 2 weeks (when Steam exposes it), hours on record, and a time series of (timestamp, 2-week hours, record hours) snapshots

Identical snapshots within 60 seconds are deduplicated; every 5-minute refresh charts a new point.

## Building the installer

```
cargo install cargo-packager
cargo packager
```

Configuration lives in `[package.metadata.packager]` in `Cargo.toml`. Icons are generated from `icons/` (a chart motif matching the app). The installer is unsigned — expect a SmartScreen warning until you sign it.
