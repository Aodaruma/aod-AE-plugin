"""Compare opened strips against source pixels and premultiplied compositing."""
from functools import lru_cache
from pathlib import Path
import math
from PIL import Image

ROOT = Path(__file__).resolve().parents[3] / "target/ae-image-pattern-clipping"


@lru_cache(maxsize=128)
def image(name):
    return Image.open(ROOT / f"{name}.png").convert("RGBA")


def expected(point, depth, clipping, order, repeat=1, copies=1, time_step=0, origin=0, edge="held", crossing=None):
    x, y = point
    # (repeat number, lower edge, upper edge) in coordinates relative to Center.
    if crossing is None:
        bands = [(i, (i-.5)*40, (i+.5)*40) for i in range(-copies, copies+1)]
        if clipping == 1:
            index = math.floor((y-96)/40 + .5)
            bands = [(index, (index-.5)*40, (index+.5)*40)]
    else:
        height = 60 + (x-128)*.5
        if crossing:
            edges = sorted([-height, -20, 20, height])
            bands = [(i-1, edges[i], edges[i+1]) for i in range(3) if edges[i+1] > edges[i]]
        else:
            bands = [(0, -20, 20)]
            if height > 20:
                bands = [(-1, -height, -20)] + bands + [(1, 20, height)]
    if order == 2:
        bands.reverse()
    rgb, alpha = [0., 0., 0.], 0.
    for index, low, high in bands:
        inside = low <= y-96 < high
        if clipping == 1 and not inside or clipping == 2 and y-96 < low or clipping == 3 and y-96 >= high:
            continue
        sy = y - 96 - index*40 if crossing is None else ((y-96-low)/(high-low)-.5)*40
        if repeat == 2 and index % 2:
            sy = -sy
        sx, sy = x - 12*index, 96 + sy + 8*index
        sx, sy = math.floor(sx+.5), math.floor(sy+.5)
        if not (0 <= sx < 256 and 0 <= sy < 192):
            continue
        time = 2 + (index-origin)*time_step
        if edge == "loop":
            time %= 5
        if time >= 5-1/24:
            ref = "last"
        else:
            ref = str(round(max(0, time)*2))
        color = image(f"source-{ref}-{depth}").getpixel((sx, sy))
        a = color[3] / 255
        # AE saveFrameToPng writes premultiplied reference RGB.
        rgb = [color[c]/255 + rgb[c]*(1-a) for c in range(3)]
        alpha = a + alpha*(1-a)
    return tuple(round(v*255) for v in rgb) + (round(alpha*255),)


def verify():
    assert "RESULT=PASS" in (ROOT / "report.txt").read_text(encoding="utf-8-sig")
    points = [(x, y) for x in (28, 76, 140, 204) for y in (12, 52, 92, 132, 172)]
    comparisons = 0
    for depth in (8, 16, 32):
        for order in (1, 2):
            native = image(f"native-{order}-{depth}")
            plugin = image(f"static-1-4-{order}-{depth}")
            for point in points:
                assert max(abs(a-b) for a, b in zip(native.getpixel(point), plugin.getpixel(point))) <= 4, ("native", depth, order, point, native.getpixel(point), plugin.getpixel(point))
        cases = []
        for repeat in (1, 2):
            for clip in range(1, 5):
                for order in (1, 2):
                    cases.append((f"static-{repeat}-{clip}-{order}", dict(clipping=clip, order=order, repeat=repeat)))
        for clip in (2, 3, 4):
            for order in (1, 2):
                cases.append((f"time-{clip}-{order}", dict(clipping=clip, order=order, time_step=.5)))
        for order in (1, 2):
            for name, controls in [
                ("negative", dict(time_step=-.5)), ("origin", dict(time_step=.5, origin=1)),
                ("held", dict(copies=2, time_step=5)), ("loop", dict(copies=2, time_step=2.5, edge="loop")),
            ]:
                cases.append((f"{name}-{order}", dict(clipping=4, order=order, **controls)))
        for split in (0, 1):
            for clip in (2, 3, 4):
                for order in (1, 2):
                    cases.append((f"cross-{split}-{clip}-{order}", dict(clipping=clip, order=order, time_step=.5, crossing=bool(split))))
        for name, controls in cases:
            rendered = image(f"{name}-{depth}")
            for point in points:
                actual = rendered.getpixel(point)
                reference = expected(point, depth, **controls)
                assert max(abs(a-b) for a, b in zip(actual, reference)) <= 4, (depth, name, point, actual, reference)
                comparisons += 1
    print(f"PASS: {comparisons} clipping/compositing comparisons; four clipping modes, Over/Under, alpha, Mirror, crossings, time origins, negative time and repeated held/looped frames.")


if __name__ == "__main__":
    verify()
