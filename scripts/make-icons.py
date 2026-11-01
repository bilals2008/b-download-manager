"""Draw the application mark as the icons of the app and of the tray.

The mark is the one the title bar draws: an amber arrow into a green tray.
The reference product's icon is a trademarked asset, so the clone draws its own.

    python scripts/make-icons.py

It writes `src-tauri/icons/32x32.png`, `128x128.png`, `128x128@2x.png` and
`icon.ico` for the application, and the tray icons under
`src-tauri/icons/tray-<style>-on-<surface>.png`:

* `default`  — the flat mark of the application,
* `3d`       — the same mark with a ramp on every body and a bevel around it,
* `classic`  — the same mark drawn flat under one heavy outline,

each for a light taskbar (`on-light`) and a dark one (`on-dark`). The tray in
`src-tauri/src/tray.rs` picks one of the six from the reader's choices.

All three are the same picture on the same 16 by 16 grid, so switching the
style changes how the mark is painted and never what it shows.
"""

from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "src-tauri" / "icons"

#: The mark of the application, and of the tray when nothing else is chosen.
AMBER = (242, 191, 98, 255)
GREEN = (142, 217, 74, 255)

#: The tray is drawn four times larger and shrunk back, so a one-pixel rim at
#: 128 is still a rim at 32.
SUPERSAMPLE = 4
TRAY_SIZE = 32

#: The inks of each style, for each surface the icon has to sit on.
PALETTES = {
    "default": {
        "on-dark": {"arrow": AMBER, "tray": GREEN},
        "on-light": {
            "arrow": (176, 106, 18, 255),
            "tray": (47, 122, 31, 255),
        },
    },
    "3d": {
        "on-dark": {
            "arrow_top": (255, 226, 168, 255),
            "arrow_bottom": (186, 110, 22, 255),
            "tray_top": (172, 236, 112, 255),
            "tray_bottom": (32, 100, 42, 255),
            "bevel": (74, 44, 8, 255),
            "rim": (255, 244, 214, 255),
        },
        "on-light": {
            "arrow_top": (232, 166, 56, 255),
            "arrow_bottom": (126, 68, 8, 255),
            "tray_top": (112, 192, 70, 255),
            "tray_bottom": (20, 72, 28, 255),
            "bevel": (46, 30, 6, 255),
            "rim": (255, 255, 255, 255),
        },
    },
    "classic": {
        "on-dark": {
            "arrow": AMBER,
            "tray": GREEN,
            "ink": (10, 18, 16, 255),
        },
        "on-light": {
            "arrow": (255, 214, 130, 255),
            "tray": (186, 236, 140, 255),
            "ink": (26, 32, 28, 255),
        },
    },
}


def ramp(size: int, top: tuple, bottom: tuple) -> Image.Image:
    """A vertical colour ramp, the size of the canvas."""
    strip = Image.new("RGBA", (1, size))
    pixel = strip.load()
    for y in range(size):
        t = y / (size - 1)
        pixel[0, y] = tuple(
            round(start + (end - start) * t) for start, end in zip(top, bottom)
        )
    return strip.resize((size, size))


def mask_of(size: int, draw_shape) -> Image.Image:
    """An L mask of whatever `draw_shape` paints on it."""
    mask = Image.new("L", (size, size), 0)
    draw_shape(ImageDraw.Draw(mask))
    return mask


def paint(
    canvas: Image.Image,
    mask: Image.Image,
    colour=None,
    top=None,
    bottom=None,
) -> None:
    """Put a flat colour or a vertical ramp into `canvas` through `mask`."""
    size = canvas.size[0]
    fill = (
        ramp(size, top, bottom)
        if top is not None
        else Image.new("RGBA", (size, size), colour)
    )
    canvas.paste(fill, (0, 0), mask)


def spread(mask: Image.Image, radius: int) -> Image.Image:
    """The mask grown outwards, which is what an outline needs."""
    if radius <= 0:
        return mask
    return mask.filter(ImageFilter.MaxFilter(radius * 2 + 1))


def slide(mask: Image.Image, dx: int, dy: int) -> Image.Image:
    """The mask slid aside, which is what a bevel needs."""
    moved = Image.new("L", mask.size, 0)
    moved.paste(mask, (dx, dy))
    return moved


def arrow(draw: ImageDraw.ImageDraw, scale: float, **kwargs) -> None:
    """The stem and head of the arrow."""
    draw.rounded_rectangle(
        [7 * scale, 1.4 * scale, 9 * scale, 6.4 * scale],
        radius=1.0 * scale,
        **kwargs,
    )
    draw.polygon(
        [(4.6 * scale, 4.9 * scale), (11.4 * scale, 4.9 * scale), (8 * scale, 8.9 * scale)],
        **kwargs,
    )


def tray(draw: ImageDraw.ImageDraw, scale: float, width: float, **kwargs) -> None:
    """The half-round the arrow falls into."""
    draw.arc(
        [2.4 * scale, 7.4 * scale, 13.6 * scale, 14.8 * scale],
        start=0,
        end=180,
        width=width,
        **kwargs,
    )


def bodies(size: int):
    """The two masks of the mark: the arrow, and the tray it falls into."""
    scale = size / 16
    stroke = max(1, round(2.4 * scale))
    arrow_mask = mask_of(size, lambda draw: arrow(draw, scale, fill=255))
    tray_mask = mask_of(size, lambda draw: tray(draw, scale, stroke, fill=255))
    return scale, arrow_mask, tray_mask


def mark_default(surface: str, size: int) -> Image.Image:
    """The flat mark of the application."""
    inks = PALETTES["default"][surface]
    _, arrow_mask, tray_mask = bodies(size)
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))

    paint(image, arrow_mask, colour=inks["arrow"])
    paint(image, tray_mask, colour=inks["tray"])
    return image


def mark_3d(surface: str, size: int) -> Image.Image:
    """The mark with a ramp on every body and a bevel around them."""
    inks = PALETTES["3d"][surface]
    scale, arrow_mask, tray_mask = bodies(size)
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)

    bevel = max(1, round(0.3 * scale))
    # The mark stands on a shadow, the way a raised button does.
    draw.ellipse(
        [3.4 * scale, 14.1 * scale, 12.6 * scale, 15.5 * scale],
        fill=(0, 0, 0, 60),
    )

    for mask in (arrow_mask, tray_mask):
        # Dark down and right, light up and left, then the body over both, so
        # only the two edges of the bevel are left showing.
        paint(image, slide(mask, bevel, bevel), colour=inks["bevel"])
        paint(image, slide(mask, -bevel, -bevel), colour=inks["rim"])

    paint(image, arrow_mask, top=inks["arrow_top"], bottom=inks["arrow_bottom"])
    paint(image, tray_mask, top=inks["tray_top"], bottom=inks["tray_bottom"])

    # One specular dash down the arrow, the way gloss catches a window.
    draw.line(
        [7.5 * scale, 2.1 * scale, 7.5 * scale, 4.3 * scale],
        fill=(255, 255, 255, 180),
        width=max(1, round(0.4 * scale)),
    )
    return image


def mark_classic(surface: str, size: int) -> Image.Image:
    """Flat bodies under one heavy outline, drawn the old way."""
    inks = PALETTES["classic"][surface]
    scale, arrow_mask, tray_mask = bodies(size)
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))

    # The two bodies grown together go underneath as one piece, so the stem and
    # its head share an outline instead of a seam where they meet.
    joined = ImageChops.lighter(arrow_mask, tray_mask)
    paint(image, spread(joined, max(1, round(0.45 * scale))), colour=inks["ink"])
    paint(image, arrow_mask, colour=inks["arrow"])
    paint(image, tray_mask, colour=inks["tray"])
    return image


MARKS = {
    "default": mark_default,
    "3d": mark_3d,
    "classic": mark_classic,
}


def main() -> None:
    ICONS.mkdir(parents=True, exist_ok=True)

    mark_default("on-dark", 32).save(ICONS / "32x32.png")
    mark_default("on-dark", 128).save(ICONS / "128x128.png")
    mark_default("on-dark", 256).save(ICONS / "128x128@2x.png")
    mark_default("on-dark", 256).save(
        ICONS / "icon.ico",
        format="ICO",
        sizes=[(size, size) for size in (16, 24, 32, 48, 64, 128, 256)],
    )

    for style, draw_mark in MARKS.items():
        for surface in ("dark", "light"):
            large = draw_mark(f"on-{surface}", TRAY_SIZE * SUPERSAMPLE)
            large.resize((TRAY_SIZE, TRAY_SIZE), Image.LANCZOS).save(
                ICONS / f"tray-{style}-on-{surface}.png"
            )

    print("icons written to", ICONS)


if __name__ == "__main__":
    main()
