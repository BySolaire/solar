"""Bake the validated mockup's home backdrop (accueil.html) into two PNGs.
horizon: CSS .main::before -> 112% wide 16:9 box, right -48 bottom -14,
rotate(-14deg), grayscale(1) brightness(1.1), opacity .87, vertical mask.
veil: the two radial accent gradients of .glass .window, pane-relative."""
import sys, numpy as np
from PIL import Image
src, out_dir = sys.argv[1], sys.argv[2]
REF_W, REF_H = 1600, 900
im = Image.open(src).convert('RGB').resize((REF_W, REF_H), Image.LANCZOS)
a = np.asarray(im).astype(np.float32) / 255.
g = (a[..., 0] * 0.2126 + a[..., 1] * 0.7152 + a[..., 2] * 0.0722) * 1.10
g = np.clip(g, 0, 1)
t = (np.arange(REF_H) + 0.5) / REF_H
m = np.interp(t, [0, .73, .82, .88, 1.], [0, 0, 1, 1, 0])[:, None] * 0.87
rgba = np.zeros((REF_H, REF_W, 4), np.float32)
rgba[..., 0] = rgba[..., 1] = rgba[..., 2] = g * m   # premultiplied
rgba[..., 3] = m * np.ones_like(g)
pm = Image.fromarray((rgba * 255 + .5).astype(np.uint8), 'RGBa')
rot = pm.rotate(14, resample=Image.BICUBIC, expand=True)  # CSS -14deg
Wr, Hr = rot.size
arr = np.asarray(rot)
ys, xs = np.where(arr[..., 3] > 1)
x0, x1, y0, y1 = xs.min(), xs.max() + 1, ys.min(), ys.max() + 1
crop = rot.crop((x0, y0, x1, y1)).convert('RGBA')
crop.save(out_dir + '/solar-horizon.png', optimize=True)
print(f'HORIZON ref=({REF_W},{REF_H}) rotated=({Wr},{Hr}) crop=({x0},{y0},{x1-x0},{y1-y0})')

# Veil, pane-relative. Mockup window 1512x982, pane x 260..1512, y 40..954.
WIN_W, WIN_H, PX, PY, PW, PH = 1512, 982, 260, 40, 1252, 914
S = 4
vw, vh = PW // S, PH // S
xx = PX + (np.arange(vw) + .5) * S
yy = PY + (np.arange(vh) + .5) * S
X, Y = np.meshgrid(xx, yy)
# Neutral grey of Solar Blue's luminance: Solar never uses blue-grey
# (client decision, 2026-11-24).
accent = np.array([0xC7, 0xC7, 0xC7], np.float32) / 255.
def layer(cx, cy, rx, ry, mix):
    r = np.sqrt(((X - cx * WIN_W) / (rx * WIN_W)) ** 2 + ((Y - cy * WIN_H) / (ry * WIN_H)) ** 2)
    al = np.clip(1 - r, 0, 1)
    return accent * mix * al[..., None], al
c1, a1 = layer(.27, .70, .65, .24, .16)
c2, a2 = layer(.66, .58, .43, .37, .10)
col = c1 + (1 - a1)[..., None] * c2
alp = 1 - (1 - a1) * (1 - a2)
straight = np.where(alp[..., None] > 0, col / np.maximum(alp, 1e-6)[..., None], 0)
v = np.dstack([straight, alp])
Image.fromarray((np.clip(v, 0, 1) * 255 + .5).astype(np.uint8), 'RGBA').save(out_dir + '/solar-veil.png', optimize=True)
print('VEIL', vw, vh)
