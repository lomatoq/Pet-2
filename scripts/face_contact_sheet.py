"""Lay out unmodified production-render captures for visual review."""
from pathlib import Path
import sys
from PIL import Image, ImageDraw

source = Path(sys.argv[1])
files = sorted(source.glob('*white-100.ppm'))
sheet = Image.new('RGB', (512 * 4, 542 * ((len(files) + 3) // 4)), 'white')
draw = ImageDraw.Draw(sheet)
for index, path in enumerate(files):
    x, y = (index % 4) * 512, (index // 4) * 542
    with Image.open(path) as frame:
        sheet.paste(frame, (x, y + 30))
    draw.text((x + 12, y + 8), path.stem, fill='black')
sheet.save(source / 'contact-sheet.png')

timeline = sorted(source.glob('Timeline-*.ppm'))
if timeline:
    frames = []
    for path in timeline:
        with Image.open(path) as frame:
            frames.append(frame.copy())
    frames[0].save(source / 'eye-transition.gif', save_all=True,
                   append_images=frames[1:], duration=33, loop=0)
    strip = Image.new('RGB', (512 * 4, 542 * 3), 'white')
    strip_draw = ImageDraw.Draw(strip)
    for index, frame_index in enumerate(range(0, min(120, len(frames)), 10)):
        x, y = (index % 4) * 512, (index // 4) * 542
        strip.paste(frames[frame_index], (x, y + 30))
        strip_draw.text((x + 12, y + 8), f'{frame_index / 30:.2f} s', fill='black')
    strip.save(source / 'eye-transition-sheet.png')
