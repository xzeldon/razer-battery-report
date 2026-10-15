#!/usr/bin/env python3
"""Download the Lucide icons of the tray and their license into assets/icons/lucide/."""

import sys
import urllib.request
from pathlib import Path

LUCIDE_VERSION = "1.57.0"
ICONS = ["mouse", "mouse-off", "zap", "moon"]

RAW = f"https://raw.githubusercontent.com/lucide-icons/lucide/{LUCIDE_VERSION}"
TARGET = Path(__file__).resolve().parent.parent / "assets" / "icons" / "lucide"
TIMEOUT_SECONDS = 60


def download(url):
    request = urllib.request.Request(url, headers={"User-Agent": "razer-battery-report-fetch-icons"})
    with urllib.request.urlopen(request, timeout=TIMEOUT_SECONDS) as answer:
        return answer.read().decode("utf-8").replace("\r\n", "\n")


def main():
    TARGET.mkdir(parents=True, exist_ok=True)
    files = {f"{name}.svg": f"{RAW}/icons/{name}.svg" for name in ICONS}
    files["LICENSE"] = f"{RAW}/LICENSE"

    try:
        fetched = {name: download(url) for name, url in files.items()}
    except OSError as error:
        sys.exit(f"error: cannot download from {RAW}: {error}")

    for stale in TARGET.glob("*.svg"):
        if stale.name not in files:
            stale.unlink()
            print(f"removed {stale.name}")
    for name, text in fetched.items():
        (TARGET / name).write_text(text, encoding="utf-8", newline="\n")
        print(f"wrote {name}")


if __name__ == "__main__":
    main()
