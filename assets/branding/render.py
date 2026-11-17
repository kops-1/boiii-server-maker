"""Render the branding sources in this folder into the files the project uses.

    icon.svg     -> icon.png (256 px, used for the window icon)
                    icon.ico (16-256 px, embedded into the .exe files)
    banner.html  -> banner.png (2560 x 1280, shown at the top of the README)
                    social-preview.png (1280 x 640, for GitHub's social preview)

Needs Google Chrome or Microsoft Edge (used headless to draw the SVG/HTML with
the Windows fonts) and Pillow (`pip install pillow`). Run it from anywhere:

    python assets/branding/render.py
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256]

BROWSER_CANDIDATES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]


def find_browser() -> str:
    for candidate in BROWSER_CANDIDATES:
        if os.path.isfile(candidate):
            return candidate
    for name in ("chrome", "msedge", "chromium"):
        found = shutil.which(name)
        if found:
            return found
    sys.exit("Chrome or Edge is needed to render the artwork.")


def screenshot(browser: str, page: Path, out: Path, width: int, height: int, scale: float = 1.0) -> None:
    """Draws `page` in a headless browser window of the given size and saves it as PNG."""
    with tempfile.TemporaryDirectory() as profile:
        subprocess.run(
            [
                browser,
                "--headless=new",
                "--disable-gpu",
                "--hide-scrollbars",
                "--default-background-color=00000000",
                f"--user-data-dir={profile}",
                f"--force-device-scale-factor={scale}",
                f"--window-size={width},{height}",
                f"--screenshot={out}",
                page.as_uri(),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    if not out.is_file():
        sys.exit(f"The browser did not write {out}")


def render_icon(browser: str, work: Path) -> None:
    # A wrapper page, so the SVG fills the window exactly at the size we ask for.
    wrapper = work / "icon.html"
    wrapper.write_text(
        "<!doctype html><style>html,body{margin:0;background:transparent}"
        f"img{{display:block;width:1024px;height:1024px}}</style><img src='{(HERE / 'icon.svg').as_uri()}'>",
        encoding="utf-8",
    )
    full = work / "icon-1024.png"
    screenshot(browser, wrapper, full, 1024, 1024)

    master = Image.open(full).convert("RGBA")
    frames = [master.resize((size, size), Image.Resampling.LANCZOS) for size in ICO_SIZES]
    frames[-1].save(HERE / "icon.png", optimize=True)
    frames[-1].save(HERE / "icon.ico", sizes=[(s, s) for s in ICO_SIZES], append_images=frames[:-1])


def render_banner(browser: str, work: Path) -> None:
    big = work / "banner@2x.png"
    screenshot(browser, HERE / "banner.html", big, 1280, 640, scale=2)

    banner = Image.open(big).convert("RGB")
    banner.save(HERE / "banner.png", optimize=True)
    banner.resize((1280, 640), Image.Resampling.LANCZOS).save(HERE / "social-preview.png", optimize=True)


def main() -> None:
    browser = find_browser()
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        render_icon(browser, work)
        render_banner(browser, work)
    for name in ("icon.png", "icon.ico", "banner.png", "social-preview.png"):
        size_kb = (HERE / name).stat().st_size / 1024
        print(f"{name:20} {size_kb:7.1f} KB")


if __name__ == "__main__":
    main()
