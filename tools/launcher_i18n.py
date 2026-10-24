"""One-off: converts the Python launcher's translation tables into launcher-rust/assets/i18n.json.

Run from anywhere: python tools/launcher_i18n.py"""
import ast
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LAUNCHER = ROOT / 'launcher'
OUTPUT = ROOT / 'launcher-rust' / 'assets' / 'i18n.json'
sys.path.insert(0, str(LAUNCHER))
import spartanport_lang  # noqa: E402
import spartanport_trophies  # noqa: E402

SOURCE = LAUNCHER / 'spartanport_launcher_win.py'


def launcher_warn():
    """The WARN constant of the launcher: the suffix marking a risky choice."""
    for node in ast.parse(SOURCE.read_text(encoding='utf-8')).body:
        if isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == 'WARN' for t in node.targets):
            return ast.literal_eval(node.value)
    raise SystemExit('WARN not found in ' + SOURCE.name)


def russian_pairs():
    """English text -> Russian text, from every _('English', 'Русский') call of the launcher."""
    warn = launcher_warn()
    pairs = {}
    for node in ast.walk(ast.parse(SOURCE.read_text(encoding='utf-8'))):
        if not (isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == '_'
                and len(node.args) == 2 and not node.keywords
                and all(isinstance(a, ast.Constant) and isinstance(a.value, str) for a in node.args)):
            continue
        english, russian = (a.value.removesuffix(warn) for a in node.args)
        pairs.setdefault(english, russian)
    return pairs


def build():
    return {
        'keys': spartanport_lang.KEYS,
        'translations': spartanport_lang.TRANSLATIONS,
        'extra': spartanport_lang.EXTRA,
        'ru': russian_pairs(),
        'windows_languages': {str(k): v for k, v in spartanport_lang.WINDOWS_LANGUAGES.items()},
        'trophies': {'count': spartanport_trophies.TROPHY_COUNT,
                     'texts': spartanport_trophies.TEXTS,
                     'ui': spartanport_trophies.UI},
    }


if __name__ == '__main__':
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT, 'w', encoding='utf-8', newline='\n') as stream:
        stream.write(json.dumps(build(), ensure_ascii=False, indent=1, sort_keys=False) + '\n')
    print(f'wrote {OUTPUT.relative_to(ROOT)}')
