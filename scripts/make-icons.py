"""Draw the application mark as the icons of the app and of the tray.

The mark is the one the title bar draws: an amber arrow into a green tray.
The reference product's icon is a trademarked asset, so the clone draws its own.

    python scripts/make-icons.py

It writes `src-tauri/icons/32x32.png`, `128x128.png`, `128x128@2x.png` and
`icon.ico`, and `src-tauri/icons/tray.png` for the tray.
"""

from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "src-tauri" / "icons"

AMBER = (242, 191, 98, 255)
GREEN = (142, 217, 74, 255)


def mark(size: int) -> Image.Image:
    """The mark on a transparent square, drawn on a 16 by 16 grid."""
    scale = size / 16
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)

    # The stem of the arrow.
    draw.rounded_rectangle(
        [7 * scale, 1.4 * scale, 9 * scale, 6.4 * scale],
        radius=1.0 * scale,
        fill=AMBER,
    )
    # The head of the arrow.
    draw.polygon(
        [(4.6 * scale, 4.9 * scale), (11.4 * scale, 4.9 * scale), (8 * scale, 8.9 * scale)],
        fill=AMBER,
    )
    # The tray the arrow falls into.
    draw.arc(
        [2.4 * scale, 7.4 * scale, 13.6 * scale, 14.8 * scale],
        start=0,
        end=180,
        fill=GREEN,
        width=max(1, round(2.4 * scale)),
    )
    return image


def main() -> None:
    ICONS.mkdir(parents=True, exist_ok=True)

    mark(32).save(ICONS / "32x32.png")
    mark(128).save(ICONS / "128x128.png")
    mark(256).save(ICONS / "128x128@2x.png")
    mark(32).save(ICONS / "tray.png")

    sizes = (16, 24, 32, 48, 64, 128, 256)
    mark(256).save(
        ICONS / "icon.ico",
        format="ICO",
        sizes=[(size, size) for size in sizes],
    )
    print("icons written to", ICONS)


if __name__ == "__main__":
    main()
