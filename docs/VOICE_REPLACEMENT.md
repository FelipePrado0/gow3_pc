# Replacing the game's voices

**English** | [Português (Brasil)](#português-brasil)

How the game picks its voice language, what its voice files are, and how to swap some voice lines for
other recordings **without touching the game dump**. No game audio and no recording is part of this
repository: you bring your own files (and their license).

Everything below was read from a dump of CUSA01623 version 01.02 (US).

## How the game picks the language

The game has a text language and a separate voice (speech) language. Both come from a small list per
region, in the game folder:

- `EXEC/WAD/PS4_LE/REGION.TXT` holds the region code (`BCUS98111` in the US dump).
- `EXEC/WAD/PS4_LE/LANGS_<code>.TXT` lists the languages of that region, one per line:
  `<language id> <text code> <voice folder>`.

```
LANGS_BCUS98111.TXT (US)        LANGS_BCES00510.TXT (Europe, excerpt)
1  en ENGLISH                   2  fr FRENCH
2  fr FRENCH                    7  pt PORTUGUESE
17 br PORTUGUESE                8  ru RUSSIAN
20 la SPANISH                   16 pl POLISH
```

- The text of a language is `EXEC/WAD/PS4_LE/R_LANG_<text code>.WAD` (`R_LANG_BR.WAD` and `R_LANG_PT.WAD`
  are two different texts).
- The voice of a language is the folder named in the third column. This dump has `ENGLISH`, `FRENCH`,
  `PORTUGUESE` and `SPANISH`; voice lines are `<FOLDER>/SOUNDS/STREAMS_PS4/<NAME>.XVAG`. Lines shared by all
  languages are in `EXEC/SOUNDS/STREAMS_PS4/`.

So Portuguese (Brazil) (id 17) is Brazilian **text** with the `PORTUGUESE` **voice**, the same folder that
Portuguese (Portugal) (id 7) uses. That voice is European Portuguese (the PlayStation Store lists
"Português (Portugal)" as the only Portuguese voice), so there is no Brazilian Portuguese voice in the game.

The launcher's *Game language* list is the system language handed to the game (`GOW3_LANGUAGE`); the game also
has its own language options in its menu.

## What a voice file is

An `.XVAG` file: a header, then the audio. The header fields used here (the layout is the one documented in
[vgmstream's `xvag.c`](https://github.com/vgmstream/vgmstream/blob/master/src/meta/xvag.c)):

| Offset | Field |
|---|---|
| `0x04` | offset of the audio data |
| `0x08` | bit 0: 1 = the header is big-endian, 0 = little-endian (both occur in the same folder) |
| `0x20` | `fmat` chunk: `0x28` channels, `0x2c` codec, `0x30` samples, `0x3c` sample rate, `0x40` data size |

- **Voice lines**: codec 6 (PS-ADPCM, 16-byte frames of 28 samples), 48 kHz (a few 44.1 kHz), 1 to 8 channels.
  A line has the same name in every language; its length can differ between languages.
- **Cinematics** (`<FOLDER>/SOUNDS/STREAMS_PS4/CINEMATICS`, 17 files per language): codec 8 (MPEG), 6 channels.

## Swapping voice lines: the Update folder

Use the launcher's **Game → Update folder**. It is laid over the game when it starts; the dump is never written
(`scripts/game_update.py`). The folder needs the game's own `eboot.bin` and `sce_sys/param.sfo`, plus the files
you change at the same paths as in the game, for example
`PORTUGUESE/SOUNDS/STREAMS_PS4/KR_001_012.XVAG`.

`tools/xvag_voice_swap.py` builds that folder from your recordings:

```
python tools/xvag_voice_swap.py GAME_DIR SOURCE UPDATE_DIR --language PORTUGUESE [--reference ORIGINAL] [--dry-run]
```

- `SOURCE` is a PS3 language archive (`.psarc`, zlib) or a folder of mono PS-ADPCM recordings named
  `<NAME>.XVAG` (Sony VAG, or XVAG).
- The new file is the original byte for byte, with only the audio frames replaced and the original's flag
  byte of each frame kept, so header, size and padding stay the same. Because of that, a recording is
  accepted only if the original is mono PS-ADPCM, the sample rate is the same and the **number of frames is
  exactly the same**. Anything else is listed as refused, with the reason.
- If `SOURCE` is a modified copy of an official archive, pass the official one as `--reference`: lines the copy
  did not change are left alone.
- Not handled yet: multichannel lines and the MPEG cinematics.
- Always run it with `--dry-run` first. `UPDATE_DIR` must be a separate folder, never inside the game.

To check that the game read a file on Windows, compare its *last access time* in Explorer or PowerShell
(`Get-Item <file> | Select LastAccessTime`) before and after playing the scene. Windows may delay or disable
that timestamp, so treat a missing change as inconclusive.

### The `mods` folder does not replace these files (yet)

A loose-file mod is laid out under `dvdroot_ps4/...` and `scripts/mods.py` keeps that first folder in the
path it builds (`tests/test_mods.py` expects exactly that). This game's dump has no `dvdroot_ps4` folder (its
files sit at the root: `EXEC/`, `PORTUGUESE/`, ...), so a file such as
`dvdroot_ps4/PORTUGUESE/SOUNDS/STREAMS_PS4/X.XVAG` ends up in a new `dvdroot_ps4` folder that the game never
opens; the log says `0 game files replaced, N added`. Until that is addressed, use the Update folder.

## Legal

The recordings are somebody's work and the game's audio is Sony's. Use files you may use, for yourself; do not
publish converted game files or other people's recordings.

---

# Português (Brasil)

Como o jogo escolhe o idioma da voz, o que são os arquivos de voz e como trocar algumas falas por outras
gravações **sem tocar no dump do jogo**. Nenhum áudio do jogo nem gravação faz parte deste repositório: os
arquivos são seus (e a licença deles também).

Tudo abaixo foi lido de um dump do CUSA01623 versão 01.02 (US).

## Como o jogo escolhe o idioma

O jogo tem um idioma de texto e um idioma de voz (fala) separados. Os dois saem de uma lista pequena por região,
na pasta do jogo:

- `EXEC/WAD/PS4_LE/REGION.TXT` guarda o código da região (`BCUS98111` no dump US).
- `EXEC/WAD/PS4_LE/LANGS_<código>.TXT` lista os idiomas da região, um por linha:
  `<id do idioma> <código do texto> <pasta da voz>` (exemplo acima).

- O texto de um idioma é `EXEC/WAD/PS4_LE/R_LANG_<código do texto>.WAD` (`R_LANG_BR.WAD` e `R_LANG_PT.WAD` são
  dois textos diferentes).
- A voz de um idioma é a pasta da terceira coluna. Este dump tem `ENGLISH`, `FRENCH`, `PORTUGUESE` e `SPANISH`;
  as falas são `<PASTA>/SOUNDS/STREAMS_PS4/<NOME>.XVAG`. As falas comuns a todos os idiomas ficam em
  `EXEC/SOUNDS/STREAMS_PS4/`.

Portanto Português (Brasil) (id 17) é **texto** brasileiro com a **voz** `PORTUGUESE`, a mesma pasta que o
Português (Portugal) (id 7) usa. Essa voz é português europeu (a PlayStation Store lista "Português (Portugal)"
como a única voz em português), então não existe voz em português do Brasil no jogo.

A lista *Game language* do launcher é o idioma de sistema entregue ao jogo (`GOW3_LANGUAGE`); o jogo também tem
opções de idioma no próprio menu.

## O que é um arquivo de voz

Um arquivo `.XVAG`: um cabeçalho e depois o áudio. Os campos usados aqui (o formato é o documentado no
[`xvag.c` do vgmstream](https://github.com/vgmstream/vgmstream/blob/master/src/meta/xvag.c)) estão na tabela
acima, em inglês: deslocamento do áudio em `0x04`; bit 0 do byte `0x08` indica cabeçalho big-endian (1) ou
little-endian (0), e os dois aparecem na mesma pasta; no chunk `fmat` (em `0x20`), canais em `0x28`, codec em
`0x2c`, amostras em `0x30`, taxa em `0x3c` e tamanho dos dados em `0x40`.

- **Falas**: codec 6 (PS-ADPCM, quadros de 16 bytes com 28 amostras), 48 kHz (algumas a 44,1 kHz), de 1 a 8
  canais. Uma fala tem o mesmo nome em todos os idiomas; a duração pode variar entre idiomas.
- **Cinemáticas** (`<PASTA>/SOUNDS/STREAMS_PS4/CINEMATICS`, 17 arquivos por idioma): codec 8 (MPEG), 6 canais.

## Trocar falas: a pasta de Update

Use **Game → Update folder** no launcher. A pasta é colocada por cima do jogo ao iniciar; o dump nunca é
gravado (`scripts/game_update.py`). Ela precisa do `eboot.bin` e do `sce_sys/param.sfo` do próprio jogo, mais os
arquivos que você troca, no mesmo caminho que têm no jogo, por exemplo
`PORTUGUESE/SOUNDS/STREAMS_PS4/KR_001_012.XVAG`.

`tools/xvag_voice_swap.py` monta essa pasta a partir das suas gravações (uso acima).

- `SOURCE` é um arquivo de idioma do PS3 (`.psarc`, zlib) ou uma pasta de gravações mono PS-ADPCM com nome
  `<NOME>.XVAG` (Sony VAG ou XVAG).
- O arquivo novo é o original byte a byte, só com os quadros de áudio trocados e o byte de flag de cada quadro do
  original mantido, então cabeçalho, tamanho e preenchimento ficam iguais. Por isso uma gravação só é aceita se o
  original for PS-ADPCM mono, a taxa for a mesma e o **número de quadros for exatamente o mesmo**. O resto é listado
  como recusado, com o motivo.
- Se `SOURCE` é uma cópia modificada de um arquivo oficial, passe o oficial em `--reference`: as falas que a cópia
  não alterou ficam como estão.
- Ainda não tratados: falas multicanal e as cinemáticas em MPEG.
- Rode sempre com `--dry-run` antes. `UPDATE_DIR` deve ser uma pasta separada, nunca dentro do jogo.

Para conferir no Windows se o jogo leu um arquivo, compare o *último acesso* dele (Explorer ou
`Get-Item <arquivo> | Select LastAccessTime`) antes e depois de jogar a cena. O Windows pode atrasar ou desligar
esse horário; se não mudar, o resultado é inconclusivo.

### A pasta `mods` ainda não troca esses arquivos

Um mod de arquivos soltos usa `dvdroot_ps4/...` e o `scripts/mods.py` mantém essa primeira pasta no caminho que
monta (o `tests/test_mods.py` espera exatamente isso). O dump deste jogo não tem `dvdroot_ps4` (os arquivos ficam
na raiz: `EXEC/`, `PORTUGUESE/`, ...), então um arquivo como `dvdroot_ps4/PORTUGUESE/SOUNDS/STREAMS_PS4/X.XVAG`
cai numa pasta `dvdroot_ps4` nova que o jogo nunca abre; o log diz `0 game files replaced, N added`. Até isso ser
tratado, use a pasta de Update.

## Aviso legal

As gravações são trabalho de alguém e o áudio do jogo é da Sony. Use arquivos que você pode usar, para uso
próprio; não publique arquivos do jogo convertidos nem gravações de terceiros.
