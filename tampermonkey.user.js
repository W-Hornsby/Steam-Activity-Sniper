// ==UserScript==
// @name         Steam Activity Sniper Callback
// @namespace    SteamActivitySniper
// @version      3.1.0
// @description  Toggleable per-page monitoring on any Steam profile. Reads games played (name, hours past 2 weeks, hours on record) in your real browser session and posts snapshots to the local Rust GUI. Supports the new profile layout (recent activity) and the legacy game list.
// @match        https://steamcommunity.com/*
// @grant        GM_xmlhttpRequest
// @grant        GM_addStyle
// @grant        GM_getValue
// @grant        GM_setValue
// @run-at       document-idle
// ==/UserScript==

(function () {
    "use strict";

    const LISTENER = "http://127.0.0.1:8765";
    const REFRESH_MS = 5 * 60 * 1000;
    const ENABLED_KEY = "sas_enabled_" + location.pathname.split("?")[0];

    function parseHours(text) {
        if (!text) return null;
        const m = text.replace(/,/g, "").match(/([\d.]+)\s*hrs/);
        if (!m) return null;
        const v = parseFloat(m[1]);
        return Number.isFinite(v) ? v : null;
    }

    function clean(text) {
        return text ? text.replace(/\s+/g, " ").trim() : "";
    }

    function appidFromHref(href) {
        const m = (href || "").match(/\/app\/(\d+)/);
        return m ? parseInt(m[1], 10) : null;
    }

    // Legacy profile markup: a game row with both "hrs past two weeks" and
    // "hrs on record" values, plus a data-appid attribute.
    function parseGameListRow(row) {
        const nameEl = row.querySelector(".game_name a");
        if (!nameEl) return null;
        const name = clean(nameEl.textContent);
        if (!name) return null;
        const appidRaw = row.getAttribute("data-appid");
        const appid = appidRaw && /^\d+$/.test(appidRaw) ? parseInt(appidRaw, 10) : null;
        let hours2w = null;
        let hoursRecord = null;
        row.querySelectorAll(".hours_played").forEach(function (el) {
            const v = parseHours(el.textContent);
            if (v === null) return;
            if (el.closest(".hours_played_2weeks")) {
                hours2w = v;
            } else {
                hoursRecord = v;
            }
        });
        return {
            appid: appid,
            name: name,
            hours_2weeks: hours2w,
            hours_record: hoursRecord,
        };
    }

    function parseGameListHtml(html) {
        const doc = document.createElement("div");
        doc.innerHTML = html;
        const games = [];
        doc.querySelectorAll(".game_list_row").forEach(function (row) {
            const g = parseGameListRow(row);
            if (g) games.push(g);
        });
        return games;
    }

    // New profile layout: the "Recent Activity" section. Each game has its
    // name, appid, hours on record and last-played date. Steam only exposes a
    // combined "X hours past 2 weeks" figure in the section header here.
    function parseRecentGames() {
        const games = [];
        document.querySelectorAll(".recent_games > .recent_game").forEach(function (row) {
            const link = row.querySelector(".game_name a");
            if (!link) return;
            const name = clean(link.textContent);
            if (!name) return;
            const detailsEl = row.querySelector(".game_info_details");
            const details = clean(detailsEl ? detailsEl.textContent : null) || "";
            const rec = details.match(/([\d.,]+)\s*hrs on record/);
            games.push({
                appid: appidFromHref(link.href),
                name: name,
                hours_2weeks: null,
                hours_record: rec ? parseFloat(rec[1].replace(/,/g, "")) : null,
            });
        });
        return games;
    }

    function readSnapshot() {
        const personaEl = document.querySelector(".actual_persona_name");
        const persona = clean(personaEl ? personaEl.textContent : null) || null;

        const legacyRows = document.querySelectorAll(".game_list_row");
        if (legacyRows.length > 0) {
            const games = [];
            legacyRows.forEach(function (row) {
                const g = parseGameListRow(row);
                if (g) games.push(g);
            });
            return { persona: persona, games: games, two_weeks_total: null };
        }

        const games = parseRecentGames();
        const twoWeekEl = document.querySelector(
            ".profile_recentgame_header .recentgame_recentplaytime > div"
        );
        return {
            persona: persona,
            games: games,
            two_weeks_total: parseHours(twoWeekEl ? twoWeekEl.textContent : null),
        };
    }

    // Best-effort enrichment: the games tab may still render the legacy game
    // list with per-game 2-week hours for the logged-in session.
    function fetchGamesTabGames(callback) {
        GM_xmlhttpRequest({
            method: "GET",
            url: location.origin + "/games/?tab=all",
            onload: function (resp) {
                let games = [];
                try {
                    games = parseGameListHtml(resp.responseText);
                } catch (e) {
                    games = [];
                }
                callback(games);
            },
            onerror: function () {
                callback([]);
            },
        });
    }

    function post(path, payload) {
        GM_xmlhttpRequest({
            method: "POST",
            url: LISTENER + path,
            headers: { "Content-Type": "application/json" },
            data: JSON.stringify(payload),
            onload: function () {},
            onerror: function (ev) { if (ev.error) console.log(ev.error); },
        });
    }

    function isProfile() {
        return /^\/((id|profiles)\/.+|profiles\/\d+|id\/.+)\/?$/.test(location.pathname);
    }

    function buildPayload(manual) {
        const payload = {
            page: location.pathname,
            url: location.href,
            time: Date.now(),
            manual: !!manual,
        };
        if (isProfile()) {
            const snapshot = readSnapshot();
            payload.persona = snapshot.persona;
            payload.games = snapshot.games;
            if (snapshot.two_weeks_total !== null) {
                payload.two_weeks_total = snapshot.two_weeks_total;
            }
        }
        return payload;
    }

    // Read the profile, optionally merge per-game 2-week hours from the games
    // tab (bounded wait so snapshots still post if it is slow or unavailable),
    // then post a single snapshot.
    function postSnapshot(manual) {
        const payload = buildPayload(manual);
        if (!isProfile() || payload.games.length === 0) {
            post("/callback", payload);
            return;
        }
        let settled = false;
        const done = function (enriched) {
            if (settled) return;
            settled = true;
            if (enriched && enriched.length > 0) {
                const has2wk = enriched.some(function (g) { return g.hours_2weeks !== null; });
                if (has2wk) {
                    payload.games = enriched;
                    payload.two_weeks_total = enriched.reduce(function (a, g) {
                        return a + (g.hours_2weeks || 0);
                    }, 0);
                }
            }
            post("/callback", payload);
        };
        fetchGamesTabGames(done);
        setTimeout(function () { done(null); }, 2500);
    }

    function scheduleReload() {
        clearTimeout(scheduleReload._t);
        if (!getEnabled()) {
            renderTimer();
            return;
        }
        scheduleReload._deadline = Date.now() + REFRESH_MS;
        scheduleReload._t = setTimeout(function () {
            hardReload();
        }, REFRESH_MS);
        renderTimer();
    }
    scheduleReload._deadline = 0;

    function hardReload() {
        // Perform a true full tab reload. Prefer the navigation API which
        // forces a fresh network fetch (bypasses the browser/HTTP cache), then
        // fall back to a plain location.reload() in older browsers.
        try {
            if (navigation && typeof navigation.reload === "function") {
                navigation.reload({ cache: "reload" });
                return;
            }
        } catch (e) {}
        location.reload();
    }

    function getEnabled() {
        return GM_getValue(ENABLED_KEY, false);
    }

    function setEnabled(on) {
        GM_setValue(ENABLED_KEY, on);
    }

    let badge, toggleBtn, refreshBtn, timerEl;

    function formatTime(ms) {
        const total = Math.max(0, Math.ceil(ms / 1000));
        const m = Math.floor(total / 60);
        const s = total % 60;
        return m + ":" + (s < 10 ? "0" : "") + s;
    }

    function renderTimer() {
        if (!timerEl) return;
        if (getEnabled() && scheduleReload._deadline) {
            timerEl.textContent = "Next refresh: " + formatTime(scheduleReload._deadline - Date.now());
            setTimeout(renderTimer, 1000);
        } else {
            timerEl.textContent = "";
        }
    }

    function render() {
        const on = getEnabled();
        badge.classList.toggle("on", on);
        toggleBtn.textContent = on ? "ON" : "OFF";
        refreshBtn.style.display = on ? "inline-block" : "none";
        renderTimer();
    }

    function toggle() {
        const on = !getEnabled();
        setEnabled(on);
        if (on) {
            post("/hello", { page: location.pathname, url: location.href });
            postSnapshot(true);
            scheduleReload();
        } else {
            clearTimeout(scheduleReload._t);
            scheduleReload._deadline = 0;
            post("/callback", {
                page: location.pathname,
                url: location.href,
                time: Date.now(),
                disabled: true,
            });
        }
        render();
    }

    function forceRefresh() {
        postSnapshot(true);
        scheduleReload._deadline = Date.now() + REFRESH_MS;
        renderTimer();
        hardReload();
    }

    GM_addStyle(
        "#sas-badge{position:fixed;top:8px;right:8px;z-index:9999;display:flex;gap:6px;" +
        "align-items:center;background:#34495e;color:#fff;border-radius:6px;padding:6px 8px;" +
        "font:12px sans-serif;box-shadow:0 1px 4px rgba(0,0,0,.35);user-select:none;}" +
        "#sas-badge .sas-btn{background:#7f8c8d;color:#fff;border:none;border-radius:4px;" +
        "padding:4px 8px;font:inherit;cursor:pointer;}" +
        "#sas-badge.on .sas-toggle{background:#27ae60;}" +
        "#sas-badge .sas-refresh{background:#16a085;}" +
        "#sas-badge .sas-timer{font-size:11px;color:#ecf0f1;white-space:nowrap;}"
    );
    badge = document.createElement("div");
    badge.id = "sas-badge";

    toggleBtn = document.createElement("button");
    toggleBtn.className = "sas-btn sas-toggle";
    toggleBtn.onclick = toggle;

    refreshBtn = document.createElement("button");
    refreshBtn.className = "sas-btn sas-refresh";
    refreshBtn.textContent = "Refresh";
    refreshBtn.onclick = forceRefresh;

    timerEl = document.createElement("span");
    timerEl.className = "sas-timer";

    badge.appendChild(toggleBtn);
    badge.appendChild(refreshBtn);
    badge.appendChild(timerEl);
    document.body.appendChild(badge);
    render();

    if (getEnabled()) {
        post("/hello", { page: location.pathname, url: location.href });
        postSnapshot(false);
        scheduleReload();
    }
})();
