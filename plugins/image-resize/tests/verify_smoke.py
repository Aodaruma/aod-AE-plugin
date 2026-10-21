"""Verify real AE renders; requires Pillow and a completed smoke.jsx run."""
from pathlib import Path

from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parents[3]
OUTPUT = ROOT / "target" / "ae-smoke"


def load(name):
    return Image.open(OUTPUT / f"{name}.png").convert("RGBA")


def same(a, b):
    assert ImageChops.difference(a, b).getbbox(alpha_only=False) is None


def verify():
    report = (OUTPUT / "report.txt").read_text(encoding="utf-8-sig")
    assert "PASS: all cases rendered" in report and "FAIL:" not in report, report
    checks = 0
    for depth in (8, 16, 32):
        baseline = load(f"baseline-{depth}")
        assert baseline.getchannel("A").getbbox() == (60, 60, 140, 100)
        for name in ("identity", "all0", "none"):
            same(baseline, load(f"{name}-{depth}"))
            checks += 1
        same(load(f"crop-{depth}"), load(f"pixels-{depth}"))
        same(load(f"rotate-{depth}"), load(f"rotate_all0-{depth}"))
        checks += 2
        for name, expected in {
            "crop": (80, 70, 140, 100),
            "all50": (70, 65, 140, 100),
            "mirror": (40, 50, 160, 110),
            "tile": (40, 50, 160, 110),
            "clamp": (40, 50, 160, 110),
            "rotate": (80, 40, 120, 120),
            "empty": None,
        }.items():
            actual = load(f"{name}-{depth}").getchannel("A").getbbox()
            assert actual == expected, (name, depth, actual, expected)
            checks += 1
        # PNG export can apply the host's HDR viewing transform. Compare with
        # unprocessed source pixels at the same bit depth, not assumed RGB codes.
        red = baseline.getpixel((65, 65))
        yellow = baseline.getpixel((125, 65))
        for name, left, right in (
            ("mirror", red, yellow),
            ("tile", yellow, red),
            ("clamp", red, yellow),
        ):
            img = load(f"{name}-{depth}")
            assert img.getpixel((50, 65)) == left, (name, depth, "left", img.getpixel((50, 65)))
            assert img.getpixel((150, 65)) == right, (name, depth, "right", img.getpixel((150, 65)))
            checks += 2
        alpha = load(f"fractional-{depth}").getpixel((60, 65))[3]
        assert abs(alpha - 191) <= 1, (depth, alpha)
        checks += 1
        # Compare entire windows with the original pixels: a scaled image could
        # have correct dimensions but would fail these pixel-for-pixel checks.
        same(baseline, load(f"size_disabled-{depth}"))
        checks += 1
        for name, box in {
            "size_crop": (80, 70, 120, 90),
            "size_shift": (90, 70, 130, 90),
            "size_all0": (80, 70, 120, 90),
            "size_odd": (79, 69, 120, 90),
        }.items():
            img = load(f"{name}-{depth}")
            assert img.getchannel("A").getbbox() == box, (name, depth, img.getchannel("A").getbbox())
            same(img.crop(box), baseline.crop(box))
            checks += 2
        for name in ("none", "mirror", "tile", "clamp", "rotate", "rotate_extend"):
            same(load(f"size_{name}-{depth}"), load(f"{name}-{depth}"))
            checks += 1
        rotated = load(f"size_rotate_crop-{depth}")
        box = (90, 50, 110, 110)
        assert rotated.getchannel("A").getbbox() == box
        same(rotated.crop(box), load(f"rotate-{depth}").crop(box))
        checks += 2
    half = load("half")
    expected = (40, 35, 70, 50) if half.size == (100, 80) else (80, 70, 140, 100)
    assert half.getchannel("A").getbbox() == expected, (half.size, half.getchannel("A").getbbox())
    checks += 1
    half_size = load("half-size")
    expected = (45, 35, 65, 45) if half_size.size == (100, 80) else (90, 70, 130, 90)
    assert half_size.getchannel("A").getbbox() == expected
    same(half_size.crop(expected), half.crop(expected))
    checks += 2
    chained = load("chained")
    assert chained.getchannel("A").getbbox() == (40, 60, 140, 100)
    assert chained.getpixel((50, 65)) == load("baseline-8").getpixel((125, 65))
    assert chained.getpixel((70, 65))[3] == 0
    assert chained.getpixel((150, 65))[3] == 0
    checks += 4
    print(f"PASS: {checks} AE image assertions; 8/16/32bpc, extension, crop, All %, rotation, exact canvas size without scaling, subpixel edges, half resolution.")


if __name__ == "__main__":
    verify()
