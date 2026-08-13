import re
import threading

from bs4 import BeautifulSoup

try:
    from curl_cffi import requests as http
    HTTP_IMPERSONATE = "chrome"
except ImportError:
    import requests as http
    HTTP_IMPERSONATE = None

CHECK_INTERVAL_SECONDS = 300

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

    @staticmethod
    def _build_session():
        if HTTP_IMPERSONATE:
            session = http.Session(impersonate=HTTP_IMPERSONATE)
        else:
            session = http.Session()
        session.headers.update(HEADERS)
        return session

    def fetch_snapshot(self):
        response = self._session.get(self.url, timeout=30)
        response.raise_for_status()
        return parse_snapshot(response.text)

    def check_now(self):
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

        changed = False
        reason = None
        previous = self.last_snapshot
        if snapshot is not None and previous is not None:
            top_changed = snapshot.get("top_game") != previous.get("top_game")
            hours_increased = False
            new_hours = _hours_value(snapshot.get("hours"))
            old_hours = _hours_value(previous.get("hours"))
            if new_hours is not None and old_hours is not None and new_hours > old_hours:
                hours_increased = True
            if top_changed and hours_increased:
                reason = "New top game and hours increased"
            elif top_changed:
                reason = "New top game"
            elif hours_increased:
                reason = "Hours increased"
            changed = reason is not None
        if changed:
            self.changes += 1
        if snapshot is not None:
            self.last_snapshot = snapshot

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
        return error is None
