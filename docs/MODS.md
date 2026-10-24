# File mods

**English** | [Português](MODS.pt-BR.md) | [Español](MODS.es.md)

Mods replace the game's textures, models, sounds, text and videos without changing your game
folder. At every start the launcher builds a view of the game with the mods' files on top, and
removes it when the game closes.

**Contents:** [Mods page](#the-launchers-mods-page) ·
[What you can change](#what-you-can-change-file-by-file) · [Examples](#examples) ·
[Common problems](#common-problems) · [Sharing and copyright](#sharing-mods-and-copyright) · [How to lay out a mod](#how-to-lay-out-a-mod) ·
[Where the mods are applied](#where-the-mods-are-applied) ·
[Mods with game patches](#mods-with-game-patches) ·
[Game patches](PATCHES.md)

## The launcher's Mods page

![The Mods page with three sample mods: two of them replace R_UI.WAD, the conflict list is open and the details panel shows the selected mod](images/mods-page.png)

*Three sample mods. "Kratos Gold Armor" and "Clean HUD" both replace `R_UI.WAD`: the warning
under the list names the file, and "Clean HUD" wins because it is lower in the list.*

- **Load mods**: the master switch. When it is off, no mod and no mod patch is loaded.
- **Load order**: the list of mods, with a checkbox to enable each one and arrows to reorder them.
  **The lower mod in the list wins** when two mods replace the same file. The counter shows how
  many are enabled ("3 of 5 active").
- **Conflicts**: when more than one enabled mod replaces the same file, a warning appears under
  the list ("⚠ 2 files are changed by more than one mod") and a ⚠ beside each mod involved.
  **Details** opens the list of files with the mods in load order (`EXEC/WAD/PS4_LE/R_UI.WAD:
  A > B`, B wins). Two mods are never merged inside one file: the winner goes in whole.
- **Mod details**: clicking a mod's name opens a panel with its picture, name, author, version,
  description, how many files it replaces, whether it carries game patches and its conflicts. The
  buttons open the mod's folder and its website (when the mod gives one). Clicking it again closes
  the panel.
- **Presets**: save the order and which mods are disabled. Type a name and click **Save**;
  **Load** applies a saved preset and **Delete** removes it. When a preset is loaded, mods that no
  longer exist are skipped and new mods go last, enabled. Presets are kept in `mods_presets.json`
  in the data folder.
- **Third-party patches** and **Folders** (the mods folder and the patches folder) are on the same
  page.
- **Refresh** reads the mods folder again after you copy a new mod with the launcher open.

Changes apply on the next start of the game.

## What you can change, file by file

A mod replaces **whole files**. The loader never edits inside a file: to change one texture in a
WAD, the mod ships the whole WAD with that texture changed. The table lists what the game folder
holds (counts from CUSA01623 01.02, US English) and what replacing each kind of file takes. The
formats come from the files themselves (`XVAG` and `GNF` headers, H.264 in the `.BSF` videos); what
a folder holds comes from its folder and file names, and the subtitle timing in the cinematics'
`.TXT` from the numbers they contain. Not every kind of file was replaced in game.

| Game folder | What is in it | Format | Can a mod replace it? |
|---|---|---|---|
| `EXEC/WAD/PS4_LE/` | Almost all of the game: models, textures, levels, menus, HUD and text (410 `.WAD`) | WAD archive, little-endian | **Yes.** Ship the whole edited WAD under the same name |
| `EXEC/SOUNDS/STREAMS_PS4/` | Music and sound effects (945 files) | `.XVAG` (Sony audio container) | **Yes**, as a valid `.XVAG` in the same format as the original |
| `<LANGUAGE>/SOUNDS/STREAMS_PS4/` | Voices and dialogue, one folder per language (about 1,200 files each) | `.XVAG` | **Yes**, per language: `ENGLISH/` does not change `PORTUGUESE/` |
| `EXEC/CINEMATICS/` | Pre-rendered cutscenes (48 `.BSF`) and their subtitle timing (35 `.TXT`) | `.BSF` = raw H.264 video stream; `.TXT` = line number, first frame, last frame | **Yes.** Keep the original's resolution and frame rate; a `.TXT` counts in frames of its video |
| `EXEC/INTRO/` | Logos and pictures of the opening (18 files) | `.GNF` (PS4 texture) | **Yes**, as a `.GNF` |
| `EXEC/photomode/` | Photo mode pictures (15 files) | `.gnf` | **Yes**, as a `.gnf` |
| `EXEC/ICONS/` | Save-list pictures (`ICON0.PNG`, `ICON_AUTO.PNG`, `NEWSAVE.PNG`, ...) | `.PNG` | **Yes**, as PNGs |
| `EXEC/DLC/` | Downloadable-content WADs and lists | `.WAD`, `.TXT` | **Yes**, like the other WADs |
| `eboot.bin`, `sce_module/`, `sce_sys/` | The program, its libraries and the package data | PS4 executables | **No.** Refused by the loader; code changes go through [game patches](#mods-with-game-patches) |

### Which WAD holds what

The WAD names follow a pattern. Some groups in `EXEC/WAD/PS4_LE/`:

| Files | What they hold |
|---|---|
| `R_UI.WAD` | Interface: menus, HUD, button prompts and the menus' script |
| `R_PERM.WAD` | By the name, assets kept for the whole game; it holds one more copy of the QTE button |
| `SHELL.WAD` | The main menu |
| `R_LANG_EN.WAD`, `R_LANG_PT.WAD`, `R_LANG_BR.WAD`, ... (16 files) | Menu and subtitle text, one per language |
| `R_HERO00.WAD` to `R_HERO08.WAD` | By the name, the hero (Kratos) |
| `R_W_CESTUS*`, `R_W_ELECTRIC*`, `R_W_EXPLOSIVE*`, `R_W_GODMODE*`, `R_W_SOUL*` | By the name, weapons and their upgrade levels |
| `R_ZEUS*`, `R_HADE*`, `R_POSH*`, `R_CYCL*`, `R_GRUN*`, `R_SOUL*`, ... | By the name, characters and enemies |
| `OLY*`, `HDS*`, `POS*`, `LAB*`, `CRN*`, `BAS*`, `CAV*`, `ICA*`, `GAZ*`, ... | By the name, levels and areas, in parts (`BAS100.WAD`, `BAS125.WAD`, ...) |
| `CHALLE*` | By the name, the challenge levels |

"By the name" means the group was identified from the file names only; check inside the WAD
before you rely on it.

### Inside a WAD

A WAD is a list of entries, each with a 64-byte header (tag, flags, size, a name of up to 56
characters) and its data padded to 16 bytes. Textures are entries named `TX<hash>.<hash>.dds` that
hold a short prefix followed by a plain DDS file (DXT1 or DXT5), and the materials find them by that
name. A texture mod that keeps the size and the DXT format of the original is the safest kind.

Level WADs (`OLY150.WAD` and the like, not `R_UI.WAD` or `R_PERM.WAD`) start with an `EntityCount`
entry (tag 0) whose size field holds a count of entities, not a length, and no data follows its
header. A reader that skips `size` bytes there lands in the middle of the file and finds nothing
after it: up to version 2.5.0 the port's own reader did, so it could change textures in `R_UI.WAD` and
`R_PERM.WAD` but in no level. `scripts/glyphs.py` (`entries()`) and `tools/wad_textures.py` now read
that marker as a header alone, so every texture of a level WAD can be found and changed the same
way as the interface ones.

The port changes textures this way: the button icons (Xbox, Nintendo, keyboard) are drawn into
copies of `R_UI.WAD`, `R_PERM.WAD` and `OLY150.WAD` (the Muses' lyre in the Chamber of the Flame),
keeping each texture's size and format so that only its pixels change (`scripts/glyphs.py`). In a
level texture only the 4 × 4 blocks whose pixels changed are encoded again, in every mip level, so
the rest of the texture stays byte for byte the original.

### Finding an in-world texture

Something painted on a surface in the world is usually painted into several textures at once, one
per material layer, at the same place in each: the colour (diffuse), the shine (specular, greyscale),
the light it gives off (a glow mask, often smaller), its relief (a normal map: DXT5, X in alpha and Y
in green), and sometimes a decal drawn over the surface. Change one and leave another, and the old
picture still shows through or on top. The lyre's four Greek letters are in five textures of
`OLY150.WAD`; the coloured ones that were still visible over the new letters came from the decal.

To find all of them, find the room's WAD (`--grep` with words from the room: `--grep Muse Lyre`),
then sheet only the part of every texture where the picture is, at a readable size:
`--crop 0.75,0,1,0.25` sheets the top-right quarter (left, top, right, bottom, as fractions). Every
texture that shows the same shapes there is a layer to change.

`tools/wad_textures.py` finds a texture by looking: it tiles every texture of the WADs you name into
contact sheets with numbers (`python tools/wad_textures.py "<game>\EXEC\WAD\PS4_LE" OLY1* -o sheets`),
sheets part of each texture (`--crop`), extracts the numbered ones as `.dds` (`--extract`), searches
the entry names of every WAD for words to find the WAD of a room (`--grep Muse Lyre`), and ranks
small textures that look like button prompts (`--find`). It is a developer tool kept in the repository:
run it with Python from a source checkout.

The WADs in `PS4_LE` are little-endian. The PS3 release ran on a big-endian console, so its WADs
are not expected to work here.

### What a mod cannot do

- Change game logic (damage, physics, AI, camera, timings): it lives in the executable. Use
  [game patches](#mods-with-game-patches).
- Replace `eboot.bin`, `sce_module` or `sce_sys`: refused by the loader.
- Inject a DLL or a script.
- Merge two mods inside one file: the lower one in the list replaces the file whole.
- Change a file that is not in your game: a mod made for another region (for example the
  European `R_LANG_UK.WAD`) adds a file nobody reads, and the log says `added` instead of
  `replaced`.

## Examples

**A texture or model mod.** The mod ships the edited WAD at the game's path:

```text
mods/
  Kratos Gold Armor/
    mod.json
    preview.png
    EXEC/WAD/PS4_LE/R_HERO00.WAD
```

**A voice line in Portuguese only.** The file goes in the language folder, with the original's
name:

```text
mods/
  Portuguese voices/
    PORTUGUESE/SOUNDS/STREAMS_PS4/3004CA_ALT1.XVAG
```

**A subtitle fix.** The text of every language is in its `R_LANG_*.WAD`. Replace the one of the
language you play in, and check in game that it is the file it reads: there is more than one
Portuguese file (`R_LANG_PT.WAD` and `R_LANG_BR.WAD`).

**A cutscene with new subtitles.** The video is `EXEC/CINEMATICS/<NAME>.BSF` and its subtitle
timing is `EXEC/CINEMATICS/<NAME>.TXT`; the subtitle text itself is in the `R_LANG_*.WAD`.

To find the exact path of a file, look for it in your game folder: the path inside the mod must be
the same from `EXEC/` or the language folder on (upper and lower case do not matter).

## Common problems

| What you see | Why | What to do |
|---|---|---|
| The mod is not in the list | Its folder has no `EXEC`, language folder, `dvdroot_ps4` or `patches/`, or a wrapper folder around them also holds other folders or files | Move the files so that `EXEC/` is right inside the mod folder, or leave only that one folder (and readme or pictures) in each wrapper |
| The log says `0 game files replaced` | The paths inside the mod do not match the game | Compare them with your game folder, from `EXEC/` on |
| The log says `added` instead of `replaced` | The file does not exist in your game (another region or release) | Use a mod made for your release, or rename to your game's file |
| A mod's change does not show up | Another mod lower in the list replaces the same file | Open **Details** under the list, or change the order |
| The game crashes or shows broken models after a mod | The file is from another release (PS3, another version) or was saved in a different format | Disable the mod; use files made for this release, keeping the original's format and texture sizes |
| A ZIP was not unpacked | A folder with the same name is already there, or the zip has unsafe paths | Remove or rename that folder, or extract the zip yourself; the log gives the reason |

## Sharing mods and copyright

The game's files belong to their rights holders. A mod made the way this guide describes ships
whole game files (a WAD, an XVAG, a video) with your changes inside, so sharing it also shares
the original content those files hold. Share only what you have the right to share: your own
textures, sounds and text, or files whose owner allows it. The launcher does not download or
share mods: it only layers the mods in your own mods folder over your own copy of the game.

## How to lay out a mod

The default folder is `mods/` in the data folder: on Windows, next to `SpartanPort.exe`; on
Linux (AppImage), in `~/.local/share/spartanport`. **Folders → Mods folder** chooses another one.

Each mod has its own folder that mirrors the game's tree. In this game the data is in `EXEC/` and in
one folder per language (`ENGLISH/`, `PORTUGUESE/`, ...):

```text
mods/
  Mod name/
    EXEC/WAD/PS4_LE/R_UI.WAD
    PORTUGUESE/SOUNDS/...
```

These are accepted too:

- `Mod name/dvdroot_ps4/EXEC/...` (the shadPS4 convention): `dvdroot_ps4/` is dropped and the rest
  goes to the game's root;
- `Mod name/app0/EXEC/...`;
- wrapper folders around either, such as an archive extracted into a folder of its own name:
  `Mod name/Mod name v1.2/EXEC/...`. Each wrapper may hold only that one folder, plus files that
  describe the mod (`.txt`, `.md`, `.json`, `.ini` and pictures), which are ignored.

A folder without `EXEC`, a language folder, `dvdroot_ps4` or `patches/` does not appear in the
list. A ZIP like that is unpacked, but the log says
`not a mod (no EXEC, language folder, dvdroot_ps4 or patches)`.

### Recognised folders

Language folders that are always recognised: `ENGLISH`, `FRENCH`, `GERMAN`, `ITALIAN`, `SPANISH`,
`PORTUGUESE`, `DUTCH`, `POLISH` and `RUSSIAN`.

The data folders found in the selected game's root count as well, such as `JAPANESE/` in an Asian
release. This applies to the selected game only: a folder from another installation does not count
here, and `eboot.bin`, `sce_module` and `sce_sys` are never replaced. The rule is the same for the
launcher's list and for applying the mods.

A mod made for another region (for example the European release's `R_LANG_UK.WAD`) replaces
nothing in a game that does not have that file: the log says `added` instead of `replaced`.

### Name, author, picture and website

The details panel reads an optional `mod.json` in the mod's folder, next to `EXEC/`. Every field is
optional and must be text:

```json
{
  "name": "Kratos Gold Armor",
  "author": "Your name",
  "version": "1.0",
  "description": "Gold armour for Kratos. Replaces R_HERO00.WAD.",
  "url": "https://example.com/kratos-gold-armor"
}
```

| Field | Shown as | Without it |
|---|---|---|
| `name` | The mod's name in the list and in the panel | The folder name |
| `author` | "by …" under the name | Nothing |
| `version` | "v…" next to the author | Nothing |
| `description` | The text of the panel | The first 400 characters of a `readme.txt` or `readme.md` |
| `url` | The **Website** button (it only appears when the link starts with `http://` or `https://`) | No button |

**The picture is a file, not a field.** Put `preview.png` (or `.jpg`/`.jpeg`) in the same folder.
Without it the launcher takes `cover.*`, then `screenshot.*`, then any other picture in the folder.
The panel shows it at most 268 pixels wide (4:3 at most in height); a 16:9 picture of 1280 × 720 is
plenty. A `mod.json` that cannot be read is ignored, as if it were not there.

The finished folder:

```text
mods/
  Kratos Gold Armor/
    mod.json
    preview.png
    readme.txt
    EXEC/WAD/PS4_LE/R_HERO00.WAD
```

### ZIP mods

A mod in a **ZIP** can be dropped straight into the mods folder: when the launcher opens or the
game starts, it is unpacked into a folder with the zip's name, and unpacked again when the zip
changes. A folder with the same name that you made yourself is not replaced. Unsafe paths, links
and zips larger than 16 GiB unpacked are refused. 7z and RAR archives must be extracted first.
`tools/zip_mod.py` packs a mod folder.

### File names and the log

Upper and lower case in a mod's file names do not matter: a file in another case replaces the game
file with the same name. Symlinks and junctions inside a mod are refused.

The log shows how many files each mod has (`Mods: Mod name: 12 files`), which mod won each disputed
file (`Mods: EXEC/...: A -> B`) and the total: `Mods: 12 game files replaced, 0 added`. If a mod
should replace files and the log says `0 game files replaced`, check the paths inside it.

## Where the mods are applied

The game's files are never changed. At every start the launcher builds a view of the game in a
`mod-game-*` folder and deletes it when the game exits. `/app0` stays read-only; saves stay in their
usual folder.

- The view is created in `out/` in the data folder. On Windows without the symlink privilege
  (Developer Mode off), when the data folder and the game are on different drives, it is created
  next to the game folder instead, because hard links only work on the same drive.
- Where symlinks are available, the game's folders and files are linked. Without them, on Windows,
  folders become junctions and files become hard links (a copy when the file is on another drive).
- On a network share (SMB), which refuses links and junctions, folders are mirrored: real folders
  whose files are hard-linked or copied. The game's and the mod's files stay untouched.

The `mod-game-*` folder points into your game files. If one is left behind after a crash, do not
delete it with tools that follow links (such as `rm -rf` in Git Bash or WSL): that can delete the
real game files.

A `<game folder>-mods/EXEC/...` folder next to the game folder (the shadPS4 convention) is loaded
before the mods in the list. The master switch turns it off too.

## Order and configuration

The order and the disabled mods are kept in `mods.json` in the data folder:

```json
{"order": ["First", "Second"], "disabled": ["First"]}
```

New folders are enabled and added at the end of the list, in alphabetical order.
`SPARTAN_MODS_CONFIG` changes the path of that file.

The presets of the Mods page are kept in `mods_presets.json`, one entry per name, with the same two
lists:

```json
{
  "Only voices": {"order": ["Portuguese voices", "Kratos Gold Armor"], "disabled": ["Kratos Gold Armor"]}
}
```

From a terminal:

```sh
SPARTAN_GAME_DIR=/path/CUSA01623 SPARTAN_MODS_DIR=/path/mods bash run.sh
SPARTAN_GAME_DIR=/path/CUSA01623 SPARTAN_MODS_ENABLED=0 bash run.sh
```

## Mods with game patches

A mod can also carry **code patches**: XML files in its `patches/` folder, in the format of the
built-in patches. A mod can hold only patches, with no game files:

```text
mods/
  Mod name/
    patches/
      my-patch.xml
```

They apply while the mod is enabled (**Load mods** off turns them off too), and a patch with the same
name as a built-in one replaces it. A `Resolution Patch - <width>x<height>` becomes a **Render
resolution** choice; the [Ultrawide](ultrawide.md) mod (`extras/mods/Ultrawide`) is the example.
The format, the rules and how to test a patch are in the [patches guide](PATCHES.md#patches-in-mods).

## Button icons and the controller drawing

The icon option in **Controls** prepares the prompts and the controller drawing in the pause menu
when you choose Xbox or Nintendo (or when automatic detects one of those controllers). No mod has to
be installed. The PlayStation style keeps the original files; the keyboard style keeps the original
controller drawing. The copies are made in the local cache, without changing the player's dump.

The drawings and the tool in `tools/controller_diagrams/` come from Kristian Andreev's contribution
in [PR #234](https://github.com/FelipePrado0/SpartanPort/pull/234), under GPL-2.0-or-later.
`scripts/controller_diagrams/*.png` contains only those drawings, with no game files. To regenerate
them, the tool uses Pillow, NumPy and scikit-image, with the Cinzel font shipped with the project
(OFL). Those drawing dependencies are not needed to use the PNGs in the launcher.

# Third-party patches

shadPS4 and GoldHEN patch files go in the `patches/` folder in the data folder and are listed on the
launcher's **Mods** page, under **Third-party patches**. They only apply to one build of `eboot.bin`: see the
[patches guide](PATCHES.md#third-party-patches-shadps4-and-goldhen).

Voice replacement through an Update folder: see [VOICE_REPLACEMENT.md](VOICE_REPLACEMENT.md).
