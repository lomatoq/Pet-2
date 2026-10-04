"""Rebuild native icon coverage from the vendored, unmodified Phosphor SVGs.
Development only: python -m pip install CairoSVG Pillow. No runtime dependency.
"""
from pathlib import Path
from io import BytesIO
from PIL import Image
import cairosvg

ROOT = Path(__file__).resolve().parent
for svg in ROOT.glob("*.svg"):
    for density in (1, 2):
        size = 28 * density
        rendered = cairosvg.svg2png(bytestring=svg.read_bytes(), output_width=size * 4, output_height=size * 4)
        coverage = Image.open(BytesIO(rendered)).convert("RGBA").getchannel("A")
        coverage = coverage.resize((size, size), Image.Resampling.LANCZOS)
        svg.with_name(f"{svg.stem}-{density}x.alpha").write_bytes(coverage.tobytes())
