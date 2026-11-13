"""Verify anchor pixels in 8/16/32-bpc AE renders without reducing PNG precision."""
import importlib.util
import json
import sys
from pathlib import Path

reader = Path(__file__).resolve().parents[2] / 'seam-scale/tests/verify_seam_map.py'
spec = importlib.util.spec_from_file_location('seam_png_reader', reader)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def verify(directory):
    log = (directory / 'result.txt').read_text(encoding='utf-8-sig')
    assert 'host renders completed' in log and 'FAIL' not in log, log
    cases = {'identity': None, 'no-map': None, 'black-pin': 0,
             'mid-pin': 1, 'white-pin': 2, 'flip-pin': 1, 'restored-pin': 1, 'x-only': 1, 'y-only': 1}
    report = []
    for depth in (8, 16, 32):
        w, h, source = module.rgba16(directory / f'source-{depth}_00000.png')
        assert (w, h) == (96, 64)
        for name, band in cases.items():
            ow, oh, output = module.rgba16(directory / f'{name}-{depth}_00000.png')
            assert (ow, oh) == (w, h)
            checked = changed = 0
            for y in range(h):
                for x in range(w):
                    i = (y * w + x) * 4
                    actual, expected = output[i:i+4], source[i:i+4]
                    if band is None or band * 32 <= x < (band + 1) * 32:
                        assert actual == expected, (name, depth, x, y, actual, expected)
                        checked += 1
                    else:
                        changed += actual != expected
            if band is not None:
                assert changed > 0, (name, depth, 'Effect did not change any unpinned pixels')
            report.append({'case': name, 'bpc': depth, 'fixed_pixels': checked,
                           'changed_unpinned_pixels': changed})
            print(f'PASS {name}-{depth}: {checked} fixed pixels; {changed} changed pixels')
    (directory / 'verification.json').write_text(json.dumps(report, indent=2) + '\n')
    print('PASS 27 cases: map checkout, identity, exact anchor pixels, expansion, flips, restoration, separate axes')


if __name__ == '__main__':
    verify(Path(sys.argv[1]))
