"""Compile selected shadPS4/GoldHEN XML patches into out/patches.bin for the loader.

Patch addresses are PS4 virtual addresses (eboot base 0x400000); the loader's
image places eboot vaddr 0 at image offset 0. Only literal writes are supported
(bytes, bytes16/32/64, float32/64, utf8, utf16); pattern ("mask") patches are rejected.
The built-in file's lines carry signatures (Pattern/At) that find each site in any build.
"""
import argparse
import hashlib
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
    return game_sfo(game).get('APP_VER')


def eboot_segments(elf):
    phoff,=struct.unpack_from('<Q',elf,0x20)
    phentsize,phnum=struct.unpack_from('<HH',elf,0x36)
    segments=[]
    for i in range(phnum):
        kind,_,_,vaddr,_,_,memsz,_=struct.unpack_from('<IIQQQQQQ',elf,phoff+i*phentsize)
        if kind==1: segments.append((vaddr,vaddr+memsz))
    return segments


def eboot_image(elf):
    """The PT_LOAD file bytes of eboot.elf at their virtual addresses (image offsets)."""
    phoff,=struct.unpack_from('<Q',elf,0x20)
    phentsize,phnum=struct.unpack_from('<HH',elf,0x36)
    loads=[struct.unpack_from('<IIQQQQQQ',elf,phoff+i*phentsize) for i in range(phnum)]
    loads=[(vaddr,offset,filesz) for kind,_,offset,vaddr,_,filesz,_,_ in loads if kind==1]
    image=bytearray(max(vaddr+filesz for vaddr,_,filesz in loads))
    for vaddr,offset,filesz in loads: image[vaddr:vaddr+filesz]=elf[offset:offset+filesz]
    return bytes(image)


# Signatures find a patch site in any build whose code matches, wherever the code moved: the
# original bytes around the site, "??" where a build may differ (rip-relative displacements,
# rel32 branch targets). tools/make_signatures.py writes them from the reference build.
def parse_pattern(text):
    return [None if b=='??' else int(b,16) for b in text.split()]


def find_pattern(image, pattern, limit=2):
    """Image offsets where pattern matches, at most limit of them."""
    runs,start=[],None
    for i,b in enumerate(pattern+[None]):
        if b is not None and start is None: start=i
        if b is None and start is not None: runs.append((start,i)); start=None
    if not runs: raise ValueError('pattern without fixed bytes')
    a,b=max(runs,key=lambda r:r[1]-r[0])
    anchor=bytes(pattern[a:b])
    found,at=[],image.find(anchor)
    while at>=0 and len(found)<limit:
        s=at-a
        if s>=0 and s+len(pattern)<=len(image) and all(p is None or image[s+i]==p for i,p in enumerate(pattern)):
            found.append(s)
        at=image.find(anchor,at+1)
    return found


def locate(image, element, prefix=''):
    """Image offset of a Line's or Target's site: its Pattern (unique) plus At, else Address."""
    pattern=element.get(prefix+'Pattern')
    if not pattern:
        return int(element.get('Address'),0)-EBOOT_BASE
    found=find_pattern(image,parse_pattern(pattern))
    if len(found)!=1:
        raise LookupError('signature not found' if not found else 'signature found more than once')
    return found[0]+int(element.get(prefix+'At','0'),0)


def resolve_targets(image, meta, sites):
    """Image offsets of a patch's named Targets: a site (Pattern/At), or the address a rip-relative
    or rel32 operand points to (Ref: the operand's offset in its Pattern; Tail: the instruction's
    bytes after the operand)."""
    targets={}
    for target in meta.iter('Target'):
        name=target.get('Name')
        if target.get('Line'):
            targets[name]=sites[int(target.get('Line'))]
            continue
        at=locate(image,target)
        if target.get('Ref'):
            disp,=struct.unpack_from('<i',image,at)
            at=at+4+int(target.get('Tail','0'))+disp
        targets[name]=at
    return targets


def fill(value, at, targets):
    """Value bytes with each {rel:NAME:TAIL} replaced by the rel32 to target NAME, measured from the
    end of the instruction (TAIL bytes after the operand)."""
    out=bytearray()
    for part in re.split(r'(\{[^}]*\})',value.replace(' ','')):
        if not part.startswith('{'):
            out+=bytes.fromhex(part)
            continue
        _,name,tail=part[1:-1].split(':')
        out+=struct.pack('<i',targets[name]-(at+len(out)+4+int(tail)))
    return bytes(out)


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


def compile_patches(xml, names, app_version, segments, image=None, skipped=None):
    """Writes of the named patches. With the game's image, each site is found by its signature
    (any build); without it, at the reference build's Address. A Line with a Target writes at that
    Target instead: data the code points to, such as a .bss variable (no bytes of its own to find).
    A patch whose sites are not all found is left out whole, with the reason in skipped[name].
    app_version None: any version."""
    found={}
    for meta in ET.parse(xml).getroot().iter('Metadata'):
        if meta.get('Name') in names and app_version in (None,meta.get('AppVer')) and meta.get('AppElf','eboot.bin')=='eboot.bin':
            found[meta.get('Name')]=meta
    missing=[n for n in names if n not in found]
    if missing: raise ValueError(f'patches not found for app version {app_version}: {missing}')
    writes=[]
    for name in names:
        lines=list(found[name].iter('Line'))
        try:
            sites=[locate(image,line) if image is not None else int(line.get('Address'),0)-EBOOT_BASE
                   for line in lines]
            targets=(resolve_targets(image,found[name],sites) if image is not None else
                     {t.get('Name'):int(t.get('Address'),0)-EBOOT_BASE for t in found[name].iter('Target')})
            sites=[targets[line.get('Target')] if line.get('Target') else site for line,site in zip(lines,sites)]
        except LookupError as error:
            if skipped is None: raise ValueError(f'{name}: {error}')
            skipped[name]=str(error)
            continue
        for line,offset in zip(lines,sites):
            data=fill(line.get('Value'),offset,targets) if '{' in line.get('Value','') else encode(line)
            if not any(start<=offset and offset+len(data)<=end for start,end in segments):
                raise ValueError(f'{name}: address {offset+EBOOT_BASE:#x} is outside the eboot')
            writes.append((offset,data))
    return writes


PATCHES_DIR=Path(__file__).resolve().parent.parent/'patches'
# Built-in patch file per game: its title IDs, the AppVer its metadata is tagged with and the
# title that also recognizes a release under an ID not listed. The signatures find the sites in
# any region or version whose code matches, so the eboot.bin build is not checked.
GAMES=[({'CUSA01623','CUSA01715','CUSA01720'},PATCHES_DIR/'God_of_War_III_Remastered.xml','01.02',
        'God of War III')]
# The build the built-in Address values and third-party patch files are written for.
REFERENCE_SHA256='d85c8135d330c3b601bf5dc3f1dd86bd6119fb8a9fce372bed2c9785f9a79299'
DMEM_RETAIL_MB=5056  # runtime_memory.c POOL_SIZE default


def game_sfo(game):
    """The values of the game folder's param.sfo, or {} when it cannot be read (prepare.read_sfo)."""
    if not game:
        return {}
    try:
        from prepare import read_sfo
    except ImportError:
        return {}
    return read_sfo(Path(game) / 'sce_sys/param.sfo') or {}


def game_title_id(game):
    return game_sfo(game).get('TITLE_ID')


def game_profile(title_id, title=None):
    """(title IDs, built-in XML, metadata AppVer, title) for a known game, else None."""
    title=(title or '').replace('®','').replace('™','')
    return next((g for g in GAMES if title_id in g[0] or (title and g[3] in title)),None)


def game_profile_for(game):
    """The profile of a game folder, by title ID or by the title in its param.sfo."""
    values=game_sfo(game) if game else {}
    return game_profile(values.get('TITLE_ID'),values.get('TITLE'))
_SHA_CACHE={}


def eboot_sha256(game):
    try:
        path=Path(game)/'eboot.bin'
        stat=path.stat()
        key=(str(path),stat.st_mtime_ns,stat.st_size)
        if key not in _SHA_CACHE:
            _SHA_CACHE[key]=hashlib.sha256(path.read_bytes()).hexdigest()
        return _SHA_CACHE[key]
    except (OSError, TypeError):
        return None


def game_patchable(game):
    """Whether the built-in patches are tried on this game folder (any region and version)."""
    return bool(game) and bool(game_profile_for(game))


def patch_status(out):
    """The last start's patch report (patches.py writes it): {'applied': [...], 'skipped': {...}}."""
    try:
        return json.loads((Path(out)/'patch_status.json').read_text())
    except (OSError, ValueError):
        return None


def available_patches(xml, app_version):
    """Names of the patches the XML has for this app version."""
    return {m.get('Name') for m in ET.parse(xml).getroot().iter('Metadata') if m.get('AppVer')==app_version}


# God of War III notes: the texture fix and the resolution patches both resize the video arena;
# enable one, never both. A chosen resolution patch replaces the default texture fix.
EXCLUSIVE_PREFIX='Resolution Patch'
REPLACED_BY_EXCLUSIVE={'Bug Fix - Texture Corruption Fix'}
# Fixes with nothing to choose: not offered in the launcher and applied even with its saved list,
# which predates them (the startup black screen fix sets a value the game overwrites at once).
ALWAYS_ON={'Bug Fix - Startup Black Screen'}


def selected_patches(xml, app_version, extra, only=False):
    """The file's isEnabled patches for this version, then the names in extra (";"-separated).
    only: extra is the whole selection (the launcher's), without the file's defaults but with
    the ALWAYS_ON ones."""
    metas=[m for m in ET.parse(xml).getroot().iter('Metadata') if m.get('AppVer')==app_version]
    names=[] if only else [m.get('Name') for m in metas if m.get('isEnabled','false').lower()=='true']
    names+=[n for n in (n.strip() for n in extra.split(';')) if n and n not in names]
    names+=[m.get('Name') for m in metas if m.get('Name') in ALWAYS_ON and m.get('Name') not in names]
    exclusive=[n for n in names if n.startswith(EXCLUSIVE_PREFIX)]
    if len(exclusive)>1:
        raise ValueError(f'choose one resolution patch, not {exclusive}')
    return [n for n in names if not (exclusive and n in REPLACED_BY_EXCLUSIVE)]


def resolution_names(xmls):
    """Sizes ("3440x1440") of the "Resolution Patch - <size>" entries of these patch files, smallest first."""
    found = []
    for path in xmls:
        try:
            root = ET.parse(path).getroot()
        except (OSError, ET.ParseError):
            continue
        for meta in root.iter('Metadata'):
            name = meta.get('Name') or ''
            if name.startswith(EXCLUSIVE_PREFIX) and name[len(EXCLUSIVE_PREFIX):].startswith(' - '):
                size = name[len(EXCLUSIVE_PREFIX) + 3:].strip()
                if size and size not in found:
                    found.append(size)
    def pixels(size):
        width, _, height = size.partition('x')
        return (int(width) * int(height), size) if width.isdigit() and height.isdigit() else (1 << 60, size)
    return sorted(found, key=pixels)


def merge_patch_files(base, extras, out):
    """Writes `out`: the patches of `base` plus those of the mods' patch files (`extras`, in load order).
    A patch with a name that is already there is replaced; a file for other titles or one that does not
    parse is left out with a note. Returns `out`."""
    tree = ET.parse(base)
    root = tree.getroot()
    ids = {e.text.strip() for e in root.iter('ID') if e.text}
    for path in extras:
        try:
            extra = ET.parse(path).getroot()
        except (OSError, ET.ParseError) as error:
            print(f'Patches: {Path(path).name}: {error}', file=sys.stderr)
            continue
        extra_ids = {e.text.strip() for e in extra.iter('ID') if e.text}
        if extra_ids and not extra_ids & ids:
            continue
        for meta in extra.iter('Metadata'):
            name = meta.get('Name')
            for old in [m for m in root.iter('Metadata') if m.get('Name') == name]:
                root.remove(old)
            root.append(meta)
            print(f'Patches: {name} from the mod file {Path(path).parent.parent.name}/{Path(path).name}')
    ET.indent(tree)
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    tree.write(out, encoding='utf-8', xml_declaration=True)
    return Path(out)


def default_enabled(xmls, app_version):
    """Names of the patches of these files that are on by default (isEnabled), for this app version."""
    names = []
    for path in xmls:
        try:
            root = ET.parse(path).getroot()
        except (OSError, ET.ParseError):
            continue
        names += [m.get('Name') for m in root.iter('Metadata')
                  if m.get('AppVer') == app_version and m.get('isEnabled', 'false').lower() == 'true']
    return names


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
        if path.resolve()==Path(exclude).resolve() or path.resolve() in {g[1].resolve() for g in GAMES}:
            continue  # built-in files (patches/ in a checkout) are not also offered as external
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
    # G3PATCH2: the patch base, so the loader can rebase pointers the patches write into
    # relocated slots.
    blob=struct.pack('<8sQQ',b'G3PATCH2',EBOOT_BASE,len(writes))
    for offset,data in writes: blob+=struct.pack('<QQ',offset,len(data))+data
    (out/'patches.bin').write_bytes(blob)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--patches-dir',type=Path,help='third-party patch XML files (shadPS4 format)')
    p.add_argument('--patches-config',type=Path,help='patches.json: enabled/disabled external patches')
    p.add_argument('--mod-patches',type=Path,action='append',default=[],
                   help='patch file of an enabled mod (repeat; in load order)')
    p.add_argument('--extra',default='',help='additional patch names, separated by ";"')
    p.add_argument('--out',type=Path,default=Path(__file__).resolve().parent.parent/'out')
    p.add_argument('--game-dir',type=Path,default=Path(os.environ.get('GOW3_GAME_DIR','../CUSA01623')))
    a=p.parse_args()
    title=game_title_id(a.game_dir)
    profile=game_profile_for(a.game_dir)
    if not profile:
        write_patches(a.out,[])
        print(f'Patches: no patch profile for {title}: none applied')
        return
    ids,xml,tag,_name=profile
    if a.mod_patches:
        xml=merge_patch_files(xml,a.mod_patches,a.out/'patches-merged.xml')
    sha=eboot_sha256(a.game_dir)
    print(f'Patches: {title} {game_app_version(a.game_dir)}, eboot.bin sha256 {sha}')
    names=selected_patches(xml,tag,a.extra,os.environ.get('GOW3_PATCHES_ONLY')=='1')
    elf=(a.out/'eboot.elf').read_bytes()
    segments=eboot_segments(elf)
    skipped={}
    writes=compile_patches(xml,names,None,segments,eboot_image(elf),skipped)
    for name,reason in skipped.items():
        print(f'Patches: {name} skipped ({reason} in this eboot.bin)')
    if a.patches_dir and sha==REFERENCE_SHA256:
        # Third-party files use the absolute addresses of the reference build, so they are only
        # applied there. After the built-in ones: an external patch of the same bytes wins.
        writes+=compile_external(external_selection(external_patches(a.patches_dir,tag,xml,ids),
                                                    a.patches_config),segments)
    write_patches(a.out,writes)
    applied=[n for n in names if n not in skipped]
    (a.out/'patch_status.json').write_text(json.dumps({'sha256':sha,'applied':applied,'skipped':skipped}))
    print(f'Patches: {title}; {len(writes)} writes from {applied or "none"}')

if __name__=='__main__':
    main()
