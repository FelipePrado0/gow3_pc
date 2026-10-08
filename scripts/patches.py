"""Compile selected shadPS4/GoldHEN XML patches into out/patches.bin for the loader.

Patch addresses are PS4 virtual addresses (eboot base 0x400000); the loader's
image places eboot vaddr 0 at image offset 0. Only literal writes are supported
(bytes, bytes16/32/64, float32/64, utf8, utf16); pattern ("mask") patches are rejected.
"""
import argparse
import json
import os
import re
import struct
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

EBOOT_BASE=0x400000


def game_app_version(game):
    """APP_VER of the game folder's param.sfo ("01.02"), or None when it cannot be read."""
    try:
        from prepare import sfo
        return sfo((Path(game) / 'sce_sys/param.sfo').read_bytes()).get('APP_VER')
    except (OSError, ValueError, ImportError):
        return None


def eboot_segments(elf):
    phoff,=struct.unpack_from('<Q',elf,0x20)
    phentsize,phnum=struct.unpack_from('<HH',elf,0x36)
    segments=[]
    for i in range(phnum):
        kind,_,_,vaddr,_,_,memsz,_=struct.unpack_from('<IIQQQQQQ',elf,phoff+i*phentsize)
        if kind==1: segments.append((vaddr,vaddr+memsz))
    return segments


def encode(line):
    kind,value=line.get('Type'),line.get('Value')
    if kind=='bytes': return bytes.fromhex(value.replace(' ',''))
    if kind in ('bytes16','bytes32','bytes64'):
        return int(value,0).to_bytes(int(kind[5:])//8,'little')
    if kind=='float32': return struct.pack('<f',float(value))
    if kind=='float64': return struct.pack('<d',float(value))
    if kind=='utf8': return value.encode()+b'\0'
    if kind=='utf16': return value.encode('utf-16-le')+b'\0\0'
    raise ValueError(f'unsupported patch type {kind!r}')


def compile_patches(xml, names, app_version, segments):
    found={}
    for meta in ET.parse(xml).getroot().iter('Metadata'):
        if meta.get('Name') in names and meta.get('AppVer')==app_version and meta.get('AppElf','eboot.bin')=='eboot.bin':
            found[meta.get('Name')]=meta
    missing=[n for n in names if n not in found]
    if missing: raise ValueError(f'patches not found for app version {app_version}: {missing}')
    writes=[]
    for name in names:
        for line in found[name].iter('Line'):
            offset=int(line.get('Address'),0)-EBOOT_BASE
            data=encode(line)
            if not any(start<=offset and offset+len(data)<=end for start,end in segments):
                raise ValueError(f'{name}: address {line.get("Address")} is outside the eboot')
            writes.append((offset,data))
    return writes


PATCHES_DIR=Path(__file__).resolve().parent.parent/'patches'
# Built-in patch file per game, the app version its addresses are for and, when known, the
# sha256 of the only eboot.bin they were checked against (another build of the same version has
# its code elsewhere: byte writes there corrupt it).
GAMES=[({'CUSA01623'},PATCHES_DIR/'God_of_War_III_Remastered.xml','01.02',
        'd85c8135d330c3b601bf5dc3f1dd86bd6119fb8a9fce372bed2c9785f9a79299')]
DMEM_RETAIL_MB=5056  # runtime_memory.c POOL_SIZE default


def game_title_id(game):
    try:
        from prepare import sfo
        return sfo((Path(game) / 'sce_sys/param.sfo').read_bytes()).get('TITLE_ID')
    except (OSError, ValueError, ImportError):
        return None


def game_profile(title_id):
    """(title IDs, built-in XML, app version, eboot sha256 or None) for a known game, else None."""
    return next((g for g in GAMES if title_id in g[0]),None)


def eboot_matches(game, profile):
    """Whether the game's eboot.bin is the build the profile's patches were checked against."""
    if not profile or not profile[3]:
        return True
    import hashlib
    try:
        return hashlib.sha256((Path(game)/'eboot.bin').read_bytes()).hexdigest()==profile[3]
    except OSError:
        return False


# God of War III notes: the texture fix and the resolution patches both resize the video arena;
# enable one, never both. A chosen resolution patch replaces the default texture fix.
EXCLUSIVE_PREFIX='Resolution Patch'
REPLACED_BY_EXCLUSIVE={'Bug Fix - Texture Corruption Fix'}


def selected_patches(xml, app_version, extra, only=False):
    """The file's isEnabled patches for this version, then the names in extra (";"-separated).
    only: extra is the whole selection (the launcher's), without the file's defaults."""
    names=[] if only else [m.get('Name') for m in ET.parse(xml).getroot().iter('Metadata')
                           if m.get('AppVer')==app_version and m.get('isEnabled','false').lower()=='true']
    names+=[n for n in (n.strip() for n in extra.split(';')) if n and n not in names]
    exclusive=[n for n in names if n.startswith(EXCLUSIVE_PREFIX)]
    if len(exclusive)>1:
        raise ValueError(f'choose one resolution patch, not {exclusive}')
    return [n for n in names if not (exclusive and n in REPLACED_BY_EXCLUSIVE)]


def patch_requirements(xml, names, app_version):
    """(direct memory MiB, VBlank Hz) the selected patches' notes ask for, None when not needed.
    shadPS4 notes say "Requires setting DMEM to N MB": N is on top of the retail pool."""
    dmem=vblank=None
    for meta in ET.parse(xml).getroot().iter('Metadata'):
        if meta.get('Name') not in names or meta.get('AppVer')!=app_version: continue
        note=meta.get('Note','')
        if m:=re.search(r'DMEM to (\d+) MB',note): dmem=max(dmem or 0,DMEM_RETAIL_MB+int(m.group(1)))
        if m:=re.search(r'VBlank to (\d+) FPS',note): vblank=max(vblank or 0,int(m.group(1)))
    return dmem,vblank


def external_patches(directory, app_version, exclude, ids):
    """[(key, file, metadata)] of eboot patches for this version in directory/*.xml (third-party
    shadPS4/GoldHEN files in the data directory's patches/ folder).
    key is "<file name>/<patch name>" (the launcher's selection, patches.json)."""
    found=[]
    directory=Path(directory)
    for path in sorted(directory.glob('*.xml')) if directory.is_dir() else []:
        if path.resolve()==Path(exclude).resolve(): continue  # the built-in file (patches/ in a checkout)
        try:
            root=ET.parse(path).getroot()
        except ET.ParseError as error:
            print(f'Patches: {path.name}: {error}',file=sys.stderr)
            continue
        file_ids={e.text.strip() for e in root.iter('ID') if e.text}
        if file_ids and not file_ids&ids: continue
        for meta in root.iter('Metadata'):
            if meta.get('AppVer')==app_version and meta.get('AppElf','eboot.bin')=='eboot.bin':
                found.append((f'{path.name}/{meta.get("Name")}',path,meta))
    return found


def external_selection(found, config):
    """Selected external patches: patches.json {"enabled": [...], "disabled": [...]} overrides
    each file's isEnabled."""
    settings={}
    if config and Path(config).is_file():
        settings=json.loads(Path(config).read_text())
    enabled,disabled=set(settings.get('enabled',[])),set(settings.get('disabled',[]))
    return [(key,path,meta) for key,path,meta in found
            if key in enabled or (key not in disabled and meta.get('isEnabled','false').lower()=='true')]


def compile_external(selected, segments):
    """Writes of the selected external patches; a patch with unsupported lines is skipped whole."""
    writes=[]
    for key,_,meta in selected:
        try:
            ours=[]
            for line in meta.iter('Line'):
                offset=int(line.get('Address') or '',0)-EBOOT_BASE
                data=encode(line)
                if not any(start<=offset and offset+len(data)<=end for start,end in segments):
                    raise ValueError(f'address {line.get("Address")} is outside the eboot')
                ours.append((offset,data))
        except ValueError as error:
            print(f'Patches: skipped {key}: {error}',file=sys.stderr)
            continue
        writes+=ours
        print(f'Patches: external {key} ({len(ours)} writes)')
    return writes


def write_patches(out, writes):
    # BBPATCH2: the patch base, so the loader can rebase pointers the patches write into
    # relocated slots.
    blob=struct.pack('<8sQQ',b'BBPATCH2',EBOOT_BASE,len(writes))
    for offset,data in writes: blob+=struct.pack('<QQ',offset,len(data))+data
    (out/'patches.bin').write_bytes(blob)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--patches-dir',type=Path,help='third-party patch XML files (shadPS4 format)')
    p.add_argument('--patches-config',type=Path,help='patches.json: enabled/disabled external patches')
    p.add_argument('--extra',default='',help='additional patch names, separated by ";"')
    p.add_argument('--out',type=Path,default=Path(__file__).resolve().parent.parent/'out')
    p.add_argument('--game-dir',type=Path,default=Path(os.environ.get('BB_GAME_DIR','../CUSA01623')))
    a=p.parse_args()
    title=game_title_id(a.game_dir)
    profile=game_profile(title)
    if not profile:
        write_patches(a.out,[])
        print(f'Patches: no patch profile for {title}: none applied')
        return
    ids,xml,needed,_sha=profile
    # The patches are byte writes at the addresses of one game build: on another they would
    # corrupt code, so such a game runs unpatched.
    version=game_app_version(a.game_dir)
    if version!=needed and not os.environ.get('BB_FORCE_PATCHES'):
        write_patches(a.out,[])
        print(f'Patches: game version {version}, {xml.name} is for {needed}: none applied')
        return
    if not eboot_matches(a.game_dir,profile) and not os.environ.get('BB_FORCE_PATCHES'):
        write_patches(a.out,[])
        print(f'Patches: eboot.bin is not the build {xml.name} was checked against: none applied')
        return
    names=selected_patches(xml,needed,a.extra,os.environ.get('BB_PATCHES_ONLY')=='1')
    segments=eboot_segments((a.out/'eboot.elf').read_bytes())
    writes=compile_patches(xml,names,needed,segments)
    if a.patches_dir:
        # After the built-in ones: an external patch of the same bytes wins.
        writes+=compile_external(external_selection(external_patches(a.patches_dir,needed,xml,ids),
                                                    a.patches_config),segments)
    write_patches(a.out,writes)
    print(f'Patches: {title}; {len(writes)} writes from {names or "none"}')


if __name__=='__main__':
    main()
