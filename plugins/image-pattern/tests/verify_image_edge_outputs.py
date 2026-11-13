"""Compare image-edge renders with independently padded source frames (Pillow)."""
import math
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parents[3] / "target/ae-image-pattern-edge"
POINTS = [(x, y) for x in (0, 8, 47, 48, 79, 80, 81, 120, 127) for y in (12, 48, 84)]


def rounded(x):
    return int(math.copysign(math.floor(abs(x) + 0.5), x))


def sample(source, x, y, edge, direction, sampling):
    width, height = source.size
    if direction == 1 and edge in (2, 3, 4):
        if y < 0 or y > height - 1:
            return [0.0] * 4
        if x < 0 or x > width - 1:
            if edge == 2:
                x = min(max(x, 0), width - 1)
            elif edge == 3:
                x %= width - 1
            else:
                x %= 2 * (width - 1)
                x = min(x, 2 * (width - 1) - x)
        edge = 2

    def resolve(v, n):
        if edge == 2:
            return min(max(v, 0), n - 1)
        if edge == 3:
            return v % n
        if edge == 4:
            v %= 2 * (n - 1)
            return min(v, 2 * (n - 1) - v)
        return v if 0 <= v < n else None

    def tap(x, y):
        x, y = resolve(x, width), resolve(y, height)
        if x is None or y is None:
            return [0.0, 0.0, 0.0, 1.0 if edge == 5 else 0.0]
        r, g, b, a = source.getpixel((x, y))
        return [r / a, g / a, b / a, a / 255] if a else [0.0] * 4

    if sampling == 1:
        return tap(rounded(x), rounded(y))
    left, top = math.floor(x), math.floor(y)
    tx, ty = x - left, y - top
    result = [0.0] * 4
    for dx, dy, weight in ((0, 0, (1-tx)*(1-ty)), (1, 0, tx*(1-ty)),
                           (0, 1, (1-tx)*ty), (1, 1, tx*ty)):
        pixel = tap(left + dx, top + dy)
        result = [a + b * weight for a, b in zip(result, pixel)]
    return result


def check(actual, expected, label):
    rgba = [round(c * expected[3] * 255) for c in expected[:3]] + [round(expected[3] * 255)]
    assert max(abs(a-b) for a, b in zip(actual, rgba)) <= 4, (label, actual, rgba)


def verify():
    assert "RESULT=PASS" in (ROOT / "report.txt").read_text(encoding="utf-8-sig")
    comparisons = 0
    for depth in (8, 16, 32):
        sources = {i: Image.open(ROOT / f"source-{i}-{depth}.png").convert("RGBA") for i in (-1, 0, 1)}
        for sampling in (1, 2):
            delta = 80 if sampling == 1 else 80.25
            for time in (0, 1):
                for direction in (1, 2):
                    for edge in range(1, 6):
                        name = f"edge-{edge}-{direction}-{sampling}-{time}-{depth}"
                        actual = Image.open(ROOT / f"{name}.png").convert("RGBA")
                        for x, y in POINTS:
                            index = math.floor((y - 48) / 32 + 0.5)
                            expected = sample(sources[index * time], x-delta*index, y-32*index, edge, direction, sampling)
                            check(actual.getpixel((x, y)), expected, (name, x, y))
                            comparisons += 1
        for time in (0, 1):
            for order in (1, 2):
                name = f"overlap-{order}-{time}-{depth}"
                actual = Image.open(ROOT / f"{name}.png").convert("RGBA")
                for x, y in POINTS:
                    expected = [0.0] * 4
                    for index in ((-1, 0, 1) if order == 1 else (1, 0, -1)):
                        front = sample(sources[index*time], x-80*index, y-32*index, 5, 2, 1)
                        back_weight = expected[3] * (1-front[3])
                        alpha = front[3] + back_weight
                        rgb = [(front[c]*front[3] + expected[c]*back_weight)/alpha for c in range(3)] if alpha else [0.0]*3
                        expected = rgb + [alpha]
                    check(actual.getpixel((x, y)), expected, (name, x, y))
                    comparisons += 1
    print(f"PASS: {comparisons} image-edge comparisons; five modes, both directions/filters, Time Delta, transparency and Over/Under.")


if __name__ == "__main__":
    verify()
