"""Compare collapsed/adjustment fixtures against AE native paths; needs Pillow."""

import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageFilter


def verify(directory):
    def alpha(name):
        with Image.open(directory / f"{name}.png") as image:
            return image.getchannel("A")

    names = [
        "collapsed-transformed", "collapsed-half", "collapsed-nested",
        "collapsed-stretched", "collapsed-remapped", "collapsed-visibility",
        "collapsed-solo", "adjustment-shapes", "adjustment-half", "cache-after",
    ]
    failures = []
    for name in names:
        actual = alpha(name).point(lambda x: 255 if x >= 64 else 0)
        native = alpha(name + "-native").point(lambda x: 255 if x >= 64 else 0)
        stray = ImageChops.subtract(actual, native.filter(ImageFilter.MaxFilter(5)))
        missing = ImageChops.subtract(native, actual.filter(ImageFilter.MaxFilter(5)))
        bad = sum(stray.histogram()[1:]) + sum(missing.histogram()[1:])
        if not actual.getbbox() or not native.getbbox() or bad:
            failures.append(f"{name}: {bad} mismatched contour pixels; actual {actual.getbbox()}, native {native.getbbox()}")
        else:
            print(f"PASS {name}: contour within 2 pixels of native stroke")
    if not ImageChops.difference(alpha("cache-before"), alpha("cache-after")).getbbox():
        failures.append("External path edit reused the previous cached output")
    if failures:
        raise AssertionError("\n".join(failures))
    print("PASS external path edits update without cache purge")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("Usage: python verify_ae_collapsed.py <fixture-output-directory>")
    verify(Path(sys.argv[1]))
