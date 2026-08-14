import random
import re
import threading
import time

import requests
from bs4 import BeautifulSoup

CHECK_INTERVAL_SECONDS = 630

HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
        "AppleWebKit/537.36 (KHTML, like Gecko) "
        "Chrome/126.0.0.0 Safari/537.36"
    ),
    "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
    "Accept-Language": "en-US,en;q=0.9",
    "Referer": "https://steamcommunity.com/",
    "Upgrade-Insecure-Requests": "1",
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


class RateLimited(Exception):
    pass


def _clean_text(value):
    if value is None:
        return None
    return re.sub(r"\s+", " ", value).strip()


def _hours_value(hours_text):
    if not hours_text:
        return None
    match = re.search(r"([\d.]+)", hours_text)
    if not match:
        return None
    try:
        return float(match.group(1))
    except ValueError:
        return None


def _diff_snapshot(previous, snapshot):
    if previous is None or snapshot is None:
        return False, None
    top_changed = snapshot.get("top_game") != previous.get("top_game")
    hours_increased = False
    new_hours = _hours_value(snapshot.get("hours"))
    old_hours = _hours_value(previous.get("hours"))
    if new_hours is not None and old_hours is not None and new_hours > old_hours:
        hours_increased = True
    if top_changed and hours_increased:
        return True, "New top game and hours increased"
    if top_changed:
        return True, "New top game"
    if hours_increased:
        return True, "Hours increased"
    return False, None


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
        self.next_wait = interval
        self._stop_event = threading.Event()
        self._check_now_event = threading.Event()
        self._session = self._build_session()
        self._rate_limit_until = 0.0

    @staticmethod
    def _build_session():
        session = requests.Session()
        session.headers.update(HEADERS)
        return session

    def fetch_snapshot(self):
        now = time.time()
        if now < self._rate_limit_until:
            raise RateLimited(f"Rate limited, backing off for {self._rate_limit_until - now:.0f}s")
        try:
            response = self._session.get(self.url, timeout=30)
        except requests.exceptions.RequestException:
            raise
        if response.status_code == 429:
            self._rate_limit_until = now + self._retry_after(response)
            raise RateLimited(
                f"HTTP 429 Too Many Requests. Backing off for "
                f"{self._rate_limit_until - now:.0f}s."
            )
        response.raise_for_status()
        return parse_snapshot(response.text)

    @staticmethod
    def _retry_after(response):
        retry_after = response.headers.get("Retry-After")
        if retry_after:
            try:
                return max(int(retry_after), 1)
            except (TypeError, ValueError):
                pass
        return 300

    def check_now(self):
        now = time.time()
        if now < self._rate_limit_until:
            return
        self._check_now_event.set()

    def stop(self):
        self._stop_event.set()
        self._check_now_event.set()

    def run(self, on_update):
        consecutive_errors = 0
        while not self._stop_event.is_set():
            ok = self._check_once(on_update)
            if self._stop_event.is_set():
                break
            if not ok:
                consecutive_errors += 1
            else:
                consecutive_errors = 0
            wait = self.interval
            if consecutive_errors:
                wait = self.interval * min(2 ** consecutive_errors, 4)
            rate_remaining = self._rate_limit_until - time.time()
            if rate_remaining > wait:
                wait = rate_remaining
            self.next_wait = wait
            waited = 0.0
            while waited < wait and not self._stop_event.is_set():
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

        changed, reason = _diff_snapshot(self.last_snapshot, snapshot)
        previous = self.last_snapshot
        if changed:
            self.changes += 1
        if snapshot is not None:
            self.last_snapshot = snapshot

        self._emit(on_update, snapshot, previous, changed, reason, error)
        return error is None

    def _emit(self, on_update, snapshot, previous, changed, reason, error):
        on_update(
            {
                "snapshot": snapshot,
                "previous": previous,
                "changed": changed,
                "reason": reason,
                "count": self.changes,
                "error": error,
            }
        )


class TampermonkeySource:
    def __init__(self, interval=CHECK_INTERVAL_SECONDS):
        self.interval = interval
        self.last_snapshot = None
        self.changes = 0
        self.next_wait = interval
        self.connected = False
        self._lock = threading.Lock()
        self._latest = None
        self._on_update = None

    def attach(self, on_update):
        self._on_update = on_update

    def on_hello(self):
        self.connected = True

    @staticmethod
    def _clean_kv(snapshot):
        if snapshot is None:
            return None
        return {
            "top_game": _clean_text(snapshot.get("top_game")),
            "hours": _clean_text(snapshot.get("hours")),
        }

    def on_callback(self, data):
        with self._lock:
            snapshot = self._clean_kv(data.get("snapshot"))
            on_update = self._on_update
        if on_update is None:
            return
        if data.get("disabled"):
            slot = {
                "snapshot": None,
                "previous": self.last_snapshot,
                "changed": False,
                "reason": None,
                "count": self.changes,
                "error": None,
                "source": "tampermonkey",
                "page": data.get("url") or data.get("page"),
                "disabled": True,
            }
            try:
                on_update(slot)
            except Exception:
                pass
            return
        previous = self.last_snapshot
        changed, reason = _diff_snapshot(previous, snapshot)
        if changed:
            self.changes += 1
        if snapshot is not None:
            self.last_snapshot = snapshot
        self.next_wait = self.interval
        slot = {
            "snapshot": snapshot,
            "previous": previous,
            "changed": changed,
            "reason": reason,
            "count": self.changes,
            "error": None,
            "source": "tampermonkey",
            "page": data.get("url") or data.get("page"),
        }
        with self._lock:
            self._latest = slot
        try:
            on_update(slot)
        except Exception:
            pass

    def take_latest(self):
        with self._lock:
            slot = self._latest
            self._latest = None
        return slot
