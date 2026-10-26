"""Verify AE-rendered fixtures; requires Pillow and NumPy."""
from pathlib import Path

import numpy as np
from PIL import Image


OUTPUT = Path(__file__).resolve().parents[3] / "target" / "curve-smoke"


def pixels(name):
    return np.asarray(Image.open(OUTPUT / f"{name}.png").convert("RGBA")).astype(float)


def main():
    assert "PASS" in (OUTPUT / "result.txt").read_text(encoding="utf-8-sig")
    for bpc in (8, 16, 32):
        source = pixels(f"source-{bpc}")
        for direction in ("forward", "inverse"):
            error = np.abs(source - pixels(f"straight-{direction}-{bpc}"))
            assert error.mean() < 0.02 and error.max() <= 2, (bpc, direction, error.mean())
    assert not pixels("missing-path").any()
    for name, factor in (("straight-half", (2, 2)), ("straight-anisotropic", (2, 4))):
        image = pixels(name)
        expected = pixels("source-8")[::factor[1], ::factor[0]]
        assert image.shape == expected.shape
        assert np.abs(image - expected).max() <= 1
    assert np.array_equal(pixels("par-source"), pixels("par-fill"))
    assert np.array_equal(pixels("par-fill"), pixels("par-contain"))
    assert pixels("animated-1")[..., 1].mean() - pixels("animated-0")[..., 1].mean() > 80
    assert np.all(pixels("ring-loop")[..., 3] == 255)
    assert 0.74 < (pixels("ring-no-loop")[..., 3] > 0).mean() < 0.76
    assert not pixels("ring-contain")[:80].any()
    assert np.all(pixels("ring-cover")[..., 3] == 255)
    ring = pixels("ring-inverse")
    assert 0.22 < (ring[..., 3] > 0).mean() < 0.24
    assert np.abs(ring - pixels("ring-flipped")).mean() > 5
    shifted = pixels("ring-post-transform")
    assert np.array_equal(shifted[:, 20:], ring[:, :-20])
    assert not shifted[:, :20].any()
    assert (pixels("bezier-inverse")[..., 3] > 0).mean() > 0.2
    print("PASS: 8/16/32bpc, straight round trips, downsampling, PAR, animation, loop, fit, flips, Post Transform, Bezier")


if __name__ == "__main__":
    main()
