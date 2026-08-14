// ==UserScript==
// @name         Steam Activity Sniper Callback
// @namespace    SteamActivitySniper
// @version      2.0.0
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
        setTimeout(function () {
            if (!getEnabled()) {
                render();
                return;
            }
            if (!document.hidden) {
                location.reload();
            } else {
                scheduleReload();
            }
        }, REFRESH_MS);
    }

    function getEnabled() {
        return GM_getValue(ENABLED_KEY, false);
    }

    function setEnabled(on) {
        GM_setValue(ENABLED_KEY, on);
    }

    let badge;

    function render() {
        const on = getEnabled();
        badge.classList.toggle("on", on);
        badge.textContent = on ? "Sniper: ON (this page)" : "Sniper: OFF (click to enable)";
    }

    function toggle() {
        const on = !getEnabled();
        setEnabled(on);
        if (on) {
            post("/hello", { page: location.pathname, url: location.href });
            post("/callback", buildPayload(false));
            scheduleReload();
        } else {
            post("/callback", {
                page: location.pathname,
                url: location.href,
                time: Date.now(),
                disabled: true,
            });
        }
        render();
    }

    GM_addStyle(
        "#sas-badge{position:fixed;top:8px;right:8px;z-index:9999;padding:7px 12px;" +
        "background:#7f8c8d;color:#fff;border-radius:4px;font:12px sans-serif;cursor:pointer;" +
        "user-select:none;box-shadow:0 1px 3px rgba(0,0,0,.3);}" +
        "#sas-badge.on{background:#27ae60;}"
    );
    badge = document.createElement("div");
    badge.id = "sas-badge";
    badge.onclick = toggle;
    document.body.appendChild(badge);
    render();

    if (getEnabled()) {
        post("/hello", { page: location.pathname, url: location.href });
        post("/callback", buildPayload(false));
        scheduleReload();
    }
})();
