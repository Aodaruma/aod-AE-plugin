"""Check distance symmetry, fixed anchors, and map bypass in AE's 16-bit PNGs."""
import json
import sys
from pathlib import Path

from verify_seam_map import rgba16


def verify(directory):
    log = (directory / 'result.txt').read_text(encoding='utf-8-sig')
    assert 'host renders completed' in log and 'FAIL' not in log, log
    report = []
    for depth in (8, 16, 32):
        def read(name):
            w, h, pixels = rgba16(directory / f'{name}-{depth}_00000.png')
            assert (w, h) == (96, 64)
            return pixels

        source = read('source')
        for name in ('identity', 'no-map'):
            assert read(name) == source, (depth, name)
        assert read('distance') == read('complement'), (depth, 'distance symmetry')
        assert read('map-off') == read('normal'), (depth, 'map OFF')
        for name in ('distance', 'complement', 'flip', 'restored', 'x-only', 'y-only'):
            output = read(name)
            changed = 0
            for y in range(64):
                for x in range(96):
                    i = (y * 96 + x) * 4
                    actual, expected = output[i:i+4], source[i:i+4]
                    if 32 <= x < 64:
                        assert actual == expected, (depth, name, x, y)
                    else:
                        changed += actual != expected
            assert changed > 0, (depth, name, 'no deformation')
            report.append({'case': name, 'bpc': depth, 'fixed_pixels': 2048,
                           'changed_unpinned_pixels': changed})
        print(f'PASS {depth} bpc: identity, unset map, distance symmetry, anchors, negative scales, restore, axes, map OFF')
    (directory / 'verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print('PASS 30 effect renders at 8/16/32 bpc')


if __name__ == '__main__':
    verify(Path(sys.argv[1]))
