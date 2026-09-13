"""Compare AE-rendered contours; requires Pillow. Run after ae_smoke_test.jsx."""

import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageFilter


def verify(directory):
    def rgba(name):
        with Image.open(directory / f"{name}.png") as image:
            return image.convert("RGBA")

    def alpha(name):
        return rgba(name).getchannel("A")

    failures = []
    for name in ("shape-polygon", "shape-asymmetric-curve", "shape-translated-curve"):
        actual = alpha(name).point(lambda x: 255 if x >= 64 else 0)
        native = alpha(name + "-nativeStroke").point(lambda x: 255 if x >= 64 else 0)
        # Both directions catch missing sections as well as stray curves. A
        # small tolerance allows AE antialiasing and brush-stamp sampling.
        stray = ImageChops.subtract(actual, native.filter(ImageFilter.MaxFilter(5)))
        missing = ImageChops.subtract(native, actual.filter(ImageFilter.MaxFilter(5)))
        bad = sum(stray.histogram()[1:]) + sum(missing.histogram()[1:])
        if not actual.getbbox() or not native.getbbox() or bad:
            failures.append(f"{name}: {bad} contour pixels farther than 2 pixels from native stroke")
        else:
            print(f"PASS {name}: entire contour within 2 pixels of native stroke")

    for depth in (8, 16, 32):
        difference = ImageChops.difference(rgba(f"mask-none-{depth}"), rgba(f"mask-add-{depth}"))
        if any(channel.getbbox() for channel in difference.split()):
            failures.append(f"mask crop changes pixels at {depth} bpc")

    for name in ("zero-width", "shape-disabled"):
        if alpha(name).getbbox():
            failures.append(f"{name}: expected transparent output")

    def expect_pixel(name, xy, expected):
        actual = rgba(name).getpixel(xy)
        if any(abs(a - e) > 2 for a, e in zip(actual, expected)):
            failures.append(f"{name} at {xy}: {actual}, expected {expected}")

    # Just inside the mask, the source covers the brush only in Behind mode.
    expect_pixel("composite-front", (72, 120), (255, 255, 255, 255))
    expect_pixel("composite-behind", (72, 120), (0, 255, 0, 255))
    for name in ("composite-front", "composite-behind"):
        expect_pixel(name, (67, 120), (255, 255, 255, 255))
        expect_pixel(name, (160, 120), (0, 255, 0, 255))
        expect_pixel(name, (20, 20), (0, 0, 0, 0))

    expect_pixel("stamp-forward", (160, 120), (0, 0, 255, 255))
    expect_pixel("stamp-reverse", (160, 120), (255, 0, 0, 255))
    if ImageChops.difference(alpha("stamp-forward"), alpha("stamp-reverse")).getbbox():
        failures.append("stamp order changes stroke coverage")
    for name, color in (("time-current-red", (255, 0, 0, 255)),
                        ("time-current-blue", (0, 0, 255, 255))):
        expect_pixel(name, (160, 120), color)
    random_image = rgba("time-random")
    colors = {color for count, color in random_image.getcolors(random_image.width * random_image.height)}
    if not {(255, 0, 0, 255), (0, 0, 255, 255)}.issubset(colors):
        failures.append("random texture timing did not sample both red and blue frames")

    if failures:
        raise AssertionError("\n".join(failures))
    print("PASS mask crop equality at 8/16/32 bpc and empty/disabled strokes")
    print("PASS front/behind compositing, reversed stamp order, and current/random texture timing")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("Usage: python verify_ae_smoke.py <fixture-output-directory>")
    verify(Path(sys.argv[1]))
