"""Verify dimensions, alpha bounds and pixel identities in AE-rendered PNGs."""

import sys
from pathlib import Path
from PIL import Image, ImageChops


def verify(directory):
    def read(name):
        with Image.open(directory / f"{name}.png") as image:
            return image.convert("RGBA")

    def equal(a, b, label):
        diff = ImageChops.difference(a, b)
        assert not any(c.getbbox() for c in diff.split()), label

    assert "PASS all AE render calls completed" in (directory / "result.txt").read_text()
    for depth in (8, 16, 32):
        equal(read(f"input-{depth}"), read(f"identity-{depth}"), f"identity {depth}")
        equal(read(f"linked-{depth}"), read(f"separate-equal-{depth}"), f"scale link {depth}")
        assert read(f"linked-{depth}").getchannel("A").getbbox() == (20, 15, 140, 105)
        assert read(f"x-only-{depth}").getchannel("A").getbbox() == (20, 0, 140, 120)
        assert read(f"restored-{depth}").getchannel("A").getextrema() == (255, 255)
        assert read(f"flat-shrink-{depth}").getchannel("A").getbbox() == (40, 15, 120, 105)
        for kind in ("flat-restore", "flat-expand"):
            image = read(f"{kind}-{depth}")
            assert image.getchannel("A").getextrema() == (255, 255)
            assert len(image.getcolors(160 * 120)) == 1
        equal(read(f"mixed-{depth}").crop((40, 30, 120, 90)), read(f"crop-{depth}"), f"ROI {depth}")
        equal(read(f"mask-precomp-{depth}"), read(f"mask-direct-{depth}"), f"alpha bounds {depth}")
        channel_outputs = []
        for channel in range(1, 8):
            prefix = f"channel-{channel}"
            positive = read(f"{prefix}-positive-{depth}")
            assert positive.getchannel("A").getbbox() == (40, 15, 120, 105)
            equal(positive.transpose(Image.Transpose.FLIP_LEFT_RIGHT),
                  read(f"{prefix}-flip-x-{depth}"), f"channel {channel} flip X {depth}")
            equal(positive.transpose(Image.Transpose.FLIP_TOP_BOTTOM),
                  read(f"{prefix}-flip-y-{depth}"), f"channel {channel} flip Y {depth}")
            equal(positive.transpose(Image.Transpose.ROTATE_180),
                  read(f"{prefix}-flip-xy-{depth}"), f"channel {channel} flip XY {depth}")
            expanded = read(f"{prefix}-expand-restored-{depth}")
            assert expanded.size == (160, 120)
            assert expanded.getchannel("A").getextrema() == (255, 255)
            channel_outputs.append(positive.tobytes())
        assert len(set(channel_outputs)) >= 2, f"channel selector has no effect at {depth} bpc"
        # The fully opaque fixture has no alpha gradient.
        equal(read(f"channel-2-positive-{depth}"), read(f"channel-7-positive-{depth}"),
              f"RGBA equals Lightness on opaque input {depth}")
        equal(read(f"shared-negative-{depth}"), read(f"separate-negative-{depth}"),
              f"negative scale linking {depth}")
        assert read(f"zero-x-{depth}").getchannel("A").getbbox() == (79, 0, 80, 120)
        half = read(f"half-{depth}")
        assert half.size == (80, 60), f"half-resolution {depth}: {half.size}"
        print(f"PASS {depth} bpc: identity, scale link, axes, restore, expansion, ROI, masks, half resolution, 7 channels, negative scales, zero scale")


if __name__ == "__main__":
    verify(Path(sys.argv[1]))
