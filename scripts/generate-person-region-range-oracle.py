#!/usr/bin/env python3
"""Independent scalar f32 reference for smart-keep on every retained source ordinal."""
import argparse, hashlib, json, math, pathlib, struct
p = argparse.ArgumentParser()
p.add_argument('--fixture', type=pathlib.Path, required=True)
p.add_argument('--output', type=pathlib.Path, required=True)
a = p.parse_args()
d = json.loads(a.fixture.read_text())
r = json.loads((pathlib.Path(d['expectedMasks']).parent / 'valid.json').read_text())
source = pathlib.Path(r['input']).read_bytes()
regions = pathlib.Path(r['regions'][0]['input']).read_bytes()
working = bytearray(pathlib.Path(r['mask']).read_bytes())
w, h, frames = r['width'], r['height'], r['frames']
scale = 512 / max(w, h)
mw, mh = [max(64, math.floor(n * scale / 16 + .5) * 16) for n in (w, h)]
assert len(source) == w * h * 4 * frames
assert len(working) == mw * mh * frames
assert len(regions) == 65536 * frames
f = lambda x: struct.unpack('f', struct.pack('f', x))[0]
def sample(data, width, height, u, v):
    x, y = f(f(u * width) - .5), f(f(v * height) - .5)
    x0, y0 = math.floor(x), math.floor(y)
    fx, fy = f(x - x0), f(y - y0)
    def at(px, py):
        return data[max(0, min(height - 1, py)) * width + max(0, min(width - 1, px))]
    upper = f(f(at(x0, y0) * f(1 - fx)) + f(at(x0 + 1, y0) * fx))
    lower = f(f(at(x0, y0 + 1) * f(1 - fx)) + f(at(x0 + 1, y0 + 1) * fx))
    return f(f(f(upper * f(1 - fy)) + f(lower * fy)) / 255)
for ordinal in range(frames):
    plane = regions[ordinal * 65536:(ordinal + 1) * 65536]
    for y in range(mh):
        for x in range(mw):
            if sample(plane, 256, 256, f(f(x + .5) / mw), f(f(y + .5) / mh)) >= .5:
                working[ordinal * mw * mh + y * mw + x] = 255
rgba = bytearray(source)
for ordinal in range(frames):
    plane = working[ordinal * mw * mh:(ordinal + 1) * mw * mh]
    for y in range(h):
        for x in range(w):
            index = ordinal * w * h * 4 + (y * w + x) * 4 + 3
            alpha = f(source[index] * sample(plane, mw, mh, f(f(x + .5) / w), f(f(y + .5) / h)))
            rgba[index] = min(255, math.floor(f(alpha + .5)))
a.output.mkdir(parents=True, exist_ok=True)
(a.output / 'range.gray').write_bytes(working)
(a.output / 'range.rgba').write_bytes(rgba)
report = {'frames': frames, 'width': w, 'height': h, 'maskWidth': mw, 'maskHeight': mh,
          'maskHash': hashlib.sha256(working).hexdigest(), 'rgbaHash': hashlib.sha256(rgba).hexdigest(),
          'scope': 'independent scalar f32 reference using retained original Concat regions; no native output used'}
(a.output / 'range-oracle.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report))
