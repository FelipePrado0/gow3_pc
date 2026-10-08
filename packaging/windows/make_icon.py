"""Draws the port's icon (an omega on a dark red field) as .ico and .png. Needs Pillow.
usage: make_icon.py launcher/gow3.ico launcher/gow3.png"""
import sys
from PIL import Image, ImageDraw, ImageFilter

S = 1024  # drawn large, scaled down for every icon size


def draw():
    image = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    mask = Image.new('L', (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle((24, 24, S - 24, S - 24), radius=190, fill=255)

    field = Image.new('RGBA', (S, S))
    fd = ImageDraw.Draw(field)
    for y in range(S):
        t = y / S
        fd.line((0, y, S, y), fill=(int(96 - 66 * t), int(14 - 8 * t), int(12 - 6 * t), 255))

    glow = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse((200, 160, 824, 784), fill=(210, 60, 30, 120))
    field.alpha_composite(glow.filter(ImageFilter.GaussianBlur(90)))

    # The omega: a thick ring open at the bottom, standing on two feet.
    omega = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    od = ImageDraw.Draw(omega)
    ash = (236, 226, 208, 255)
    cx, cy, r, w = 512, 450, 270, 92
    od.ellipse((cx - r, cy - r, cx + r, cy + r), outline=ash, width=w)
    od.polygon([(cx - 120, cy + 120), (cx + 120, cy + 120), (cx + 150, cy + r + 10), (cx - 150, cy + r + 10)],
               fill=(0, 0, 0, 0))
    for side in (-1, 1):
        foot_x = cx + side * 150
        od.rectangle((min(foot_x, foot_x + side * 170), 760, max(foot_x, foot_x + side * 170), 842), fill=ash)
        od.rectangle((foot_x - 46, cy + 150, foot_x + 46, 842), fill=ash)
    shadow = omega.copy()
    shadow.putalpha(omega.getchannel('A').point(lambda a: a * 0.6))
    shadow = Image.composite(Image.new('RGBA', (S, S), (0, 0, 0, 255)), Image.new('RGBA', (S, S), (0, 0, 0, 0)),
                             shadow.getchannel('A')).filter(ImageFilter.GaussianBlur(18))
    field.alpha_composite(shadow, (0, 14))
    field.alpha_composite(omega)

    image.paste(field, (0, 0), mask)
    border = ImageDraw.Draw(image)
    border.rounded_rectangle((24, 24, S - 24, S - 24), radius=190, outline=(200, 169, 106, 255), width=22)
    return image


big = draw()
big.resize((256, 256), Image.LANCZOS).save(
    sys.argv[1], sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
big.resize((128, 128), Image.LANCZOS).save(sys.argv[2])
