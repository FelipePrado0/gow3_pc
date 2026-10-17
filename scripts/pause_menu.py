# SPDX-License-Identifier: GPL-2.0-or-later
""""Exit to Desktop" rows in the game's own pause menu and in its Options screen (docs/plans/2026-10-15-native-menu-feasibility.md).

All menus are one compiled state-machine script, the SMI_R_UI entry of R_UI.WAD. Its header (LE)
gives the code start (+0x0c) and the symbol table (+0x10); fields inside are big-endian:
- state records, a preorder tree up to the code: `04 <subtree size>`, parent and symbol words,
  then items `01 <entry code>`, `02 <update code>` and transitions `03 <code>` each followed by
  its trigger words (0x80 in the first byte: more follow). Code refs and literal operands count
  from code start + 4; state refs from SMI + 4; symbol refs from the symbol table + 4.
- code: `16 <u16 literal>`, `01 <i32>`, `09 e0NN` (local NN), `13 <provider:4 function:12>`
  (builtin call), `43` (pop), `49` (return), `4a <rel32>` (call), `47 <state>` (go to state).

Pause menu: PauseMain shows line0..4 (Continue .. Quit Game) and clears line5..7, whose rows the
UI already has. WadChoose, a cheat-only level loader right after PauseMain's subtree, becomes its
sixth child HLExit, with its code written over WadChoose's: no byte moves.

Options screen (reached from the pause menu and from the main menu): PauseOptions shows four rows
and clears line4. A fifth state record is inserted after HLDisplay, so the state table and
everything after it move: state refs, parents and subtree sizes are corrected, and the header and
the WAD entry grow. Its code and the label share WadChoose's dead code with the pause row, so the
literal pool and the symbol table keep their places relative to the code.

Confirming a row asks Quit Game's question (the confirmation is the FailSafe machine of the same
script: a message to it shows the dialog, and its FailSafeYes state acts on "yes" by the kind of
action). The row marks its request with a third parameter of 5 (builtin 0x5e pops a count, then up to 3 parameters, the message and the target); FailSafeDisplay keeps
that mark in a local nothing else uses, and FailSafeYes, given the mark, calls builtin 0xfff, which
src/menu_hook.h turns into the end of the process, instead of what Quit Game does. Both states get a
copy of their block with the few instructions added, written over WadChoose's dead code.
"""
import json
from pathlib import Path
import shutil
import struct

from glyphs import entries

VERSION = 6  # of the patch: a change rebuilds the cached copy
WAD = 'EXEC/WAD/PS4_LE/R_UI.WAD'
LABEL = b'Exit to Desktop'
EXIT_CALL = b'\x13\x0f\xff\x43'  # builtin 0xfff (src/menu_hook.h), then pop
UP, DOWN, CONFIRM, REPLY = [0xe0, 0xec], [0xe4, 0xf0], [0xe9], [0x4b]  # trigger ids: pad events and the dialog's answer
ROW_SIZE = 0x3c  # of the row's state record
# Quit Game's reaction to the dialog being hidden by "no" (the message's first parameter is 0): input back on.
REPLY_TEMPLATE = bytes.fromhex('01 00000000 130047 01 00000000 3f 170005 19 130069 43 49')
MARK = 5  # the exit rows' third message parameter (Restart and Quit send two)
# Opening of FailSafeDisplay's block for message 75 ("show" when the first parameter is 1) and of
# FailSafeYes' confirm block (it branches on the action's kind).
SHOW_BLOCK = bytes.fromhex('01 00000000 130047 01 00000001 3f 170084')
YES_BLOCK = bytes.fromhex('03 e001 0b e002 01 00000001 3f 17002f')
# FailSafeDisplay keeps the mark in its local 22 (failSafeMenu, unused); the confirm block ends the
# process when it is MARK. The jump skips the exit call, its pop and the return.
LOCAL = bytes([0xe0, 0x16])
SHOW_PREFIX = bytes.fromhex('01 00000002 130047') + b'\x07' + LOCAL + b'\x43'
YES_PREFIX = b'\x03' + LOCAL + b'\x01' + MARK.to_bytes(4, "big") + bytes.fromhex('3f 170005') + EXIT_CALL + b'\x49'
# Bytes taken by each opcode with its operands (the interpreter's handlers, image 0x1d5a4e).
LENGTH = {op: 5 if op in (0x00, 0x01, 0x47, 0x4a) else 3 if 0x02 <= op <= 0x18 else 1 for op in range(0x4c)}


def be(value, size=4):
    return value.to_bytes(size, 'big')


def decode(data, start, end):
    """[(offset, opcode, operand bytes)] of the code in data[start:end]; ValueError when it does
    not split exactly into instructions."""
    out, i = [], start
    while i < end:
        op = data[i]
        if op not in LENGTH:
            raise ValueError(f'unknown opcode {op:#x}')
        out.append((i, op, bytes(data[i + 1:i + LENGTH[op]])))
        i += LENGTH[op]
    if i != end:
        raise ValueError('code overruns its block')
    return out


class Script:
    def __init__(self, data, base):
        self.data, self.base = data, base
        self.code, self.symtab = struct.unpack_from('<II', data, base + 0x0c)
        self.lit = self.code + 4  # code refs and literal operands count from here
        names, i, pool = {}, self.symtab + 8, None
        while pool is None or i < pool:
            _kind, _index, name = struct.unpack_from('>hhI', data, base + i)
            at = self.symtab + 4 + name
            pool = at if pool is None else min(pool, at)
            names[i - self.symtab - 4] = self.string(at)
            i += 8
        self.records = self.parse_records()
        self.by_name = {}
        for r in self.records:
            self.by_name.setdefault(names.get(r['id']), []).append(r)

    def string(self, at):
        end = self.data.index(b'\0', self.base + at)
        return bytes(self.data[self.base + at:end]).decode('latin1')

    def parse_records(self):
        # The tree starts after a header table of variable length: the first offset that parses
        # exactly up to the code is it.
        for start in range(0x18, 0x400, 4):
            try:
                return self.records_from(start)
            except ValueError:
                continue
        raise ValueError('state table not found')

    def records_from(self, at):
        d, b, out = self.data, self.base, []
        while at < self.code:
            if d[b + at] != 4:
                raise ValueError(f'no state record at {at:#x}')
            size = int.from_bytes(d[b + at + 1:b + at + 4], 'big')
            parent, sid = struct.unpack_from('>II', d, b + at + 4)
            j, items = at + 12, []
            while j < self.code and d[b + j] != 4:
                items.append((d[b + j], int.from_bytes(d[b + j + 1:b + j + 4], 'big')))
                j += 4
            out.append(dict(at=at, size=size, parent=parent, id=sid, items=items))
            at = j
        if at != self.code:
            raise ValueError('state table overruns the code')
        return out

    def one(self, name):
        found = self.by_name.get(name, [])
        if len(found) != 1:
            raise ValueError(f'{name}: {len(found)} states')
        return found[0]

    @staticmethod
    def blocks(record):
        """(kind, code ref, triggers) of each code block of a state."""
        out = []
        for kind, value in record['items']:
            if kind in (1, 2, 3):
                out.append([kind, value, []])
            elif out and out[-1][0] == 3:
                out[-1][2].append(value)
        return out

    def code_refs(self):
        return sorted(v for r in self.records for k, v in r['items'] if k in (1, 2, 3))

    def block_bytes(self, ref):
        refs = self.code_refs()
        end = next(r for r in refs if r > ref)
        return bytes(self.data[self.base + self.lit + ref:self.base + self.lit + end])

    def literal(self, text):
        at = self.data.find(text + b'\0', self.base + self.lit, self.base + self.symtab)
        if at < 0:
            raise ValueError(f'literal {text!r} not found')
        return at - self.base - self.lit

    def spans(self):
        """[(start, end)] code offsets of every code block of an unpatched script: the last one
        ends where the literal pool begins, at the smallest literal operand."""
        refs = sorted(set(self.code_refs()))
        spans = list(zip(refs, refs[1:]))
        pool = min(int.from_bytes(arg, 'big') for a, b in spans
                   for _at, op, arg in decode(self.data, self.base + self.lit + a, self.base + self.lit + b) if op == 0x16)
        if refs[-1] >= pool:
            raise ValueError('literal pool not found')
        return [*spans, (refs[-1], pool)]


def transition(s, record, triggers):
    found = [b for b in s.blocks(record) if b[0] == 3 and b[2] == triggers]
    if len(found) != 1:
        raise ValueError(f'transition {triggers} not found')
    return found[0][1]


def retarget(s, record, triggers, old, new):
    """(offset, bytes) that make the state `record` go to the state ref `new` instead of `old`
    when `triggers` fire."""
    code = s.base + s.lit + transition(s, record, triggers)
    at = s.data.find(b'\x47' + be(old), code, code + 0x20)
    if at < 0:
        raise ValueError('transition target not found')
    return at + 1, be(new)


def row_code(s, quit_, start, up, down, slot, label=True):
    """(code, pieces) of an Exit row: the label (unless `label` is false), then Quit Game's blocks
    as the template, with `slot` as its highlight, laid from code offset `start`; `up` and `down`
    are the state refs its arrows go to. Confirming asks Quit Game's own question ("Are you sure
    you want to Quit?") with the exit mark; the row's handler for the dialog being hidden is Quit
    Game's. pieces: code offset of each block."""
    lit = lambda text: be(s.literal(text), 2)  # noqa: E731
    blocks = {k: v for k, v, _t in s.blocks(quit_) if k in (1, 2)}
    highlight = (s.block_bytes(blocks[1]), s.block_bytes(blocks[2]))
    old, new = b'\x16' + lit(b'menuItem4Select'), b'\x16' + lit(slot)
    if any(code.count(old) != 1 for code in highlight):
        raise ValueError('Quit Game highlight not found')
    arrow = s.block_bytes(transition(s, quit_, UP))
    ask = s.block_bytes(transition(s, quit_, CONFIRM))
    answer = s.block_bytes(transition(s, quit_, REPLY))
    if arrow[0] != 0x4a or ask[0] != 0x16 or ask[3:7] != b'\x13\x00\x5c\x43' or b'\x13\x00\x5e\x43' not in ask:
        raise ValueError('Quit Game transitions not as expected')
    if answer != REPLY_TEMPLATE:
        raise ValueError("Quit Game's answer handler not as expected")
    request = b'\x01' + be(4) + b'\x13\x00\x5e\x43'  # the argument count (target, message, 2 parameters), then the send
    if ask.count(request) != 1:
        raise ValueError("Quit Game's request not as expected")
    # A third parameter goes in before the count, which becomes 5 (the count alone underflows the stack).
    ask = ask.replace(request, b'\x01' + be(MARK) + b'\x01' + be(5) + b'\x13\x00\x5e\x43')
    sound = transition(s, quit_, UP) + 5 + struct.unpack('>i', arrow[1:5])[0]  # the menu move sound

    code, pieces = bytearray(LABEL + b'\0' if label else b''), {}
    for kind, body in (('entry', highlight[0]), ('update', highlight[1])):
        pieces[kind] = start + len(code)
        code += body.replace(old, new)
    for kind, target in (('up', up), ('down', down)):
        pieces[kind] = start + len(code)
        code += b'\x4a' + struct.pack('>i', sound - (pieces[kind] + 5)) + b'\x47' + be(target) + b'\x49'
    for kind, body in (('confirm', ask), ('reply', answer)):
        pieces[kind] = start + len(code)
        code += body
    return bytes(code), pieces


def row_record(parent, sid, pieces):
    """The row's state record: highlight, the two arrows, confirm and the dialog's answer."""
    triggers = lambda ids: b''.join(be(t | 0x80000000) for t in ids[:-1]) + be(ids[-1])  # noqa: E731
    record = (be(0x04000000 | ROW_SIZE) + be(parent) + be(sid)
              + be(0x01000000 | pieces['entry']) + be(0x02000000 | pieces['update'])
              + be(0x03000000 | pieces['up']) + triggers(UP) + be(0x03000000 | pieces['down']) + triggers(DOWN)
              + be(0x03000000 | pieces['confirm']) + triggers(CONFIRM)
              + be(0x03000000 | pieces['reply']) + triggers(REPLY))
    if len(record) != ROW_SIZE:
        raise ValueError('record size mismatch')
    return record


def moved_copy(s, ref, at, final):
    """The code block at `ref`, laid at code offset `at`: states that move are followed and calls
    are rebased."""
    end = next(r for r in s.code_refs() if r > ref)
    first = s.base + s.lit + ref
    out = bytearray(s.data[first:s.base + s.lit + end])
    for pos, op, arg in decode(s.data, first, s.base + s.lit + end):
        i = pos - first
        if op == 0x47:
            out[i + 1:i + 5] = be(final(int.from_bytes(arg, 'big') + 4) - 4)
        elif op == 0x4a:
            out[i + 1:i + 5] = struct.pack('>i', ref + i + 5 + struct.unpack('>i', arg)[0] - (at + i + 5))
    return bytes(out)


def failsafe_code(s, start, final):
    """(code, {record: (old block, offset of the new one)}): the FailSafe states' two blocks with
    the exit mark's instructions added, to be laid from code offset `start`."""
    out, code, redirects = bytearray(), bytearray(), {}
    for name, trigger, opening, prefix in (('FailSafeDisplay', REPLY, SHOW_BLOCK, SHOW_PREFIX),
                                           ('FailSafeYes', CONFIRM, YES_BLOCK, YES_PREFIX)):
        record = s.one(name)
        ref = transition(s, record, trigger)
        block = s.block_bytes(ref)
        if not block.startswith(opening):
            raise ValueError(f'{name} not as expected')
        at = start + len(code)
        code += prefix + moved_copy(s, ref, at + len(prefix), final)
        redirects[name] = (record, ref, at)
    return bytes(code), redirects


def blank_line(s, record, local):
    """Offset of the 16-bit literal that `record`'s entry assigns to the local `local` while it
    is empty, to be replaced by the label."""
    entry_ref = next(v for k, v, _t in s.blocks(record) if k == 1)
    entry = s.block_bytes(entry_ref)
    found = [i for i in range(len(entry) - 6)
             if entry[i] == 0x16 and entry[i + 3:i + 7] == b'\x09\xe0' + bytes([local]) + b'\x43'
             and s.string(s.lit + int.from_bytes(entry[i + 1:i + 3], 'big')) == '']
    if len(found) != 1:
        raise ValueError(f'line {local - 1} assignment not found')
    return s.base + s.lit + entry_ref + found[0] + 1


def patch(data):
    """Adds the rows to the R_UI.WAD in `data` (a bytearray); ValueError when this is not the
    script the patch was made for (nothing is changed then).

    Both rows are new state records. The pause row's replaces WadChoose's, a cheat-only level
    loader right after PauseMain's subtree; the Options row's goes after HLDisplay. Each record is
    a little bigger than what it replaces or follows, so the state table grows and everything after
    it moves: the header, the sizes of the subtrees the records join and leave, the parent of later
    records, every goto to a later state and the WAD entry are corrected. The code of both rows and
    their label go over WadChoose's dead code, which leaves the literal pool and the symbol table
    where they were relative to the code."""
    base = next((offset + 0x40 for offset, _size, name in entries(data) if name == 'SMI_R_UI'), None)
    if base is None:
        raise ValueError('no SMI_R_UI in this WAD')
    work = bytearray(data)
    s = Script(work, base)
    main, cont, quit_, debug = (s.one(n) for n in ('PauseMain', 'HLContinue', 'HLQuitGame', 'WadChoose'))
    opt, ctl, dsp = (s.one(n) for n in ('PauseOptions', 'HLControls', 'HLDisplay'))
    n = ROW_SIZE
    if main['at'] + main['size'] != debug['at'] or debug['parent'] != main['parent'] or debug['size'] > n:
        raise ValueError('WadChoose is not where the patch expects it')
    if dsp['at'] + dsp['size'] != opt['at'] + opt['size'] or dsp['parent'] != opt['at'] - 4:
        raise ValueError('PauseOptions is not laid out as expected')
    starts = {r['at'] for r in s.records}

    # The two splices, in table order: WadChoose's record is replaced, the Options row is added.
    pause_at, options_at = debug['at'], opt['at'] + opt['size']
    deltas = [(debug['at'] + debug['size'], n - debug['size']), (options_at, n)]  # (first offset it moves, bytes)

    def final(at):
        """Where the old record starting at `at` ends up."""
        return at + sum(delta for end, delta in deltas if end <= at)

    pause_ref, options_ref = pause_at - 4, options_at + deltas[0][1] - 4  # state refs of the new records
    fref = lambda record: final(record['at']) - 4  # noqa: E731

    # WadChoose's code is dead once its record is replaced: nothing else may use it.
    refs = s.code_refs()
    own = [v for k, v in debug['items'] if k in (1, 2, 3)]
    start, end = min(own), next(r for r in refs if r > max(own))
    if any(start <= r < end for r in refs if r not in own):
        raise ValueError('WadChoose code is shared')
    # No call into it from elsewhere (a 0x4a byte that is not an opcode can only refuse too much).
    for i in range(base + s.lit, base + s.symtab - 5):
        if work[i] == 0x4a and not start <= i - base - s.lit < end:
            target = i - base - s.lit + 5 + struct.unpack_from('>i', work, i + 1)[0]
            if start <= target < end:
                raise ValueError('WadChoose code is called from elsewhere')
    pause_code, pause_pieces = row_code(s, quit_, start, up=fref(quit_), down=fref(cont), slot=b'menuItem5Select')
    options_start = start + len(pause_code)
    options_code, options_pieces = row_code(s, quit_, options_start, up=fref(dsp), down=fref(ctl),
                                            slot=b'menuItem4Select', label=False)
    failsafe_start = options_start + len(options_code)
    failsafe, redirects = failsafe_code(s, failsafe_start, final)
    if failsafe_start + len(failsafe) > end:
        raise ValueError('no room for the new rows')

    # Edits in the layout as it is now. First every goto to a state that moves (or to WadChoose,
    # which the pause row replaces), then the rows' own wiring.
    edits = []
    for a, b in s.spans():
        if start <= a < end:  # WadChoose's dead code: the new code goes over it
            continue
        for at, op, arg in decode(work, base + s.lit + a, base + s.lit + b):
            if op == 0x47:
                target = int.from_bytes(arg, 'big') + 4
                moved = pause_ref if target == debug['at'] else final(target) - 4
                if moved != target - 4:
                    edits.append((at + 1, be(moved)))
    edits += [retarget(s, cont, UP, quit_['at'] - 4, pause_ref), retarget(s, quit_, DOWN, cont['at'] - 4, pause_ref),
              retarget(s, ctl, UP, dsp['at'] - 4, options_ref), retarget(s, dsp, DOWN, ctl['at'] - 4, options_ref),
              (blank_line(s, main, 6), be(start, 2)),  # PauseMain clears line5 (local 6): the label goes there
              (blank_line(s, opt, 5), be(start, 2)),   # PauseOptions clears line4 (local 5)
              (base + s.lit + start, pause_code + options_code + failsafe)]
    for record, ref, at in redirects.values():  # the FailSafe states run their new blocks
        item = next(i for i, (k, v) in enumerate(record['items']) if k == 3 and v == ref)
        edits.append((base + record['at'] + 12 + 4 * item, be(0x03000000 | at)))
    parents = {r['at']: r for r in s.records}
    grow = {}  # sizes of the subtrees the new records join (+) and the replaced one leaves (-)

    def chain(record):
        out = [record]
        while out[-1]['parent'] + 4 in starts:
            out.append(parents[out[-1]['parent'] + 4])
        return out
    for record, sign, amount in ((main, 1, n), (opt, 1, n), (parents[debug['parent'] + 4], -1, debug['size'])):
        for r in chain(record):
            grow[r['at']] = grow.get(r['at'], 0) + sign * amount
    edits += [(base + at, be(0x04000000 | (parents[at]['size'] + g))) for at, g in grow.items() if g]
    for r in s.records:
        if r['at'] != debug['at'] and r['parent'] + 4 in starts and final(r['parent'] + 4) != r['parent'] + 4:
            edits.append((base + r['at'] + 4, be(final(r['parent'] + 4) - 4)))
    for at, value in edits:
        work[at:at + len(value)] = value

    old_size = struct.unpack_from('<I', work, base - 0x40 + 4)[0]
    grown = sum(delta for _end, delta in deltas)
    work[base + options_at:base + options_at] = row_record(fref(opt), dsp['id'], options_pieces)
    work[base + pause_at:base + pause_at + debug['size']] = row_record(fref(main), debug['id'], pause_pieces)
    for offset in (0x08, 0x0c, 0x10):  # the script's size, code start and symbol table
        struct.pack_into('<I', work, base + offset, struct.unpack_from('<I', work, base + offset)[0] + grown)
    struct.pack_into('<I', work, base - 0x40 + 4, old_size + grown)
    work[base + old_size + grown:base + old_size + grown + -old_size % 16] = bytes(-(old_size + grown) % 16)
    data[:] = work


def build(game, out, source=None):
    """{guest path: patched copy} for run.py to mount, from `source` (another patched copy) or the
    game's own R_UI.WAD; {} when the script is not the one this patch knows. The copy in
    out/pause_menu is kept until the source changes."""
    source = Path(source or Path(game) / WAD)
    target = Path(out) / 'pause_menu'
    copy = target / Path(WAD).name
    if not source.is_file():
        return {}
    stamp = json.dumps([VERSION, str(source), source.stat().st_size, source.stat().st_mtime_ns])
    stamp_file = target / 'stamp.json'
    if not (stamp_file.is_file() and stamp_file.read_text() == stamp):
        shutil.rmtree(target, ignore_errors=True)
        target.mkdir(parents=True)
        data = bytearray(source.read_bytes())
        try:
            patch(data)
        except ValueError as error:
            print(f'Pause menu: {error}; no Exit to Desktop row', flush=True)
            return {}
        copy.write_bytes(data)
        stamp_file.write_text(stamp)  # last: an interrupted build is redone
    if not copy.is_file():
        return {}
    return {'/app0/' + WAD: copy}
