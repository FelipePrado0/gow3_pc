# Ultrawide (21:9 and 32:9) — beta

**English** | [Português (Brasil)](#português-brasil)

The ultrawide sizes are **built in**: choose one in **Render resolution** (launcher or in-game menu); they are
marked "(beta)". Before that they were a **mod**, still available for older versions: the folder
[`extras/mods/Ultrawide`](../extras/mods/Ultrawide)
(install steps in its README). It uses the patch support of mods: a mod folder can carry game patches
(XML files with signatures, in its `patches/` folder), applied while the mod is ticked on the Mods
page. See [MODS.md](MODS.md).

The game's camera has a fixed *horizontal* field of view. Raising the render resolution to an ultrawide
size without touching the camera only crops the 16:9 picture top and bottom, so it looks zoomed in.
Each `Resolution Patch - <width>x<height>` entry fixes that: the picture fills the whole screen, the world keeps
its proportions, and the vertical view is the same as at 16:9.

| Render resolution | Aspect | Status |
|---|---|---|
| 2560 × 1080 | 21:9 | **Tested in game** |
| 3440 × 1440 | 21:9 | **Tested in game** |
| 3840 × 1600 | 21:9 | **Tested in game** |
| 5120 × 1440 | 32:9 | **Tested in game** |
| 1920 × 1200 | 16:10 | **Tested in game** (taller than 16:9: no field of view change) |
| 2560 × 1600 | 16:10 | **Tested in game** (taller than 16:9: no field of view change) |

Every size was tested in game: all six on an RX 6700 XT (Windows 11, reference build `d85c8135…`), and
3440x1440, 5120x1440, 1920x1200 and 2560x1600 also by players in issue #180 (thank you!). They stay marked
"(beta)" until more screens and GPUs confirm them. **If something looks wrong on yours, please say how it looks**
(an issue): the resolution, your GPU, and whether anything is cropped, stretched or missing at the edges.

### Known limitations (outside 16:9)

They come from the game, which is built for 16:9, and are already known; they will be fixed soon:

- **Menus**: the darkened background covers only the middle 16:9 area, so the sides stay lighter, and on the
  start menu Kratos is half covered by the fade.
- **QTE prompts and some HUD elements** keep a 16:9 layout.
- The sides can **flash black for a moment** in the transition from a camera sequence back to gameplay.
- **Startup logo screens** are stretched to the whole screen.
- **Pre-rendered videos** (cutscenes, intros) keep their native 16:9 size, with black bars at the sides.

## Using it

Choose a size in **Render resolution** (launcher, **Home** and **Performance**) or in the in-game menu (Insert or
R3+L2 → Performance → Render resolution). Each one shows its exact size and aspect ratio, for example
`3440 × 1440 · 21:9 (beta)`. Pick one and **PLAY** (in the game menu: "Apply and restart"). The **Engine frame
rate** (60, 100, 120 or 240) is a separate choice that works with any resolution.

**Older versions (before 2.4.0):** put `Ultrawide.zip` (made with `python tools/zip_mod.py extras/mods/Ultrawide`)
or the `Ultrawide` folder in the mods folder; a zip is unpacked by itself when the launcher or the game starts. The
mod's resolutions then show up in Render resolution after the built-in ones. Untick the mod to go back.

What you get:

- The world and the HUD fill the screen; characters keep their proportions. Menus are in the right proportion.
- **Cutscenes keep their borders.** They are pre-rendered 16:9 videos, so they play with black bars at
  the sides (above and below at 16:10). Stretching them to fill the screen would distort the picture, and
  they cannot be re-rendered. The game draws a cutscene in a 16:9 picture in the middle and never clears the
  rest of the screen (at 16:9 there is no rest), so the last game frame used to stay in the bars. While the
  video decoder hands out pictures, the presenter now draws everything outside that picture black
  (`video_core/renderer_vulkan/movie_bars.h`; render sizes within 1% of 16:9 are left alone).
- **Menus are 16:9.** The game fits its menus and HUD to a 16:9 area in the middle, so the darkened
  background of a menu does not reach the sides (the top and bottom at 16:10).
- At 32:9 the field of view is multiplied by 2 horizontally. It is rectilinear, so objects at the very
  edges look stretched, as in any game with a very wide field of view.

## 16:10 (taller than 16:9)

The camera keeps the horizontal view, so on a taller screen the picture simply shows more at the top and
bottom, without distortion. The 16:10 entries therefore only set the resolution and the aspect ratio
(`k = 1`: no field of view change, no caves). 4:3 is not included.

## What it changes (wider than 16:9)

1. The render resolution, as the built-in `Resolution Patch` entries do, and the Video Arena size.
2. The display aspect ratio, two floats in the video mode (`16/9` → `width/height`).
3. The tangent `t` of the field of view, multiplied by `k = (width/height) / (16/9)` (43/32 at
   3440x1440), in the four places the cameras read it. The projection is `x = 2/t`, `y = 2A/t`
   (`A` = aspect ratio), so with the new `A` and `t·k` the vertical view stays the one of a 16:9 picture and
   the horizontal one widens. The same `t` feeds the culling vectors (two builders) and the standard
   camera's `tan(fov/2)`, so nothing is culled at the sides.

There is no room for a multiplication in place. Each of the four sites becomes a `jmp` into two chained
15-byte NOP pads between functions ("caves": the first runs the replaced instruction, the second
multiplies by `k` and jumps back), and a ninth pad holds the constant `k`. Every site and pad is found
by a signature (`Pattern`/`At`) like the built-in patches; a build that does not match skips the patch.

## Other resolutions

`tools/make_ultrawide_patch.py` writes the entries of the mod (it needs the 6e72 `eboot.bin` and capstone):

```
python tools/make_ultrawide_patch.py path/to/eboot.bin 3840 1080 --mod extras/mods/Ultrawide/patches/Ultrawide.xml
```

The two numbers are the width and the height. A new size is marked "not tested" in the patch note until it
is added to `TESTED_IN_GAME` in the tool. A mod's resolutions are listed by the launcher and the game menu from
its patch file; to make a size built in, also add its entry to `patches/God_of_War_III_Remastered.xml` and its
name to the resolution lists (`scripts/graphics_options.py`, `gpu/shim/gow3_graphics.h`, `gow3_overlay.cpp`).

## Reference build addresses

The `Address` attributes the tool writes are addresses in the `6e72…` build. The signatures are what is used.
The built-in entries in `patches/God_of_War_III_Remastered.xml` were rebased to the reference build
(`d85c8135…`); to do the same for a new entry, run with that build's `eboot.bin`, once per size:

```
python tools/make_ultrawide_patch.py --rebase path/to/eboot_d85c.bin 3440 1440 --mod extras/mods/Ultrawide/patches/Ultrawide.xml
```

---

## Português (Brasil)

As resoluções ultrawide são **embutidas**: escolha em **Resolução de renderização** (launcher ou menu do jogo);
elas aparecem com "(beta)". Antes eram um **mod**, que continua disponível para versões antigas: a pasta
[`extras/mods/Ultrawide`](../extras/mods/Ultrawide) (o passo a
passo está no README dela). Ele usa o suporte a patches dos mods: uma pasta de mod pode trazer patches de
jogo (arquivos XML com assinaturas, na pasta `patches/`), aplicados enquanto o mod está marcado na página
Mods. Veja [MODS.md](MODS.md).

A câmera do jogo tem campo de visão *horizontal* fixo. Aumentar a resolução para um tamanho ultrawide sem
mexer na câmera só corta a imagem de 16:9 em cima e embaixo, e ela parece com zoom. Cada entrada
`Resolution Patch - <largura>x<altura>` corrige isso: a imagem enche a tela, o mundo mantém as proporções e a
visão vertical é a mesma do 16:9.

**Todas as resoluções foram testadas no jogo:** as seis numa RX 6700 XT (Windows 11, build de referência
`d85c8135…`), e 3440x1440, 5120x1440, 1920x1200 e 2560x1600 também por jogadores na issue #180. Elas continuam
marcadas como "(beta)" até mais telas e placas confirmarem; se algo parecer errado na sua, conte numa issue (a
resolução, a placa de vídeo e o que aparece cortado, esticado ou faltando nas bordas). Os tamanhos 16:10
(1920x1200 e 2560x1600): como a câmera mantém o campo horizontal, uma imagem mais alta só mostra mais em cima e
embaixo, então essas entradas mudam apenas a resolução e o aspect ratio.

**Como usar:** escolha um tamanho em **Resolução de renderização** (launcher, páginas **Início** e
**Desempenho**) ou no menu do jogo (Insert ou R3+L2 → Performance → Render resolution). Cada opção mostra a
resolução exata e a proporção, por exemplo `3440 × 1440 · 21:9 (beta)`. Escolha e clique em **JOGAR**. A **Taxa
de quadros do motor** (60, 100, 120 ou 240) é uma escolha à parte e vale com qualquer resolução. **Versões antigas
(antes da 2.4.0):** coloque o `Ultrawide.zip` (gerado com `python tools/zip_mod.py extras/mods/Ultrawide`) ou a
pasta `Ultrawide` na pasta de mods; o zip é extraído sozinho ao abrir o launcher ou o jogo.

**As cutscenes continuam com bordas:** são vídeos 16:9 pré-renderizados, então tocam com barras pretas dos
lados (em cima e embaixo no 16:10); esticá-los deformaria a imagem. O jogo desenha a cutscene num quadro 16:9
no meio e nunca limpa o resto da tela, então o último frame do jogo ficava nas barras; enquanto o decodificador
de vídeo entrega quadros, o presenter agora pinta de preto tudo fora desse quadro. **Os menus são 16:9:** o
fundo escurecido de um menu não chega aos lados (nem em cima e embaixo no 16:10). No 32:9 o campo de visão horizontal fica multiplicado por 2, e
objetos nas bordas extremas parecem esticados.

**Limitações conhecidas (fora do 16:9),** que vêm do próprio jogo, feito para 16:9; já são conhecidas e serão
corrigidas em breve: o fundo escurecido dos menus cobre só a área 16:9 do meio (os lados ficam mais claros); os
prompts de QTE e parte do HUD ficam no layout 16:9; as laterais podem piscar em preto por um instante na transição
de uma sequência de câmera para o gameplay; as telas de logo da abertura ficam esticadas; e os vídeos
pré-renderizados mantêm o tamanho 16:9 com barras pretas.
