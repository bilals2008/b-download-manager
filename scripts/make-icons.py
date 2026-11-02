"""Pack the application logo into the icons the app and the tray need.

The logo is the brand mark: a "B" whose left half is the green of the tray and
whose right half is the charcoal of the bar, with a white arrow falling into
it. It is drawn once by hand and kept in `src-tauri/icons/source.png`; this
script only resizes and packs it, so the app icon and the tray icon are the
same picture rather than a second interpretation of it.

    python scripts/make-icons.py

It writes, under `src-tauri/icons/`:

* `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.ico` — the application,
* `tray-on-dark.png` — the logo with a light rim, for a dark taskbar,
* `tray-on-light.png` — the logo as it is, for a light one.

The tray in `src-tauri/src/tray.rs` picks one of the two when the reader has
switched on "Dark Mode support"; without it the dark drawing is used.
"""

from pathlib import Path

from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "src-tauri" / "icons"
SOURCE = ICONS / "source.png"

#: The tray is drawn four times larger and shrunk back, so a one-pixel rim at
#: 128 is still a rim at 32.
SUPERSAMPLE = 4
TRAY_SIZE = 32

#: The rim that keeps the charcoal half of the logo apart from a dark taskbar.
RIM = (236, 240, 236, 150)

ICO_SIZES = [(size, size) for size in (16, 24, 32, 48, 64, 128, 256)]


def logo(size: int) -> Image.Image:
    """The mark at one size, shrunk with Lanczos so an edge stays an edge."""
    with Image.open(SOURCE) as source:
        return source.convert("RGBA").resize((size, size), Image.LANCZOS)


def with_rim(mark: Image.Image, radius: int) -> Image.Image:
    """The mark laid over a grown copy of its own silhouette."""
    if radius <= 0:
        return mark
    alpha = mark.getchannel("A").filter(ImageFilter.MaxFilter(radius * 2 + 1))
    canvas = Image.new("RGBA", mark.size, (0, 0, 0, 0))
    canvas.paste(Image.new("RGBA", mark.size, RIM), (0, 0), alpha)
    canvas.paste(mark, (0, 0), mark)
    return canvas


def main() -> None:
    if not SOURCE.exists():
        raise SystemExit(f"the logo is missing: {SOURCE}")
    ICONS.mkdir(parents=True, exist_ok=True)

    logo(32).save(ICONS / "32x32.png")
    logo(128).save(ICONS / "128x128.png")
    logo(256).save(ICONS / "128x128@2x.png")
    logo(256).save(ICONS / "icon.ico", format="ICO", sizes=ICO_SIZES)

    for surface, rim in (("dark", 1), ("light", 0)):
        large = logo(TRAY_SIZE * SUPERSAMPLE)
        large = with_rim(large, rim * SUPERSAMPLE)
        large.resize((TRAY_SIZE, TRAY_SIZE), Image.LANCZOS).save(
            ICONS / f"tray-on-{surface}.png"
        )

    print("icons written to", ICONS)


if __name__ == "__main__":
    main()
