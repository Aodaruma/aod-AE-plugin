"""Compare ImagePattern's AE renders with reference frames. Requires Pillow."""
from pathlib import Path
import math
from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parents[3] / "target"
EXTENDED = ROOT / "ae-image-pattern-extended"
SMOKE = ROOT / "ae-image-pattern-smoke"


def image(folder, name):
    return Image.open(folder / (name + ".png")).convert("RGBA")


def verify():
    for folder in (SMOKE, EXTENDED):
        assert "RESULT=PASS" in (folder / "report.txt").read_text(encoding="utf-8-sig")
    comparisons = 0
    for depth in (8, 16, 32):
        def check(name, point, reference):
            nonlocal comparisons
            actual = image(EXTENDED, f"{name}-{depth}").getpixel(point)
            expected = image(EXTENDED, f"source-{reference}-{depth}").getpixel((128, 96))
            assert max(abs(a-b) for a, b in zip(actual, expected)) <= 2, (depth, name, point, actual, expected)
            comparisons += 1

        for name, point, reference in [
            ("xy-time", (128, 136), 5), ("xy-time", (128, 96), 4),
            ("xy-time-origin", (128, 136), 4), ("xy-time-origin", (128, 96), 3),
            ("overlap-time-on", (28, 111), 5), ("overlap-time-off", (28, 111), 4),
            ("overlap-time-origin-off", (28, 111), 3),
            ("boundary-time", (168, 146), 5), ("boundary-time", (168, 106), 4),
            ("boundary-time-origin", (168, 146), 4), ("boundary-time-origin", (168, 106), 3),
            ("time-strip", (128, 96), 4), ("time-strip", (128, 144), 5),
            ("time-strip", (128, 48), 3), ("time-strip-origin", (128, 144), 4),
            ("time-strip-origin", (128, 96), 3), ("time-strip-negative", (128, 144), 3),
            ("time-tile-origin", (192, 96), 4), ("time-tile-origin", (128, 96), 3),
            ("time-polar", (192, 96), 4), ("time-polar", (128, 144), 5),
            ("time-feedback-front", (128, 96), 6), ("time-feedback-back", (128, 96), 4),
            ("time-tile-2-1", (192, 144), 5), ("time-tile-2-2", (192, 96), 5),
            ("time-tile-2-2", (64, 96), 3), ("time-tile-2-3", (128, 144), 5),
            ("time-tile-2-3", (128, 48), 3),
        ]:
            check(name, point, reference)
        for shape in (1, 2, 3):
            for axis in (1, 2, 3):
                name = f"time-tile-{shape}-{axis}"
                check(name, (128, 96), 4)
                assert image(EXTENDED, f"{name}-{depth}").getchannel("A").getextrema() == (255, 255)

    for depth in (8, 16, 32):
        assert image(EXTENDED, f"boundary-strip-mirror-{depth}").getchannel("A").getextrema() == (255, 255)

    for depth in (8, 16, 32):
        assert ImageChops.difference(image(EXTENDED, f"boundary-strip-mirror-{depth}"), image(EXTENDED, f"overlap-strip-off-{depth}")).convert("RGB").getbbox()

    for case, reference in [("held", "last"), ("loop", "loop"), ("ping", "ping"), ("transparent", "loop")]:
        actual = image(EXTENDED, f"time-edge-{case}-16").getpixel((128, 96))
        expected = image(EXTENDED, f"source-{reference}-16").getpixel((128, 96))
        assert max(abs(a-b) for a, b in zip(actual, expected)) <= 2, (case, actual, expected)
        comparisons += 1

    empty = image(EXTENDED, "time-empty-current-16")
    assert empty.getpixel((128, 96))[3] == 0
    assert empty.getpixel((128, 144))[3] == 255
    half = image(EXTENDED, "time-half-resolution-16")
    point = (int(half.width * 0.5), int(half.height * 0.75))
    actual = half.getpixel(point)
    expected = image(EXTENDED, "source-5-16").getpixel((128, 96))
    assert max(abs(a-b) for a, b in zip(actual, expected)) <= 2, ("half", half.size, actual, expected)

    for shape in (1, 2, 3):
        images = [image(EXTENDED, f"affect-tile-{shape}-{affect}") for affect in (0, 50, 100)]
        assert all(im.getchannel("A").getextrema() == (255, 255) for im in images)
        assert all(ImageChops.difference(images[n], images[n+1]).convert("RGB").getbbox() for n in (0, 1))
    assert ImageChops.difference(image(EXTENDED, "edge-along-strip"), image(EXTENDED, "edge-image-axes")).convert("RGB").getbbox()
    assert not ImageChops.difference(image(SMOKE, "mix-zero"), image(SMOKE, "source")).getbbox()
    print(f"PASS: {comparisons} temporal reference comparisons; temporal edges, transparent current input, half resolution, polygon opacity, Affect variants, directional edges, and Mix=0.")
    verify_xy()


def verify_xy():
    comparisons = 0
    points = [(x, y) for x in (76, 140, 204) for y in (12, 52, 92, 132, 172)]
    for depth in (8, 16, 32):
        source = image(EXTENDED, f"xy-source-{depth}")
        for name, dx, dy, mode, mirror, vertical in [
            ("zero", 0, 0, 1, False, False), ("x", 12, 0, 1, False, False),
            ("y", 0, -8, 1, False, False), ("mode-1", 12, -8, 1, False, False),
            ("mode-2", 12, -8, 2, False, False), ("mode-3", 12, -8, 3, False, False),
            ("mode-4", 12, -8, 4, False, False), ("mirror", 12, -8, 1, True, False),
            ("angle", 12, -8, 1, False, True),
        ]:
            actual_image = image(EXTENDED, f"xy-{name}-{depth}")
            for x, y in points:
                index = math.floor(((128 - x) if vertical else (y - 96)) / 40 + 0.5)
                step = {1: index, 2: -index, 3: abs(index), 4: min(index % 4, 4 - index % 4)}[mode]
                sx, sy = (x + 40 * index, y) if vertical else (x, y - 40 * index)
                if mirror and index % 2:
                    sy = 192 - sy
                sx, sy = sx - dx * step, sy - dy * step
                expected = source.getpixel((sx, sy)) if 0 <= sx < 256 and 0 <= sy < 192 else (0, 0, 0, 0)
                actual = actual_image.getpixel((x, y))
                assert max(abs(a-b) for a, b in zip(actual, expected)) <= 2, (depth, name, (x, y), (sx, sy), actual, expected)
                comparisons += 1
        for split in (0, 1):
            original = image(EXTENDED, f"xy-mask-zero-{split}-{depth}")
            shifted = image(EXTENDED, f"xy-mask-pan-{split}-{depth}")
            assert not ImageChops.difference(original.getchannel("A"), shifted.getchannel("A")).getbbox()
            assert ImageChops.difference(original, shifted).convert("RGB").getbbox()
        a = image(EXTENDED, f"xy-random-{depth}")
        repeat = image(EXTENDED, f"xy-random-repeat-{depth}")
        other = image(EXTENDED, f"xy-random-other-{depth}")
        assert not ImageChops.difference(a, repeat).convert("RGB").getbbox()
        assert ImageChops.difference(a, other).convert("RGB").getbbox()
    print(f"PASS: {comparisons} Delta X/Y source-pixel comparisons; both overlap masks unchanged, deterministic random seeds, Mirror, and rotated strips.")


if __name__ == "__main__":
    verify()
