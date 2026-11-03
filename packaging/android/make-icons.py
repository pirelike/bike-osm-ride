#!/usr/bin/env python3
"""Android launcher icons from the desktop icon.

`dx bundle` (0.7.10) always writes its own Dioxus-template icons into the generated Gradle project,
so the app's icon cannot come from `Dioxus.toml`; `packaging/android/resoverlay.sh` drops these
files beside them (and deletes the template ones) and repackages the APK.

Input: `packaging/icons/1024.png` — the dark rounded tile with the accent ridge, the same art the
desktop window icon is drawn from (`crates/ride-app/src/icon.rs`).

Output (in `packaging/android/res/`):
  mipmap-<density>/ic_launcher.webp              legacy launcher icon (48/72/96/144/192 px)
  mipmap-<density>/ic_launcher_foreground.webp   adaptive foreground: the ridge glyph alone on a
                                                108 dp canvas, inside the 72 dp safe zone

The file names and the format are deliberately **the same as the template icons** `dx` writes
(`ic_launcher.webp`): the overlay just overwrites those files, so a reused Gradle project never ends
up with two resources of the same name.
"""
from __future__ import annotations

import sys
from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
SOURCE = ROOT / "packaging" / "icons" / "1024.png"
OUT = HERE / "res"

# density name -> legacy icon size in px (mdpi 48 = 48 dp)
LEGACY = {
    "mdpi": 48,
    "hdpi": 72,
    "xhdpi": 96,
    "xxhdpi": 144,
    "xxxhdpi": 192,
}
# adaptive foreground canvas is 108 dp; the glyph may use the inner 72 dp
FOREGROUND_DP = 108
GLYPH_SHARE = 0.62  # of the canvas, so the ridge sits inside the launcher mask


def save_webp(image: Image.Image, path: Path) -> None:
    """Lossless, so the flat shapes keep their crisp edges at icon size."""
    image.save(path, format="WEBP", lossless=True, quality=100, method=6)


def glyph_layer(icon: Image.Image) -> Image.Image:
    """The accent ridge alone, cropped to its bounding box, with transparent surroundings."""
    icon = icon.convert("RGBA")
    pixels = icon.load()
    width, height = icon.size
    mask = Image.new("L", icon.size, 0)
    out = mask.load()
    for y in range(height):
        for x in range(width):
            r, g, b, _ = pixels[x, y]
            # the accent is a light teal; the tile is a dark blue-grey
            if g > 150 and b > 130 and r < 150:
                out[x, y] = 255
    box = mask.getbbox()
    if box is None:
        raise SystemExit("no accent glyph found in the source icon")
    # only the glyph keeps its pixels: whatever sat above the ridge inside the box was the tile
    glyph = icon.copy()
    glyph.putalpha(mask)
    return glyph.crop(box)


def main() -> int:
    if not SOURCE.exists():
        print(f"missing source icon: {SOURCE}", file=sys.stderr)
        return 1
    icon = Image.open(SOURCE).convert("RGBA")
    if icon.width != icon.height:
        print("the source icon must be square", file=sys.stderr)
        return 1
    glyph = glyph_layer(icon)

    for density, size in LEGACY.items():
        folder = OUT / f"mipmap-{density}"
        folder.mkdir(parents=True, exist_ok=True)
        tile = icon.resize((size, size), Image.LANCZOS)
        save_webp(tile, folder / "ic_launcher.webp")

        # adaptive foreground: the canvas is 108 dp, the glyph is drawn inside the safe zone
        canvas_px = round(size * FOREGROUND_DP / 48)
        canvas = Image.new("RGBA", (canvas_px, canvas_px), (0, 0, 0, 0))
        target_w = round(canvas_px * GLYPH_SHARE)
        scale = target_w / glyph.width
        target = glyph.resize(
            (target_w, max(1, round(glyph.height * scale))), Image.LANCZOS
        )
        canvas.alpha_composite(
            target,
            ((canvas_px - target.width) // 2, (canvas_px - target.height) // 2),
        )
        save_webp(canvas, folder / "ic_launcher_foreground.webp")
        print(f"{density}: {size}px tile, {canvas_px}px foreground")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
