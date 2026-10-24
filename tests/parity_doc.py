"""Checker shared by the parity-map tests: every Python test has one row, a status, and a Rust test that exists.

`check(doc, files, this)` is called by `test_launcher_parity_doc.py` and `test_runner_parity_doc.py`.
`doc` is the map (a Markdown file), `files` the Python test files it covers and `this` the running
`unittest.TestCase`, which receives the assertions.
"""
import collections
import re

from paths import ROOT

RUST = ROOT / 'launcher-rust'

GAP = 'dropped: no Rust counterpart — '
# Behaviour the spec replaced on purpose, allowed in any file: `dropped: spec §<section> <reason>`.
SPEC = 'dropped: spec §'
SPEC_DROP = re.compile(r'^dropped: spec §\d[\d.]* .*\S')

RUST_ID = re.compile(r'`([^`]+\.rs(?:::\w+)+)`')
STATUS = re.compile(r'^(ported|replaced by (?P<by>.+)|dropped: (?P<why>\S.*))$')


def python_tests(files):
    """`tests/<file>::Class::test` for every test method of `files`, in file order."""
    found = []
    for path in files:
        owner = None
        for line in path.read_text(encoding='utf-8').splitlines():
            klass = re.match(r'^class (\w+)', line)
            if klass:
                owner = klass.group(1)
            method = re.match(r'^\s*def (test_\w+)', line)
            if method:
                prefix = f'tests/{path.name}::'
                if line[0].isspace() and owner:
                    prefix += f'{owner}::'
                found.append(prefix + method.group(1))
    return found


def table_rows(doc):
    """(python test, rust cell, status) for each row of the map, in document order: the rows whose
    first cell names a test (`tests/<file>.py::...`), not the per-file counts."""
    rows = []
    for line in doc.read_text(encoding='utf-8').splitlines():
        if not re.match(r'\| `tests/[^`]+\.py::', line):
            continue
        cells = [cell.strip() for cell in line.strip().strip('|').split(' | ')]
        while len(cells) < 3:
            cells.append('')
        rows.append((cells[0].strip('`'), cells[1], cells[2]))
    return rows


def gap_list(doc):
    """The Python tests the Gaps section names."""
    text = doc.read_text(encoding='utf-8')
    section = text.split('\n## Gaps', 1)
    if len(section) < 2:
        return None
    body = section[1].split('\n## ', 1)[0]
    return re.findall(r'^- `(tests/[^`]+)`', body, flags=re.MULTILINE)


def rust_tests(path):
    """Names of the `#[test]` functions in a Rust file, and the modules it declares."""
    names, pending = set(), False
    lines = path.read_text(encoding='utf-8').splitlines()
    for line in lines:
        stripped = line.strip()
        if stripped == '#[test]':
            pending = True
            continue
        if pending:
            match = re.match(r'(?:pub\s+)?(?:async\s+)?fn (\w+)\s*\(', stripped)
            if match:
                names.add(match.group(1))
                pending = False
            elif not (stripped.startswith('#[') or stripped.startswith('//') or not stripped):
                pending = False
    modules = set(re.findall(r'^\s*(?:pub\s+)?mod (\w+)\s*\{', '\n'.join(lines), flags=re.MULTILINE))
    return names, modules


def missing_rust_test(identifier):
    """Why `file.rs::module::name` names no Rust test, or None when it does."""
    parts = identifier.split('::')
    path = RUST / parts[0]
    if not parts[0].endswith('.rs') or len(parts) < 2:
        return f'{identifier}: not file.rs::name'
    if not path.is_file():
        return f'{identifier}: no file {path.relative_to(ROOT).as_posix()}'
    names, modules = rust_tests(path)
    for module in parts[1:-1]:
        if module not in modules:
            return f'{identifier}: no module {module}'
    if parts[-1] not in names:
        return f'{identifier}: no #[test] fn {parts[-1]}'
    return None


def is_allowed_drop(name, allowed):
    """True when the test `name` is inside an allowed prefix: a file (`tests/<file>.py`), a class
    (`tests/<file>.py::Class`) or the test itself."""
    return any(name == prefix or name.startswith(prefix + '::') for prefix in allowed)


def check(doc, files, this, allowed_drops=()):
    """Run every check of a map against `files`, reporting through the TestCase `this`.

    `allowed_drops`: prefixes of tests whose `dropped: <reason>` needs no `no Rust counterpart`
    wording (mechanics the Rust side has no equivalent for); every other drop is a gap or a spec drop.
    """
    rows = table_rows(doc)

    with this.subTest('every Python test has exactly one row'):
        expected = python_tests(files)
        listed = collections.Counter(row[0] for row in rows)
        this.assertEqual(sorted(name for name, count in listed.items() if count > 1), [], 'listed twice')
        this.assertEqual(sorted(set(expected) - set(listed)), [], 'no row')
        this.assertEqual(sorted(set(listed) - set(expected)), [], 'row for no Python test')

    with this.subTest('every row has a known status'):
        bad = [f'{name}: {status!r}' for name, _, status in rows if not STATUS.match(status)]
        this.assertEqual(bad, [])

    with this.subTest('drops name no Rust test and outside the allowed ones are gaps'):
        bad = []
        for name, rust, status in rows:
            if not status.startswith('dropped: '):
                continue
            if rust != '—':
                bad.append(f'{name}: dropped but names {rust}')
            if status.startswith(SPEC):
                if not SPEC_DROP.match(status):
                    bad.append(f'{name}: a spec drop names the section and the reason: {status!r}')
            elif not is_allowed_drop(name, allowed_drops) and not status.startswith(GAP):
                bad.append(f'{name}: drop outside the allowed ones must start {GAP!r} or {SPEC!r}')
            if status.startswith(GAP) and len(status) == len(GAP):
                bad.append(f'{name}: gap without what it tested')
        this.assertEqual(bad, [])

    with this.subTest('ported and replaced rows name Rust tests that exist'):
        bad = []
        for name, rust, status in rows:
            match = STATUS.match(status)
            if not match or status.startswith('dropped: '):
                continue
            named = RUST_ID.findall(rust)
            if not named:
                bad.append(f'{name}: no Rust test in {rust!r}')
            if match.group('by') is not None:
                by = RUST_ID.findall(match.group('by'))
                if not by or not set(by) <= set(named):
                    bad.append(f'{name}: replaced by {match.group("by")!r}, not a test of the row')
            for identifier in named:
                problem = missing_rust_test(identifier)
                if problem:
                    bad.append(f'{name}: {problem}')
        this.assertEqual(bad, [])

    with this.subTest('the Gaps section lists every gap row'):
        gaps = gap_list(doc)
        this.assertIsNotNone(gaps, 'no "## Gaps" section')
        gap_rows = sorted(name for name, _, status in rows if status.startswith(GAP))
        this.assertEqual(sorted(gaps), gap_rows)
