"""Validate actual AE dynamics renders. Run after ae_dynamics.jsx; needs Pillow."""

import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageFilter


def verify(directory):
    def rgba(name):
        with Image.open(directory / f"{name}.png") as image:
            return image.convert("RGBA")

    def alpha(name):
        return rgba(name).getchannel("A")

    def equal(a, b):
        return not any(c.getbbox() for c in ImageChops.difference(rgba(a), rgba(b)).split())

    def height(name, x):
        a = alpha(name)
        box = a.crop((x, 0, x + 1, a.height)).getbbox()
        return box[3] - box[1] if box else 0

    assert equal("random-early", "random-late"), "Random frames drift over time"
    assert equal("random-owner-early", "random-owner-late"), "Owner timing changes random frames"
    assert not equal("random-early", "random-seed"), "Random seed has no effect"
    palette = {(255, 0, 0), (0, 255, 0), (0, 0, 255), (255, 255, 0)}
    for name in ("random-early", "random-late", "random-seed", "random-stretched", "random-reverse", "random-owner-late"):
        im = rgba(name)
        pixels = [im.getpixel((x, 120)) for x in range(30, 290)]
        assert all(p[3] == 255 and p[:3] in palette for p in pixels), f"{name}: blank/invalid frames"
        assert len({p[:3] for p in pixels}) >= 3, f"{name}: missing source-frame variation"
    im = rgba("random-trimmed")
    trimmed = {im.getpixel((x, 120)) for x in range(30, 290)}
    assert trimmed == {(0, 255, 0, 255), (0, 0, 255, 255)}, "Trimmed range leaks excluded frames"
    im = rgba("random-reverse-short")
    assert all(im.getpixel((x, 120)) == (255, 255, 0, 255) for x in range(30, 290)), "Subframe reverse trim samples outside its range"
    print("PASS fixed random assignment, seed, valid source range, trim and stretch")

    assert height("map-size", 60) == height("map-size", 250) == 0
    assert 4 <= height("map-size", 110) <= 7
    assert 10 <= height("map-size", 160) <= 13
    assert 15 <= height("map-size", 210) <= 19
    for name in ("map-size-shift", "map-parent"):
        assert height(name, 110) == 0
        assert 4 <= height(name, 150) <= 7
        assert 10 <= height(name, 200) <= 13
        assert 15 <= height(name, 250) <= 19
    assert height("map-size-scale", 60) > 0 and height("map-size-scale", 250) > 0
    assert equal("map-size", "map-alpha"), "Equivalent alpha/luminance stripes differ"
    assert equal("map-mask-none", "map-mask-cropped"), "Mask crop shifts absolute map coordinates"
    print("PASS absolute map position, scale, parenting, alpha, and cropped owner")

    for full, half in (("map-size", "map-size-half"), ("map-mask-cropped", "map-mask-half")):
        a = alpha(full).point(lambda x: 255 if x >= 64 else 0)
        b = alpha(half).resize(a.size, Image.Resampling.NEAREST).point(lambda x: 255 if x >= 64 else 0)
        stray = ImageChops.subtract(a, b.filter(ImageFilter.MaxFilter(7)))
        missing = ImageChops.subtract(b, a.filter(ImageFilter.MaxFilter(7)))
        assert not stray.getbbox() and not missing.getbbox(), f"{half}: shifted/downsampled map"
    assert 100 < alpha("map-opacity").getpixel((110, 120)) < 230
    assert alpha("map-opacity").getpixel((160, 120)) == 255
    assert height("map-rotation", 110) == 24 and height("map-rotation", 160) == 12
    im = alpha("map-density")
    row = [im.getpixel((x, 120)) for x in range(30, 290)]
    assert 0 < sum(v > 128 for v in row) < 80, "Density map should produce sparse stamps"
    assert any(row[50:110]) and any(row[150:210]), "Density must resume beyond zero regions"
    print("PASS map resolution, opacity, rotation, and density")

    assert 10 <= height("length-neutral", 160) <= 12
    assert 22 <= height("length-double", 160) <= 24
    assert height("crowding", 80) == 0 and height("crowding", 200) > 0
    assert not alpha("curvature").crop((100, 86, 280, 94)).getbbox(), "Straight path has curvature"
    assert alpha("curvature").crop((100, 145, 220, 180)).getbbox(), "Curved path missing"
    print("PASS individual length, neighboring paths, and curvature")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("Usage: python verify_ae_dynamics.py <fixture-output-directory>")
    verify(Path(sys.argv[1]))
