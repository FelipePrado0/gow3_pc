"""Adds the game to Steam as a non-Steam shortcut, with its icon and artwork.

Steam keeps non-Steam games in userdata/<account>/config/shortcuts.vdf (binary VDF) and their
pictures in userdata/<account>/config/grid/ named after the shortcut's app id. Steam rewrites
shortcuts.vdf when it exits, so the shortcut is only written while Steam is closed.

The pictures are made from the game dump (sce_sys/pic1.png) and the launcher's icon: the
vertical and horizontal capsules (with the logo), the hero background, the transparent logo.
"""
import binascii
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys

APP_NAME = 'God of War III Remastered'
CAPSULE = (600, 900)    # vertical capsule: <id>p.png
WIDE = (920, 430)       # horizontal capsule: <id>.png
HERO = (1920, 620)      # library background: <id>_hero.png
GOLD = (233, 214, 164)

WINDOWS = os.name == 'nt'
MACOS = sys.platform == 'darwin'
LINUX_ROOTS = ('~/.local/share/Steam', '~/.steam/steam', '~/.var/app/com.valvesoftware.Steam/.local/share/Steam')
WINDOWS_ROOTS = ('C:/Program Files (x86)/Steam', 'C:/Program Files/Steam')
MACOS_ROOTS = ('~/Library/Application Support/Steam',)
FONTS = ('C:/Windows/Fonts/georgiab.ttf', 'C:/Windows/Fonts/timesbd.ttf',
         '/System/Library/Fonts/Supplemental/Georgia Bold.ttf',
         '/System/Library/Fonts/Supplemental/Times New Roman Bold.ttf',
         '/usr/share/fonts/TTF/DejaVuSerif-Bold.ttf', '/usr/share/fonts/dejavu/DejaVuSerif-Bold.ttf',
         '/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf', '/usr/share/fonts/noto/NotoSerif-Bold.ttf',
         '/usr/share/fonts/truetype/noto/NotoSerif-Bold.ttf')


class SteamError(Exception):
    """code: 'running', 'no_steam' or 'no_art' (the launcher words each one)."""

    def __init__(self, code, detail=''):
        super().__init__(f'{code}: {detail}' if detail else code)
        self.code, self.detail = code, detail


# ---------------------------------------------------------------------------------------------
# Binary VDF: 0 = map (key, entries, 8), 1 = string, 2 = uint32, 8 = end of the map.

def parse_vdf(data):
    pos = 0

    def text():
        nonlocal pos
        end = data.index(b'\0', pos)
        value = data[pos:end].decode('utf-8', 'replace')
        pos = end + 1
        return value

    def entries():
        nonlocal pos
        result = {}
        while True:
            kind = data[pos]
            pos += 1
            if kind == 8:
                return result
            key = text()
            if kind == 0:
                result[key] = entries()
            elif kind == 1:
                result[key] = text()
            elif kind == 2:
                result[key], = struct.unpack_from('<I', data, pos)
                pos += 4
            else:
                raise ValueError(f'unknown VDF type {kind} at {pos}')
    return entries()


def dump_vdf(tree):
    out = bytearray()

    def write(mapping):
        for key, value in mapping.items():
            if isinstance(value, dict):
                out.extend(b'\x00' + key.encode() + b'\0')
                write(value)
            elif isinstance(value, str):
                out.extend(b'\x01' + key.encode() + b'\0' + value.encode() + b'\0')
            else:
                out.extend(b'\x02' + key.encode() + b'\0' + struct.pack('<I', value & 0xffffffff))
        out.append(8)
    write(tree)
    return bytes(out)


def shortcut_appid(exe, name):
    """The 32-bit id Steam gives a non-Steam game: the CRC of its quoted exe and name, top bit set."""
    return (binascii.crc32((exe + name).encode('utf-8')) & 0xffffffff) | 0x80000000


# ---------------------------------------------------------------------------------------------
# Steam folders.

def steam_running():
    if WINDOWS:
        result = subprocess.run(['tasklist', '/FI', 'IMAGENAME eq steam.exe', '/NH'], capture_output=True,
                                text=True, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        return 'steam.exe' in result.stdout.lower()
    # The Mac client's process is steam_osx.
    return subprocess.run(['pgrep', '-x', 'steam_osx' if MACOS else 'steam'], capture_output=True).returncode == 0


def steam_roots():
    """Steam installation folders to look in, the registry's first on Windows."""
    if WINDOWS:
        roots = []
        try:
            import winreg
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r'Software\Valve\Steam') as key:
                roots.append(Path(winreg.QueryValueEx(key, 'SteamPath')[0]))
        except OSError:
            pass
        return roots + [Path(p) for p in WINDOWS_ROOTS]
    return [Path(os.path.expanduser(p)) for p in (MACOS_ROOTS if MACOS else LINUX_ROOTS)]


def account_folders(root=None):
    """userdata/<account> folders of the Steam installation(s) of this user."""
    roots = [Path(root)] if root else steam_roots()
    seen, found = set(), []
    for base in roots:
        userdata = base / 'userdata'
        if not userdata.is_dir():
            continue
        for entry in sorted(userdata.iterdir()):
            real = entry.resolve()
            if entry.name.isdigit() and entry.name != '0' and entry.is_dir() and real not in seen:
                seen.add(real)
                found.append(entry)
    return found


# ---------------------------------------------------------------------------------------------
# Pictures.

def font(size):
    from PIL import ImageFont
    for path in FONTS:
        if os.path.isfile(path):
            return ImageFont.truetype(path, size)
    return ImageFont.load_default(size)


def cover(image, width, height, focus=(0.5, 0.5)):
    """image scaled to fill width x height and cropped around the focus point (0..1 each axis)."""
    from PIL import Image
    scale = max(width / image.width, height / image.height)
    resized = image.resize((round(image.width * scale), round(image.height * scale)), Image.LANCZOS)
    left = round((resized.width - width) * focus[0])
    top = round((resized.height - height) * focus[1])
    return resized.crop((left, top, left + width, top + height))


def spaced_width(draw, text, face, gap):
    return sum(draw.textlength(ch, font=face) for ch in text) + gap * (len(text) - 1)


def fit_face(draw, text, target, gap_ratio):
    """A font whose text (letters spaced by gap_ratio of the size) is `target` px wide."""
    size = target / 8
    for _ in range(3):
        face = font(round(size))
        width = spaced_width(draw, text, face, size * gap_ratio)
        size *= target / width
    return font(round(size)), size * gap_ratio


def spaced(draw, center_x, y, text, face, fill, gap):
    """text with `gap` px between letters, centered on center_x."""
    widths = [draw.textlength(ch, font=face) for ch in text]
    x = center_x - (sum(widths) + gap * (len(text) - 1)) / 2
    for ch, w in zip(text, widths):
        draw.text((x, y), ch, font=face, fill=fill)
        x += w + gap


def usable_emblem(image):
    """Whether the icon can be the logo's emblem: large and with a transparent background."""
    rgba = image.convert('RGBA')
    return rgba.width >= 200 and rgba.getpixel((1, 1))[3] < 40


def make_logo(emblem, width):
    """Transparent logo: the emblem (when there is one) over GOD OF WAR III and REMASTERED, `width` px wide."""
    from PIL import Image, ImageDraw, ImageFilter
    scale = 2
    w = width * scale
    emblem_size = round(w * 0.30) if emblem is not None else 0
    probe = ImageDraw.Draw(Image.new('RGBA', (1, 1)))
    title_face, title_gap = fit_face(probe, 'GOD OF WAR III', w * 0.94, 0.04)
    sub_face, sub_gap = fit_face(probe, 'REMASTERED', w * 0.60, 0.45)
    title_y = emblem_size + (w * 0.03 if emblem is not None else 0)
    sub_y = title_y + title_face.size * 1.35
    height = round(sub_y + sub_face.size * 1.5)
    canvas = Image.new('RGBA', (w, height), (0, 0, 0, 0))
    if emblem is not None:
        mark = emblem.convert('RGBA').resize((emblem_size, emblem_size), Image.LANCZOS)
        canvas.alpha_composite(mark, ((w - emblem_size) // 2, 0))
    shadow = Image.new('RGBA', canvas.size, (0, 0, 0, 0))
    for layer, fill, offset in ((shadow, (0, 0, 0, 220), 3 * scale), (canvas, GOLD + (255,), 0)):
        draw = ImageDraw.Draw(layer)
        spaced(draw, w / 2 + offset, title_y + offset, 'GOD OF WAR III', title_face, fill, title_gap)
        spaced(draw, w / 2 + offset, sub_y + offset, 'REMASTERED', sub_face, fill, sub_gap)
    blurred = shadow.filter(ImageFilter.GaussianBlur(2 * scale))
    blurred.alpha_composite(canvas)
    return blurred.resize((width, round(height / scale)), Image.LANCZOS)


def darken_bottom(image, strength=0.85, start=0.45):
    from PIL import Image
    gradient = Image.new('L', (1, image.height))
    for y in range(image.height):
        t = max(0.0, (y / image.height - start) / (1 - start))
        gradient.putpixel((0, y), round(255 * strength * t))
    mask = gradient.resize(image.size)
    return Image.composite(Image.new('RGBA', image.size, (0, 0, 0, 255)), image, mask)


def make_art(game, icon, out_dir, app_id):
    """Writes the five pictures of shortcut app_id into out_dir; returns the icon's path."""
    from PIL import Image
    game, out_dir = Path(game), Path(out_dir)
    background = next((game / 'sce_sys' / n for n in ('pic1.png', 'pic0.png') if (game / 'sce_sys' / n).is_file()), None)
    if background is None:
        raise SteamError('no_art', str(game / 'sce_sys'))
    base = Image.open(background).convert('RGBA')
    picture = Image.open(icon if Path(icon).is_file() else game / 'sce_sys' / 'icon0.png').convert('RGBA')
    emblem = picture if usable_emblem(picture) else None
    out_dir.mkdir(parents=True, exist_ok=True)

    cover(base, *HERO, focus=(0.5, 0.52)).convert('RGB').save(out_dir / f'{app_id}_hero.png')
    logo = make_logo(emblem, 1000)
    logo.save(out_dir / f'{app_id}_logo.png')

    wide = cover(base, *WIDE, focus=(0.5, 0.5))
    small = make_logo(emblem, 380)
    wide.alpha_composite(small, (48, (WIDE[1] - small.height) // 2))
    wide.convert('RGB').save(out_dir / f'{app_id}.png')

    tall = darken_bottom(cover(base, *CAPSULE, focus=(0.9, 0.5)))
    big = make_logo(emblem, 500)
    tall.alpha_composite(big, ((CAPSULE[0] - big.width) // 2, CAPSULE[1] - big.height - 50))
    tall.convert('RGB').save(out_dir / f'{app_id}p.png')

    icon_path = out_dir / f'{app_id}_icon.png'
    picture.resize((256, 256), Image.LANCZOS).save(icon_path)
    (out_dir / f'{app_id}.json').write_text(json.dumps(
        {'nVersion': 1, 'logoPosition': {'pinnedPosition': 'BottomLeft', 'nWidthPct': 42, 'nHeightPct': 56}}))
    return icon_path


# ---------------------------------------------------------------------------------------------
# The shortcut.

def shortcut_entry(app_id, exe, start_dir, options, icon):
    return {'appid': app_id, 'AppName': APP_NAME, 'Exe': exe, 'StartDir': start_dir, 'icon': str(icon),
            'ShortcutPath': '', 'LaunchOptions': options, 'IsHidden': 0, 'AllowDesktopConfig': 1,
            'AllowOverlay': 1, 'OpenVR': 0, 'Devkit': 0, 'DevkitGameID': '', 'DevkitOverrideAppID': 0,
            'LastPlayTime': 0, 'FlatpakAppID': '', 'tags': {}}


def add_to_steam(game, command, icon, steam_root=None, running=None):
    """Adds (or updates) the shortcut for every Steam account of the user.

    command is (executable, launch options, start folder). Returns {'accounts': n, 'updated': bool,
    'app_id': id}; raises SteamError when Steam is open ('running') or not installed ('no_steam')."""
    if running is None:
        running = steam_running()
    if running:
        raise SteamError('running')
    accounts = account_folders(steam_root)
    if not accounts:
        raise SteamError('no_steam')
    program, options, start_dir = command
    exe = f'"{program}"'
    app_id = shortcut_appid(exe, APP_NAME)
    updated = False
    for account in accounts:
        config = account / 'config'
        grid = config / 'grid'
        config.mkdir(parents=True, exist_ok=True)
        icon_path = make_art(game, icon, grid, app_id)
        file = config / 'shortcuts.vdf'
        tree = parse_vdf(file.read_bytes()) if file.is_file() and file.stat().st_size else {'shortcuts': {}}
        shortcuts = tree.setdefault('shortcuts', {})
        entry = shortcut_entry(app_id, exe, f'"{start_dir}"', options, icon_path)
        slot = next((k for k, v in shortcuts.items() if v.get('appid') == app_id or v.get('AppName') == APP_NAME), None)
        if slot is None:
            slot = str(max((int(k) for k in shortcuts if k.isdigit()), default=-1) + 1)
        else:
            updated = True
            entry['tags'] = shortcuts[slot].get('tags', {})
            entry['LastPlayTime'] = shortcuts[slot].get('LastPlayTime', 0)
        shortcuts[slot] = entry
        if file.is_file():
            shutil.copy2(file, file.with_name('shortcuts.vdf.gow3-backup'))
        file.write_bytes(dump_vdf(tree))
    return {'accounts': len(accounts), 'updated': updated, 'app_id': app_id}
