# Controller prompts: an example mod and the tools that build it

The game's button prompts are DXT textures inside `EXEC/WAD/PS4_LE/R_UI.WAD` (one more copy of
the QTE circle in `R_PERM.WAD`). This folder draws Xbox or Nintendo replacements and writes
patched copies of those two WADs for the [mod loader](../../docs/MODS.md). No game files are
included: the mod is built from your own copy.

Compared with the launcher's **In-game button icons** option (`scripts/glyphs.py`), this draws
the QTE prompts as thin marks like the original cross and circle, keeps the in-text buttons
without an outline, doubles the texture resolution (with mip chains, so the glyphs stay sharp
at 1080p), and adds the pause-menu controller diagram. Either can be adopted into the other.

Needs Python 3 with Pillow, numpy and scikit-image (`pip install pillow numpy scikit-image`),
or the port's own executable: `"God of War III.exe" --script <script> <args>`.

```sh
python scan_wads.py "<game>/EXEC/WAD/PS4_LE"      # which WADs hold prompt textures (R_UI, R_PERM)

python patch_wad.py "<game>/EXEC/WAD/PS4_LE/R_UI.WAD"   art/xbox --rebuild --out "mods/Xbox Prompts/EXEC/WAD/PS4_LE/R_UI.WAD"
python patch_wad.py "<game>/EXEC/WAD/PS4_LE/R_PERM.WAD" art/xbox --rebuild --out "mods/Xbox Prompts/EXEC/WAD/PS4_LE/R_PERM.WAD"
```

Then enable **Xbox Prompts** on the launcher's Mods page, with **In-game button icons** on
*Original* (that option mounts its own copies over any mod). `art/nintendo` builds the Switch
set the same way.

| File | What it does |
| --- | --- |
| `wad.py` | Lists the chunks of a PS4_LE WAD (`0x40` header: u32 type, u32 size, name; data padded to 16). |
| `patch_wad.py` | Re-encodes each `art/<hash>.png` in the original texture's DXT format and writes it over the original pixels. With `--rebuild` the art may be larger: the chunk grows, later chunks shift, and a mip chain is added with the header of the game's own mipmapped prompts. Without it the file keeps its exact size. |
| `draw_prompts.py` | Draws the 16 prompt textures for a style (`--style xbox` or `nintendo`, `--scale 2` for double size): in-text buttons in the game's serif font, QTE letters as monoline strokes at the originals' weight and saturation, the L1/R1 and L2/R2 mash atlases. |
| `draw_pad.py` | Draws the pause-menu controller diagram from `controllers.sketch` (Xbox) or `switch_pro.svg` (Nintendo) in the original's style: 2 px red lines, a near-black body with faint smoke, stretched to the original's 913x489 box (the game narrows the quad on screen). |
| `scan_wads.py` | Reports which WADs under a folder carry a copy of a prompt texture. |
| `art/` | The generated textures, named by the hash of the texture they replace. |

Texture map (`R_UI.WAD`): 64x64 DXT5 in-text glyphs `1BBACF4D` cross, `BA40AFD2` circle,
`64F3D812` square, `573C282A` triangle, `91A47980` L1, `5CED5AE9` R1, `6FC83ACE` L2, `67E86446` R2,
`9AE3BA81` L3, `FB33513A` R3; 128x128 DXT1 QTE prompts (black is transparent) `7FF61BF7` cross,
`9D590CC8` circle (also in `R_PERM.WAD`), `1F5FC983` square, `E8889579` triangle; 256x256 DXT1 mash
atlases (top half rotated 180°) `0FFF3CCB` R2/R1, `FB91136E` L2/L1; 1024x512 DXT5 controller
diagram `CE352DC5`. The materials reference textures by this name, so a WAD loaded earlier wins
when two hold the same one.

The controller outlines were traced by Kristian Andreev; the Xbox logo is replaced by a plain
ring. Everything here is GPL-2.0-or-later like the rest of the port.
