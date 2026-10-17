# Ultrawide mod (21:9 and 32:9) — beta

**English** | [Português (Brasil)](#português-brasil)

Makes God of War III Remastered fill an ultrawide screen **without the zoomed-in picture**. The game's
camera has a fixed horizontal field of view, so a plain resolution change only crops a 16:9 picture top
and bottom. This mod adds render resolutions that also widen the field of view: the world keeps its
proportions and you see more to the sides.

| Render resolution | Aspect | Status |
|---|---|---|
| 2560x1080 | 21:9 | **Tested in game** |
| 3440x1440 | 21:9 | **Tested in game** |
| 3840x1600 | 21:9 | **Tested in game** |
| 5120x1440 | 32:9 | **Tested in game** |
| 1920x1200 | 16:10 | **Tested in game** (taller than 16:9: see Notes) |
| 2560x1600 | 16:10 | **Tested in game** (taller than 16:9: see Notes) |
| 1280x800 | 16:10 | Not tested (the Steam Deck's screen) |
| 1280x960, 1440x1080, 1920x1440, 2048x1536, 2560x1920 | 4:3 | 1280x960 tested (menu and gameplay, on a 5:4 monitor); the others are not |
| 1440x900 | 16:10 | Not tested |
| 1360x768 | 16:9 | Not tested (the shape of the original picture) |

Every size but 1280x800, 1440x900, 1360x768 and the 4:3 ones except 1280x960 was tested in game. Thanks to the players who tested them in issue #180. Since 2.4.0 these sizes are
built in (Render resolution); the mod is for older versions. If something looks wrong on your screen, please say
how it looks (resolution, and whether anything is cropped, stretched or missing at the edges) in an issue.

## How to install

**Versions that read zip mods (the easy way: one file).** That is the Ultrawide test build linked from pull
request #202 and the releases after it is merged. 2.2.0 and 2.3.0 do not: with them the zip is ignored and the
game stays at its usual resolution.

1. Put `Ultrawide.zip` into the `mods` folder, next to `God of War III.exe` (in the launcher: **Mods** →
   **Open**; create the folder if it is not there). Do not unzip it: the game does that by itself.
2. Open the launcher. The mod is listed on the Mods page, ticked, with "(game patches)".
3. In **Render resolution** (Home or Performance) pick `3440x1440 (mod)` or another size, then **PLAY**. The
   in-game menu (Insert → Performance) lists them too. The **Engine frame rate** (60, 120 or 240) is a
   separate choice that works with any resolution.
4. Use a display or window of the same size (borderless is the easy way).

**Official 2.2.0 or 2.3.0 (they cannot read zip mods yet): use the installer, `Ultrawide-installer.zip`**
(attached to pull request #202).

1. Close the game and the launcher. Unzip `Ultrawide-installer.zip` anywhere.
2. Double-click **`INSTALL.bat`**. Paste the game folder when it asks (the one with `God of War III.exe`; if you
   unzipped inside that folder it finds it by itself), then pick a resolution from the menu.
3. Start the game with **`Play God of War III.exe`**. Do not use the launcher on 2.2.0 or 2.3.0: it does not list the new
   sizes and can reset the setting. To pick another size later, run `Ultrawide - choose resolution.bat` in the
   game folder.
4. To undo everything: **`UNINSTALL.bat`** (it puts the original files back).

The installer only touches `run.py` and three files of `scripts\`, after backing them up to
`ultrawide-backup`, and refuses to run if they are not the stock 2.2.0 or 2.3.0 files (it never overwrites a newer
version). Its files are plain text: read them before you run them.

To go back to normal: untick the mod (or pick another resolution, or `UNINSTALL.bat`). Nothing of the game
is changed.

## Notes

- **Cutscenes keep their borders.** They are pre-rendered 16:9 videos, so they play with black bars at
  the sides (above and below at 16:10); stretching them would distort the picture, and they cannot be
  re-rendered. Builds before the test build left the last game frame in those bars instead of black.
- **Menus are 16:9.** The game fits its menus and HUD to a 16:9 area in the middle, so the darkened
  background of a menu does not reach the sides (the top and bottom at 16:10).
- **16:10 (taller than 16:9).** The game's camera keeps the horizontal view, so a taller picture simply shows
  more at the top and bottom, without distortion. These entries only set the resolution and the aspect ratio
  (no field of view change). 4:3: the game letterboxes its picture to 16:9 when its second aspect float is exactly 4/3, so the 4:3 entries write it one unit above.
- At 32:9 the horizontal field of view is doubled, so objects at the very edges look stretched, as in any
  game with a very wide field of view.
- It needs extra direct memory (the Video Arena grows with the pixel count); the launcher sets that from the
  patch notes. It works on game builds the patch signatures match (tested on `eboot.bin` sha256 `6e72330e…`).
- How it works and how to make another size: [docs/ultrawide.md](../../../docs/ultrawide.md).

### Known limitations (outside 16:9)

Reported by testers; they come from the game, which is built for 16:9:

- **Startup logo screens** are stretched to the whole screen.
- **Pre-rendered videos** (cutscenes, intros) keep their native 16:9 size, with black bars at the sides. The
  game picture can show around them in some builds; the test build by devLucasEmiliano clears it to black.
- **Menus** show a slightly darker box over the original 16:9 area, and on the start menu Kratos is half
  covered by the fade. That fade is drawn by the game to delimit its 16:9 screen; it is only cosmetic and
  there is no setting to remove it.
- Gameplay and HUD are fine on every size reported so far.

---

## Português (Brasil)

Faz o God of War III Remastered encher uma tela ultrawide **sem a imagem com zoom**. A câmera do jogo tem
campo de visão horizontal fixo, então só trocar a resolução corta a imagem de 16:9 em cima e embaixo. Este
mod adiciona resoluções que também abrem o campo de visão: o mundo mantém as proporções e você vê mais dos
lados.

**Como instalar.** Nas versões **que leem mods em zip** (a build de teste do Ultrawide indicada no pull request
#202 e as releases depois do merge; a 2.2.0 e a 2.3.0 não leem, e o jogo fica na resolução de sempre): coloque o
`Ultrawide.zip` (sem extrair) na pasta `mods`, ao lado do `God of War III.exe` (no launcher: **Mods** →
**Open**), abra o launcher e escolha o tamanho em **Render resolution**. Na **2.2.0 ou 2.3.0 oficiais** (que ainda não leem mods em zip): baixe o instalador
`Ultrawide-installer.zip` (anexado ao pull request #202), extraia, dê dois cliques no **`INSTALL.bat`**, cole
a pasta do jogo, escolha a resolução no menu e inicie o jogo pelo **`Play God of War III.exe`** (não pelo
launcher). Para desfazer: **`UNINSTALL.bat`**. Em **Render resolution** aparecem `3440x1440 (mod)` e os outros tamanhos (e no menu do jogo,
tecla Insert). O **Engine frame rate** (60, 120 ou 240) é uma escolha à parte e vale com qualquer resolução.

**As cutscenes continuam com bordas:** são vídeos 16:9 pré-renderizados, então tocam com barras pretas dos
lados (em cima e embaixo no 16:10); as builds anteriores à de teste deixavam o último frame do jogo nessas
barras. **Os menus são 16:9:** o fundo escurecido de um menu não chega aos lados (nem em cima e embaixo no
16:10). O 3440x1440 foi testado por quem fez; 5120x1440, 1920x1200 e 2560x1600 foram testados por outros jogadores
e funcionam; 2560x1080 e 3840x1600 ainda não. Se testar, conte como ficou numa issue do projeto.
Os tamanhos 16:10 (1920x1200 e 2560x1600) só mudam a resolução e o aspect ratio: como a câmera mantém o campo
horizontal, uma imagem mais alta mostra mais em cima e embaixo, sem deformar.

**Limitações conhecidas (fora do 16:9),** relatadas pelos testers; vêm do próprio jogo, feito para 16:9: as
telas de logo da abertura ficam esticadas; os vídeos pré-renderizados mantêm o tamanho 16:9 com barras pretas; e
os menus mostram uma caixa um pouco mais escura sobre a área 16:9 original (um "fade" que o jogo desenha para
delimitar a tela; é só estético). O gameplay e o HUD estão bons em todos os tamanhos relatados.
