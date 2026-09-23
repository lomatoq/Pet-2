"""Export editable RGBA layers using the production WGSL occlusion mask.

This is a deterministic asset compiler for already generated artwork. It does
not generate, retouch, crop, or reframe the art. Both layers keep the full canvas.
"""
from pathlib import Path
import json
import numpy as np
from PIL import Image

folder = Path(__file__).resolve().parent
source = Image.open(folder / "nest-pearl-v28.png").convert("RGBA")
pixels = np.asarray(source).copy()
height, width, _ = pixels.shape
u = (np.arange(width, dtype=np.float64) + 0.5) / width
v = (np.arange(height, dtype=np.float64)[:, None] + 0.5) / height
contour = json.loads((folder / "rim-contour-v29.json").read_text(encoding="utf-8"))
knots = np.asarray(contour["knots"], dtype=np.float64)
x = np.clip(u * width, knots[0, 0], knots[-1, 0])
interval = np.clip(np.searchsorted(knots[:, 0], x, side="left") - 1, 0, len(knots) - 2)
a = knots[interval]
b = knots[interval + 1]
h = b[:, 0] - a[:, 0]
q = np.clip((x - a[:, 0]) / h, 0.0, 1.0)
q2, q3 = q * q, q * q * q
edge_pixels = ((2 * q3 - 3 * q2 + 1) * a[:, 1]
    + (q3 - 2 * q2 + q) * h * a[:, 2]
    + (-2 * q3 + 3 * q2) * b[:, 1]
    + (q3 - q2) * h * b[:, 2])
assert np.all(edge_pixels >= np.minimum(a[:, 1], b[:, 1]) - 1e-6)
assert np.all(edge_pixels <= np.maximum(a[:, 1], b[:, 1]) + 1e-6)
edge = edge_pixels / height
feather = contour["feather_uv"]
t = np.clip((v - edge + feather) / (2.0 * feather), 0.0, 1.0)
mask = t * t * (3.0 - 2.0 * t)
alpha = pixels[:, :, 3].astype(np.float64) / 255.0
front_alpha = alpha * mask
back_alpha = (alpha - front_alpha) / np.maximum(1.0 - front_alpha, 1e-6)
back = pixels.copy()
front = pixels.copy()
back[:, :, 3] = np.rint(back_alpha * 255.0).astype(np.uint8)
front[:, :, 3] = np.rint(front_alpha * 255.0).astype(np.uint8)
Image.fromarray(back).save(folder / "nest-back-v29.png")
Image.fromarray(front).save(folder / "nest-front-v29.png")
ab = back[:, :, 3].astype(np.float64) / 255.0
af = front[:, :, 3].astype(np.float64) / 255.0
error = np.abs((af + ab * (1.0 - af)) - alpha) * 255.0
assert float(error.max()) <= 1.0
assert np.array_equal(back[:, :, :3], pixels[:, :, :3])
assert np.array_equal(front[:, :, :3], pixels[:, :, :3])
metrics = {
    "size": [width, height],
    "max_recomposed_alpha_error_in_u8_levels": float(error.max()),
    "source_rgb_preserved_exactly": True,
    "formula": "A_front=A_source*mask; A_back=(A_source-A_front)/(1-A_front)",
    "layers": ["nest-back-v29.png", "nest-front-v29.png"],
    "contour": "rim-contour-v29.json",
    "interpolation_preserves_monotonicity_without_overshoot": True,
    "center_rim_y": float(edge_pixels[width // 2]),
}
(folder / "layer-validation-v29.json").write_text(json.dumps(metrics, indent=2), encoding="utf-8")
print(json.dumps(metrics, indent=2))
