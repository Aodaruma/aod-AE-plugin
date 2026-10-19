"""Verify AE scanline samples and PNGs for depth, alpha, height and refraction."""
from pathlib import Path
import sys

import numpy as np
from PIL import Image


root = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parents[3] / "target/image-bevel-smoke"


def read(name):
    return np.loadtxt(root / f"{name}.csv", delimiter=",", encoding="utf-8-sig")


height = read("height-32")
assert height[128, 0] > 0.99, "中心の高さが1ではない"
assert height[32, 0] < 0.01, "内側生成が輪郭の外側へ漏れている"
assert 0.1 < height[68, 0] < 0.95, "ベベルの途中の高さが不正"
for depth in (8, 16):
    other = read(f"height-{depth}")
    assert np.max(np.abs(height - other)) < 0.025, f"{depth}bpcと32bpcが一致しない"
    assert np.max(np.abs(np.clip(read("lit-32"), 0, 1) - read(f"lit-{depth}"))) < 0.035

original, curve, none = [read("alpha-" + name) for name in ("original", "curve", "none")]
assert original[55, 3] == 0
assert curve[55, 3] > 0.9 and curve[20, 3] == 0
# sampleImage's last box straddles the layer edge; verify the PNG's full alpha too.
assert np.min(none[:-1, 3]) == 1
assert np.asarray(Image.open(root / "alpha-none.png"))[:, :, 3].min() == 255
assert 0.3 < read("direction-both")[64, 0] < 0.99
assert read("offset")[58, 0] > 0.1

assert np.max(np.abs(read("neumann") - read("dirichlet"))) > 0.01
normal = read("normal")[128, :3]
assert np.max(np.abs(normal - [0.5, 0.5, 1.0])) < 0.01
assert read("emission")[128, 0] > read("emission")[128, 1] > 0
assert read("emission-hdr")[128, 0] > 2.99, "HDRの発光がクリップされている"
assert np.mean(np.abs(read("refraction-ior1") - read("refraction-ior15"))) > 0.005
report = (root / "report.txt").read_text(encoding="utf-8-sig")
assert "COMPLETE" in report and "ERROR" not in report
demo = np.asarray(Image.open(root / "demo.png").convert("RGB"))
assert demo.max() > 128 and np.mean(demo) > 5
print("PASS: AE出力の深度・アルファ3方式・内外方向・Poisson境界・法線・発光・屈折")
