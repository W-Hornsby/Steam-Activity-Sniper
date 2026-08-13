import queue
import threading
import time
from datetime import datetime

import tkinter as tk
from tkinter import messagebox, scrolledtext

from steam_monitor import CHECK_INTERVAL_SECONDS, SteamProfileMonitor, normalize_url

COLOR_ACTIVE = "#27ae60"
COLOR_IDLE = "#7f8c8d"
COLOR_ERROR = "#e74c3c"
COLOR_WARNING = "#f39c12"


class SteamActivitySniper(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("Steam Activity Sniper")
        self.geometry("620x640")
        self.resizable(False, False)

        self.url_var = tk.StringVar()
        self.counter_var = tk.StringVar(value="Changes detected: 0")
        self.top_game_var = tk.StringVar(value="Top game: —")
        self.hours_var = tk.StringVar(value="Hours: —")
        self.countdown_var = tk.StringVar(value="")

        self.monitor = None
        self.monitor_thread = None
        self.updates = queue.Queue()
        self.next_check_at = None

        self._build_ui()
        self.protocol("WM_DELETE_WINDOW", self._on_close)
        self.after(200, self._poll_updates)
        self.after(1000, self._tick_countdown)

    def _build_ui(self):
        container = tk.Frame(self, padx=16, pady=16)
        container.pack(fill="both", expand=True)

        url_row = tk.Frame(container)
        url_row.pack(fill="x")
        tk.Label(url_row, text="Profile URL:").pack(side="left")
        self.url_entry = tk.Entry(url_row, textvariable=self.url_var, width=48)
        self.url_entry.pack(side="left", padx=(8, 8), fill="x", expand=True)
        self.start_btn = tk.Button(
            url_row, text="Start Monitoring", width=16, command=self._toggle_monitoring
        )
        self.start_btn.pack(side="left")

        self.status_label = tk.Label(
            container,
            text="Idle - enter a profile link and press Start",
            fg=COLOR_IDLE,
            anchor="w",
        )
        self.status_label.pack(fill="x", pady=(10, 2))

        self.activity_label = tk.Label(
            container, text="Waiting to start", font=("Segoe UI", 20, "bold"), fg=COLOR_IDLE
        )
        self.activity_label.pack(pady=(6, 2))

        tk.Label(
            container,
            textvariable=self.counter_var,
            font=("Segoe UI", 12, "bold"),
            fg="#2c3e50",
        ).pack(pady=(0, 6))

        tk.Label(container, textvariable=self.top_game_var, anchor="w").pack(fill="x")
        tk.Label(container, textvariable=self.hours_var, anchor="w").pack(fill="x")

        self.check_btn = tk.Button(
            container, text="Check Now", width=16, state="disabled", command=self._check_now
        )
        self.check_btn.pack(pady=(8, 2))

        tk.Label(container, textvariable=self.countdown_var, fg=COLOR_IDLE).pack()

        tk.Label(container, text="Log", font=("Segoe UI", 10, "bold"), anchor="w").pack(
            fill="x", pady=(10, 4)
        )
        self.log = scrolledtext.ScrolledText(container, height=12)
        self.log.pack(fill="both", expand=True)

    def _toggle_monitoring(self):
        if self.monitor:
            self._stop_monitoring()
        else:
            self._start_monitoring()

    def _start_monitoring(self):
        try:
            url = normalize_url(self.url_var.get())
        except ValueError as exc:
            messagebox.showerror("Invalid URL", str(exc))
            return
        self.monitor = SteamProfileMonitor(url)
        self.monitor_thread = threading.Thread(
            target=self.monitor.run, args=(self._on_update,), daemon=True
        )
        self.monitor_thread.start()
        self.start_btn.config(text="Stop Monitoring")
        self.check_btn.config(state="normal")
        self.status_label.config(text="Monitoring", fg=COLOR_ACTIVE)
        self.activity_label.config(text="Checking...", fg=COLOR_WARNING)
        self.next_check_at = time.time() + self.monitor.interval
        self._log(f"Monitoring started: {self.monitor.url}")

    def _stop_monitoring(self):
        if self.monitor:
            self.monitor.stop()
        self.monitor = None
        self.start_btn.config(text="Start Monitoring")
        self.check_btn.config(state="disabled")
        self.status_label.config(text="Stopped", fg=COLOR_IDLE)
        self.next_check_at = None
        self.countdown_var.set("")
        self._log("Monitoring stopped.")

    def _check_now(self):
        if self.monitor:
            self.monitor.check_now()
            self.countdown_var.set("Checking...")

    def _on_update(self, update):
        self.updates.put(update)

    def _poll_updates(self):
        while True:
            try:
                update = self.updates.get_nowait()
            except queue.Empty:
                break
            self._apply_update(update)
        self.after(200, self._poll_updates)

    def _apply_update(self, update):
        if self.monitor:
            self.next_check_at = time.time() + self.monitor.interval

        error = update.get("error")
        snapshot = update.get("snapshot")
        changed = update.get("changed", False)
        count = update.get("count", 0)

        self.counter_var.set(f"Changes detected: {count}")

        if error:
            self.activity_label.config(text="CHECK FAILED", fg=COLOR_ERROR)
            self.top_game_var.set("Top game: —")
            self.hours_var.set("Hours: —")
            self._log(f"ERROR: {error}")
            return

        if snapshot is None:
            self.activity_label.config(text="NO ACTIVITY VISIBLE", fg=COLOR_WARNING)
            self.top_game_var.set("Top game: —")
            self.hours_var.set("Hours: —")
            self._log("Profile shows no readable recent activity (private or empty profile?).")
            return

        top = snapshot.get("top_game") or "—"
        hours = snapshot.get("hours") or "—"
        self.top_game_var.set(f"Top game: {top}")
        self.hours_var.set(f"Hours: {hours}")

        if changed:
            self.activity_label.config(text="RECENTLY ACTIVE!", fg=COLOR_ACTIVE)
            self._log(f"RECENTLY ACTIVE! Top game: {top} | {hours}")
        else:
            self.activity_label.config(text="No change detected", fg=COLOR_IDLE)
            self._log(f"No change. Top game: {top} | {hours}")

    def _tick_countdown(self):
        if self.next_check_at:
            remaining = self.next_check_at - time.time()
            if remaining > 0:
                minutes, seconds = divmod(int(remaining), 60)
                self.countdown_var.set(f"Next check in {minutes:02d}:{seconds:02d}")
            else:
                self.countdown_var.set("Checking...")
        self.after(1000, self._tick_countdown)

    def _log(self, message):
        timestamp = datetime.now().strftime("%H:%M:%S")
        self.log.insert(tk.END, f"[{timestamp}] {message}\n")
        self.log.see(tk.END)

    def _on_close(self):
        if self.monitor:
            self.monitor.stop()
        self.destroy()


if __name__ == "__main__":
    app = SteamActivitySniper()
    app.mainloop()
