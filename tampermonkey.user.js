// ==UserScript==
// @name         Steam Activity Sniper Callback
// @namespace    SteamActivitySniper
// @version      2.2.0
// @description  Toggleable per-page monitoring on any Steam page. When enabled on a profile, reads recent-game metadata in your real browser session and posts snapshots to the local Python GUI.
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

    function readSnapshot() {
        const gameEl = document.querySelector(".recent_games > .recent_game .game_name a");
        const hoursEl = document.querySelector(".recentgame_recentplaytime > div");
        const top_game = gameEl ? gameEl.textContent.replace(/\s+/g, " ").trim() : null;
        const hours = hoursEl ? hoursEl.textContent.replace(/\s+/g, " ").trim() : null;
        return { top_game: top_game, hours: hours };
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
            payload.snapshot = readSnapshot();
        }
        return payload;
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
            forceRefresh();
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
        post("/callback", buildPayload(true));
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
        post("/callback", buildPayload(false));
        scheduleReload();
    }
})();
