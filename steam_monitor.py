import re
import threading

import requests
from bs4 import BeautifulSoup

CHECK_INTERVAL_SECONDS = 300

HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
        "AppleWebKit/537.36 (KHTML, like Gecko) "
        "Chrome/126.0.0.0 Safari/537.36"
    )
}


def normalize_url(url):
    url = (url or "").strip().strip('"\'')
    if not url:
        raise ValueError("Please enter a Steam profile URL.")
    if not re.match(r"^https?://", url, re.I):
        url = "https://" + url
    if "steamcommunity.com" not in url:
        raise ValueError("The link must point to steamcommunity.com.")
    if not re.search(r"steamcommunity\.com/(?:id|profiles)/", url, re.I):
        raise ValueError(
            "Enter a profile link like "
            "https://steamcommunity.com/id/name or "
            "https://steamcommunity.com/profiles/12345678901234567"
        )
    return url


def _clean_text(value):
    if value is None:
        return None
    return re.sub(r"\s+", " ", value).strip()


def parse_snapshot(html):
    soup = BeautifulSoup(html, "html.parser")
    hours_el = soup.select_one(".recentgame_recentplaytime > div")
    game_el = soup.select_one(".recent_games > .recent_game .game_name a")
    hours = _clean_text(hours_el.get_text(" ", strip=True)) if hours_el else None
    top_game = _clean_text(game_el.get_text(" ", strip=True)) if game_el else None
    if top_game is None and hours is None:
        return None
    return {"top_game": top_game, "hours": hours}


class SteamProfileMonitor:
    def __init__(self, url, interval=CHECK_INTERVAL_SECONDS):
        self.url = normalize_url(url)
        self.interval = interval
        self.last_snapshot = None
        self.changes = 0
        self._stop_event = threading.Event()
        self._check_now_event = threading.Event()

    def fetch_snapshot(self):
        response = requests.get(self.url, headers=HEADERS, timeout=30)
        response.raise_for_status()
        return parse_snapshot(response.text)

    def check_now(self):
        self._check_now_event.set()

    def stop(self):
        self._stop_event.set()
        self._check_now_event.set()

    def run(self, on_update):
        while not self._stop_event.is_set():
            self._check_once(on_update)
            if self._stop_event.is_set():
                break
            waited = 0.0
            while waited < self.interval and not self._stop_event.is_set():
                if self._check_now_event.wait(0.2):
                    self._check_now_event.clear()
                    break
                waited += 0.2

    def _check_once(self, on_update):
        snapshot = None
        error = None
        try:
            snapshot = self.fetch_snapshot()
        except Exception as exc:
            error = str(exc)

        changed = False
        previous = self.last_snapshot
        if snapshot is not None and previous is not None:
            changed = snapshot != previous
        if changed:
            self.changes += 1
        if snapshot is not None:
            self.last_snapshot = snapshot

        on_update(
            {
                "snapshot": snapshot,
                "previous": previous,
                "changed": changed,
                "count": self.changes,
                "error": error,
            }
        )
