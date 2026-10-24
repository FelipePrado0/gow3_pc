# Game patches

**English** | [Português](PATCHES.pt-BR.md) | [Español](PATCHES.es.md)

Patches change the game's code and data **in memory**, when the game starts. They fix bugs, change
the render resolution, raise the frame rate and skip videos. The game folder is never changed: the
patched bytes are written into the copy of the program that the port loads. To change textures,
sounds or text instead, see the [mods guide](MODS.md).

**Contents:** [Built-in patches](#built-in-patches) · [Where to choose them](#where-to-choose-them) ·
[What happens at start](#what-happens-at-start) · [Reading the log](#reading-the-log) ·
[When a patch does not apply](#when-a-patch-does-not-apply) ·
[Writing a patch](#writing-a-patch) · [Patches in mods](#patches-in-mods) ·
[Third-party patches](#third-party-patches-shadps4-and-goldhen) · [Credits](#credits)

## Built-in patches

The built-in patches are in `patches/CUSA01623.xml`. They are for CUSA01623 and
CUSA01715, version 01.02, and they find their sites by **signature** (the original bytes around
each site), so they also work on other builds whose code matches.

| Patch | What it does | Who turns it on | Author |
|---|---|---|---|
| **Bug Fix - Startup Black Screen** | Fixes the black screen at start with the FPS counter running: the frame pacing index starts at 2 instead of 0 | Always applied; not shown in the launcher | SpartanPort |
| **Bug Fix - Texture Corruption Fix** | Fixes corrupted textures by giving the game a larger video arena (more video memory) | Applied whenever the render resolution is **Native**; a resolution patch replaces it | cuesta4 |
| **Skip Any Video With X Button** | Lets you skip any cinematic with Cross | **Game** page, on by default | cuesta4 |
| **Skip Intro** | Skips the logos at start | **Game** page, off by default | cuesta4 |
| **Resolution Patch - &lt;size&gt;** (21 sizes, from 480p to 5120 × 2160) | Sets the render resolution and the video arena; the ultrawide sizes also widen the field of view (Hor+) | **Render resolution** (launcher **Performance** page, or the in-game menu); one at a time | kvicken + cuesta4 (480p, 720p, 1440p, 1800p, 4K); SpartanPort (900p and the ultrawide, 16:10 and 4:3 sizes) |
| **Frame Rate Patch - 240 FPS** | Raises the engine's frame rate limit and corrects the swing energy so it matches the original 60 Hz simulation | Applied on every start; the chosen **Engine frame rate** (60, 100, 120, 144, 180 or 240) is then written live | kvicken, SpartanPort |
| Frame Rate Patch - 120 FPS / 144 FPS / 100 FPS | The same patch with a limit of 120, 144 or 100 | Not used by the launcher any more; kept for other uses of the file | kvicken / kvicken, SpartanPort |

Notes:

- **Resolution and texture fix.** Both change the size of the video arena, so the game takes one or
  the other, never both. At **Native**, the texture fix is always added, even when its checkbox on
  the **Game** page is cleared.
- **Memory.** A patch note such as "Requires setting DMEM to 3072 MB" makes the port reserve that
  much more direct memory, on top of the 5056 MB the retail game has. Nothing has to be set by hand.
- **Frame rate.** The engine rate changes while the game runs (in-game menu, **R3 + L1 + R1** or
  **F9**): every start takes the 240 FPS patch and the port writes the chosen rate into the game.
  144 FPS is intended for 144 Hz displays; with VSync, it keeps presentation aligned with that refresh rate.
  If the 240 FPS patch is not found in your `eboot.bin`, the highest frame rate patch that is found
  is used; if none is found, the engine stays at the original 60 FPS.

## Where to choose them

**Game** page, **Game patches**: the patches you can turn on and off, with their author and note.
Resolution and frame rate patches are not listed here: they follow **Performance**.

![The Game patches section of the launcher's Game page](images/game-patches.png)

- **Render resolution**: launcher **Performance** page, or the in-game menu (Insert or R3 + L2 →
  **Performance**). A new resolution applies on the next start (**Apply and restart** in the game).
- **Engine frame rate**: launcher **Performance** page, or the in-game menu, where it changes live.
- **Patches in mods** come with the mod while it is enabled ([Patches in mods](#patches-in-mods)).
- **Third-party patches** are on the **Mods** page ([Third-party patches](#third-party-patches-shadps4-and-goldhen)).

## What happens at start

1. The launcher builds the list: the **Game** page choices, the always-on fixes, the resolution and
   frame rate patches of **Performance**, and the patches of the enabled mods that are on by default.
2. `scripts/patches.py` looks for every site of every patch in your `eboot.bin`, by its signature.
   A patch is applied only when **all** of its sites are found; otherwise it is left out whole and
   the log says why.
3. The writes go into `out/patches.bin`, and the loader applies them to the game's code in memory
   before the game runs. Third-party patches are applied after the built-in ones.
4. The memory and VBlank that the applied patches' notes ask for are set for this start.

## Reading the log

`user/last_run.log` (or **Advanced → Log** in the launcher) shows what was applied:

```text
Patches: CUSA01623 01.02, eboot.bin sha256 d85c8135d330c3b601bf5dc3f1dd86bd6119fb8a9fce372bed2c9785f9a79299
Patches: CUSA01623; 10 writes from ['Bug Fix - Texture Corruption Fix', 'Skip Intro', 'Skip Any Video With X Button', 'Bug Fix - Startup Black Screen', 'Frame Rate Patch - 240 FPS']
Engine frame rate: 120 FPS, live up to 240
```

| Line | Meaning |
|---|---|
| `Patches: <title> <version>, eboot.bin sha256 …` | The game and the exact build of your `eboot.bin` |
| `Patches: <name> skipped (signature not found in this eboot.bin)` | That patch's code is not in your build; it was left out |
| `Patches: <name> is not available for this game build: skipped` | The patch file has no patch with that name for this version |
| `Patches: using Frame Rate Patch - <n> FPS instead` | The 240 FPS patch is missing; a lower one is used |
| `Patches: <name> from the mod file <mod>/<file>.xml` | A mod's patch was added, or replaced a built-in one with the same name |
| `Patches: external <file>.xml/<name> (<n> writes)` | A third-party patch was applied |
| `Patches: skipped <file>.xml/<name>: <reason>` | A third-party patch was left out (unsupported line or address outside the eboot) |
| `Patches: <title>; <n> writes from [...]` | The patches that were applied |

## When a patch does not apply

| What you see | Why | What to do |
|---|---|---|
| `skipped (signature not found …)` | Your `eboot.bin` is a build whose code differs at that site | Nothing to set: the game starts without that patch. Report the sha256 from the log in an issue |
| `signature found more than once` | The pattern is not unique in your build | A patch author has to make the pattern longer |
| The 240 FPS patch is skipped and the engine stays at 60 FPS | No frame rate patch matches your build | Report the sha256 from the log |
| A third-party patch does nothing | Third-party patches only apply to the reference build (below) | Check the sha256 line in the log |
| The game does not start and the log says `choose one resolution patch, not [...]` | `SPARTAN_PATCHES` (terminal) names two resolution patches | Name only one; the launcher always passes the one chosen in **Render resolution** |
| The game crashes at start after adding a patch | The patch writes the wrong bytes for your build | Disable it; patches with signatures are safer than fixed addresses |

## Writing a patch

Patches use the shadPS4/GoldHEN XML format. A file has the title IDs it is for and one `Metadata`
per patch:

```xml
<?xml version="1.0"?>
<Patch>
    <TitleID>
        <ID>CUSA01623</ID>
        <ID>CUSA01715</ID>
    </TitleID>
    <Metadata Title="SpartanPort" Name="Skip Any Video With X Button" Note="Allows skipping any cinematic with Cross." Author="cuesta4" PatchVer="1.0" AppVer="01.02" AppElf="eboot.bin" isEnabled="true">
        <PatchList>
            <Line Type="bytes" Address="0x0041b2a3" Value="7542" Pattern="e8 ?? ?? ?? ?? 31 c9 80 3d ?? ?? ?? ?? 00 75 4a" At="14"/>
            <Line Type="bytes" Address="0x0041b2e7" Value="b901000000909090" Pattern="8b 00 eb 05 e8 ?? ?? ?? ?? 85 c0 0f 95 c0 0f b6 c8" At="9"/>
        </PatchList>
    </Metadata>
</Patch>
```

**`Metadata` attributes**

| Attribute | Meaning |
|---|---|
| `Name` | The patch's name, as the launcher and the log show it. A name that starts with `Resolution Patch - <width>x<height>` becomes a **Render resolution** choice |
| `Note` | Shown under the patch in the launcher. "Requires setting DMEM to N MB" and "Requires setting VBlank to N FPS" are read by the port |
| `Author`, `PatchVer` | Shown in the launcher (author) and kept for reference |
| `AppVer` | The game version the patch is for: `01.02` |
| `AppElf` | `eboot.bin` (patches for other files are ignored) |
| `isEnabled` | `true`: on by default |

**`Line` attributes**

| Attribute | Meaning |
|---|---|
| `Type` | `bytes` (hex, spaces allowed), `bytes16`, `bytes32`, `bytes64` (a number, written little-endian), `float32`, `float64`, `utf8` and `utf16` (text, a zero terminator is added) |
| `Value` | What is written. In `bytes`, `{rel:NAME:TAIL}` writes the 4-byte relative offset to the target `NAME` (see below) |
| `Pattern` | The **signature**: the original bytes around the site, in hex, with `??` for bytes that may differ between builds (relative offsets, branch targets). It must match exactly once in the `eboot.bin` |
| `At` | Where the write starts, counted in bytes from the start of `Pattern` |
| `Address` | The site in the reference build (base `0x400000`). Used only when the line has no `Pattern` |
| `Target` | Writes at a named target instead of the line's own site: data the code points to, such as a variable that has no bytes of its own to search for |

**Targets.** A `Target` names an address the patch needs, found by its own `Pattern`/`At` (or
`Line="N"`: the site of line N, counting from 0). With `Ref="1"`, the 4 bytes found there are a
relative offset, and the target is the address it points to (`Tail`: how many bytes of the
instruction come after that offset). The 240 FPS patch uses this to point an instruction at the
game's own 60.0 constant:

```xml
<Line Type="bytes" Address="0x0048ff1a" Value="488d05{rel:sixty:0}" Pattern="c5 fa 11 85 98 fb ff ff 48 8d 05 ?? ?? ?? ?? c5 fa 10 00" At="8"/>
<Target Name="sixty" Address="0x006fc0d8" Pattern="c5 fa 59 85 9c fd ff ff c5 fa 59 05 ?? ?? ?? ?? e8 ?? ?? ?? ?? c5 f9 70 c0 00" At="12" Ref="1" Tail="0"/>
```

**Signatures.** `tools/make_signatures.py` writes them from the reference build. A good pattern is
long enough to be unique and replaces with `??` every byte that changes between builds, such as the
4 bytes after an `e8` (call) or a rip-relative offset. Mask lines (`Type="mask"`, the pattern search
of some shadPS4 files) are not supported.

**Testing.** Put the file in a mod's `patches/` folder ([Patches in mods](#patches-in-mods)), start
the game and read the log: the patch must appear in the `writes from [...]` line and not in a
`skipped` line.

## Patches in mods

A mod can carry patches: XML files in its `patches/` folder, in the format above. A mod can have
only patches, with no game files.

```text
mods/
  My patch/
    mod.json
    patches/
      my-patch.xml
```

- The mod's patches apply while the mod is enabled; **Load mods** off turns them off too.
- They are added to the built-in ones in load order. A patch with the same name as a built-in one
  (or as one of a mod higher in the list) **replaces** it.
- A file whose `TitleID` list has none of the game's IDs (`CUSA01623`, `CUSA01715`) is ignored. A
  file without `TitleID` is accepted.
- `isEnabled="true"` turns the patch on with the mod. Resolution and frame rate patches are the
  exception: they always follow **Performance**. A `Resolution Patch - <width>x<height>` becomes a
  **Render resolution** choice while the mod is enabled, and applies when you choose it; see the
  [Ultrawide mod](ultrawide.md).
- The mod's details panel shows "game patches" for mods that carry them.

## Third-party patches (shadPS4 and GoldHEN)

Third-party patch files go in the `patches/` folder in the data folder (on the launcher's **Mods**
page, **Third-party patches** lists them and **Folders → Patches folder** chooses another folder).

![The Third-party patches section of the launcher's Mods page, with two sample patches](images/third-party-patches.png)

- Each patch is listed as `<file>.xml/<patch name>`. Whether it is on by default comes from the
  file's `isEnabled`; your choices are kept in `patches.json`:

  ```json
  {"enabled": ["file.xml/Patch name"], "disabled": ["file.xml/Other patch"]}
  ```

- Only patches for version `01.02` and `eboot.bin` are used; files for other games' `TitleID` are
  ignored.
- **They only apply to the reference build**, the `eboot.bin` with sha256
  `d85c8135d330c3b601bf5dc3f1dd86bd6119fb8a9fce372bed2c9785f9a79299`, because they use fixed
  addresses. With another build they are not applied; the first `Patches:` line of the log shows
  your sha256.
- Accepted line types: `bytes`, `bytes16/32/64`, `float32/64`, `utf8`, `utf16`. A patch with a `mask`
  line or an address outside the `eboot.bin` is left out whole, with a note in the log.
- They are applied after the built-in ones: where both write, the third-party one wins.
- From a terminal: `SPARTAN_PATCHES_DIR=/path SPARTAN_PATCHES_CONFIG=/path/patches.json bash run.sh`.

To use a third-party patch on any build, port it to the built-in format with signatures and ship it
in a mod ([Patches in mods](#patches-in-mods)).

## Credits

- **kvicken** and **cuesta4**: the resolution, texture fix, 120 FPS and skip video patches,
  published for shadPS4, building on the PS3 patches by **illusion0001**.
- **SpartanPort** (Felipe Prado): the port of those patches to this build and to signatures, the 100 and
  144 and 240 FPS versions, the startup black screen fix and the ultrawide, 16:10 and 4:3 sizes
  (`tools/make_ultrawide_patch.py`).
