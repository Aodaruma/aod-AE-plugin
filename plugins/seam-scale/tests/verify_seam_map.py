"""Check AE map coverage and report native displacement reconstruction errors."""

import json
import struct
import sys
import zlib
from pathlib import Path
from PIL import Image, ImageChops


def rgba16(path):
    """Decode PNG filters without reducing the RGB+A samples to 8 bits."""
    data = path.read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    payload = bytearray()
    pos = 8
    while pos < len(data):
        length = int.from_bytes(data[pos:pos+4], "big")
        kind = data[pos+4:pos+8]
        block = data[pos+8:pos+8+length]
        if kind == b"IHDR":
            w, h, depth, color, compression, filtering, interlace = struct.unpack(">IIBBBBB", block)
            assert (depth, color, compression, filtering, interlace) == (16, 6, 0, 0, 0)
        elif kind == b"IDAT":
            payload.extend(block)
        pos += length + 12
    raw = zlib.decompress(payload)
    stride, bpp = w * 8, 8
    previous = bytearray(stride)
    samples = []
    for y in range(h):
        start = y * (stride + 1)
        mode = raw[start]
        row = bytearray(raw[start+1:start+1+stride])
        for x in range(stride):
            a = row[x-bpp] if x >= bpp else 0
            b = previous[x]
            c = previous[x-bpp] if x >= bpp else 0
            if mode == 0:
                predictor = 0
            elif mode == 1:
                predictor = a
            elif mode == 2:
                predictor = b
            elif mode == 3:
                predictor = (a+b)//2
            elif mode == 4:
                p = a+b-c
                pa, pb, pc = abs(p-a), abs(p-b), abs(p-c)
                predictor = a if pa <= pb and pa <= pc else b if pb <= pc else c
            else:
                raise AssertionError(f"Unknown PNG filter {mode}")
            row[x] = (row[x] + predictor) & 255
        samples.extend(struct.unpack(">"+"H"*(w*4), row))
        previous = row
    return w, h, samples


def verify(directory):
    assert "PASS all Seam Map render calls completed" in (directory / "result.txt").read_text()
    cases = ("identity", "shrink", "x-only", "flip-x", "flip-xy", "restore",
             "expand", "expand-restored", "zero")
    bounds = {"shrink": (16, 12, 48, 36), "x-only": (8, 0, 56, 48),
              "flip-x": (16, 6, 48, 42), "flip-xy": (16, 6, 48, 42),
              "zero": (31, 0, 32, 48)}
    report = []
    for depth in (8, 16, 32):
        for case in cases:
            suffix = f"{case}-{depth}"
            map_path = directory / f"map-{suffix}_00000.png"
            direct_path = directory / f"direct-{suffix}_00000.png"
            warped_path = directory / f"warped-{suffix}_00000.png"
            with Image.open(map_path) as im:
                field = im.convert("RGBA")
            with Image.open(direct_path) as im:
                direct = im.convert("RGBA")
            with Image.open(warped_path) as im:
                warped = im.convert("RGBA")
            assert field.size == (64, 48), suffix
            assert field.getchannel("A").getextrema() == (255, 255), suffix
            assert field.getchannel("B").getbbox() == bounds.get(case, (0, 0, 64, 48)), suffix
            if case == "identity":
                # AE's PNG export rounds the exact 0.5 midpoint by bit depth.
                assert field.getchannel("R").getextrema() in ((127, 127), (128, 128)), suffix
                assert field.getchannel("G").getextrema() in ((127, 127), (128, 128)), suffix
            # Straight PNGs may keep hidden RGB after Set Matte. Compare the
            # visible premultiplied result, including alpha as its own channel.
            def premultiply(im):
                r, g, b, a = im.split()
                return Image.merge("RGBA", (ImageChops.multiply(r,a), ImageChops.multiply(g,a),
                                            ImageChops.multiply(b,a), a))
            diff = ImageChops.difference(premultiply(direct), premultiply(warped))
            maximum = max(v[1] for v in diff.getextrema())
            raw = diff.tobytes()
            changed = sum(any(raw[i:i+4]) for i in range(0, len(raw), 4))
            assert diff.getchannel("A").getbbox() is None, f"alpha: {suffix}"
            _, _, direct16 = rgba16(direct_path)
            _, _, warped16 = rgba16(warped_path)
            for samples in (direct16, warped16):
                for i in range(0, len(samples), 4):
                    for c in range(3):
                        samples[i+c] = (samples[i+c]*samples[i+3]+32767)//65535
            full_differences = [abs(a-b) for a,b in zip(direct16, warped16)]
            max16 = max(full_differences)
            alpha16 = max(full_differences[3::4])
            assert alpha16 <= (32 if depth == 32 else 0), f"16-bit alpha: {suffix}"
            if case == "identity":
                assert max16 == 0, f"identity reconstruction: {suffix}"
            if depth == 32 and case in ("shrink", "x-only", "flip-x", "flip-xy", "zero"):
                # Native AE resampling has finite precision even at integer targets.
                assert max16 <= 32, f"32-bpc shrink reconstruction: {suffix}"
            report.append({"case": case, "depth": depth, "max_8bit_difference": maximum,
                           "changed_pixels": changed, "max_16bit_difference": max16,
                           "max_alpha_16bit_difference": alpha16})
            print(f"{suffix}: max 8-bit difference={maximum}, max 16-bit difference={max16}, changed={changed}")
    (directory / "verification.json").write_text(json.dumps(report, indent=2)+"\n")
    print("PASS 27 cases: opaque maps, coverage, alpha, identity and 32-bpc shrink reconstruction")
    return report


if __name__ == "__main__":
    verify(Path(sys.argv[1]))
