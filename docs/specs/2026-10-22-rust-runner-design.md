# Runner em Rust (sub-projeto 2 da migração Python → Rust)

Data: 2026-10-22. Estado: implementada (plano em `docs/plans/2026-10-22-rust-runner.md`); o que
foi construído está no mapa de paridade `2026-10-22-rust-runner-parity.md`.

## 1. Objetivo

Substituir `run.py` e os módulos de preparação de `scripts/` (versão 2.7.0 do SpartanPort, `main`
em `003e42b`, mesclado na `rust-launcher` em `fe318ef`) por código Rust dentro do crate
`launcher-rust/`, de modo que PLAY, o reinício in-game e o `--play` do Steam não executem Python.
O launcher Rust (sub-projeto 1, PR #959) hoje inicia `python run.py`; ao fim deste sub-projeto ele
inicia o próprio executável com `--run`.

Sub-projetos (spec do launcher, §1): 1 launcher (feito), **2 runner (esta spec)**, 3 integrações
(Steam, atualização do jogo, auto-instalação da atualização), 4 cutover do empacotamento e remoção
do Python, 5 workspace Cargo. A presença no Discord e a vista do update (`game_update`), listadas
no 3, entram aqui (§2): `run.py` chama as duas a cada PLAY.

## 2. Decisões já tomadas

| Tema | Decisão |
|---|---|
| Processo | `src/runner/` no mesmo crate. PLAY lança `<exe atual> --run` como processo filho, no lugar de `python run.py`: leitor de saída, job object, STOP e a detecção da linha `restarting through run.py` do launcher não mudam. Alternativas descartadas: runner em thread do launcher (muda log, STOP e reinício; um pânico derruba a janela) e crate separado (dois exe no pacote antes do sub-projeto 5). |
| Cobertura | Os 13 módulos mais `game_update` (a vista jogo + update que `run.py` monta quando há `SPARTAN_PATCH_DIR`), `link_libc` incluído: `out/boot-libc.bin` e `out/libc-link.json` continuam gerados embora nada os leia. Só `steam_shortcut` fica para o 3. |
| Byte a byte | Onde o probe ou o jogo leem: `boot.bin`, `boot-libc.bin`, `boot-linked.bin`, `isa-rewrite.bin`, `content.bin`, `patches.bin`, `patches-merged.xml`, `patch_status.json`, `graphics_state.json`, o `R_UI.WAD` de `out/pause_menu/` e, nos WADs de `out/glyphs/`, tudo menos os blocos DXT pintados (linha "Desenho dos glyphs"). `window_icon.bmp` só precisa do mesmo tamanho e cabeçalho (§3.2). |
| DXT | Compressão idêntica ao Pillow 12.3.0: port em Rust do encoder BC1/BC3 de `src/libImaging/BcnEncode.c` dessa release (licença HPND, atribuída em `launcher-rust/assets/`), chamado por faixa de 4 linhas como `glyphs.encode_region` chama `Image.save(..., 'DDS', pixel_format=…)`. Um encoder de crate daria blocos diferentes. |
| Desenho dos glyphs | Os pixels pintados (texto em `Cinzel-Black.ttf` e `BodoniModa.ttf`, elipses, retângulos, redução Lanczos de 6×) saem de `skrifa` (port em Rust puro do auto-hinter do FreeType: contornos e avanços com hinting) e `ab_glyph_rasterizer`, não de FreeType e `ImageDraw`: comparados ao desenho do Pillow por PSNR ≥ 40 dB por textura (medido: no mínimo 52,79 dB; as texturas só de formas saem idênticas) e, no dump real, bytes idênticos fora dos blocos pintados. O `ab_glyph` não serviu: sem hinter, errava hastes e avanços em até 2 px (17 de 74 texturas abaixo de 40 dB). Invisível para o jogador; os bytes diferem só nos blocos pintados. Igualdade total exigiria FreeType compilada e o port de `ImageDraw` e `Resample.c`: descartada. |
| ISA | Crate `iced-x86` 1.21.0, o mesmo motor e versão da binding Python (`iced-x86==1.21.0` em `README.md`): os mesmos sites de MOVBE, LZCNT, BMI1 e F16C. |
| Discord | `discord_presence.py` vem para este sub-projeto (`runner/discord.rs`): `run.py` cria `Presence(pid)` em processo e não há CLI Python para uma ponte; sem o port a presença sumiria entre o 2 e o 3. |
| Ícone | `game_icon.ico` (ícone do jogo, da Sony, criado pelo launcher antigo) deixa de ser copiado pela migração portátil (`core/settings.rs`); o ícone é só o do SpartanPort (`launcher/spartanport.ico`, `launcher/spartanport.png`). |
| Dump real | Os testes que precisam do jogo (patches do eboot real, `R_UI.WAD`, cheats nativos) rodam localmente com `SPARTAN_GAME_DIR`, como os testes Python; o CI roda só os sintéticos. Nada derivado do dump entra no repositório. |
| Mensagens | As linhas que o runner imprime continuam em inglês e idênticas às do `run.py`: o launcher as mostra no Log e deriva `exit_hint` delas. |
| Pacote | `run.py` e `scripts/` continuam no zip e no AppImage até o sub-projeto 4 (`run.sh` e o AppImage ainda os usam); o launcher deixa de chamá-los. `--script` e `--install-update` seguem encaminhados ao Python até o 3 e o 4. |
| Reinício | `SPARTAN_RESTART_COMMAND` = `<exe> --run --after <pid> <args>` no Windows (`list2cmdline`), `<exe> --run <args>` no Linux (`shlex.join`). O probe em C continua imprimindo `Runtime: restarting through run.py`; renomear a mensagem fica fora. |

## 3. Arquitetura

### 3.1 Onde vive

`launcher-rust/src/runner/`, com a regra de `src/core/`: nunca importa iced
(`tests/core_boundary.rs::core_never_mentions_iced` passa a varrer `src/runner/` também). O
runner depende de `core/` (`paths`, `ini`, `sfo`, `env`, `legacy`, `patches`, `mods`,
`save_backup`, `platform`) e de nada da UI.

### 3.2 Módulos

| Rust | Python | Conteúdo |
|---|---|---|
| `runner/mod.rs` | `run.py` | `pub fn run(args: &[OsString], env: &IndexMap<String, String>, paths: &Paths, io: &mut dyn Write) -> i32`: a ordem de §5; `wait_for_pid`, `parse_cpu_features`, `isa_rewrite_wanted`, `launch_prefix`, `find_executable`, `glyph_mounts`, `pause_menu_args`. |
| `runner/prepare.rs` | `prepare.py` | SELF/ELF em texto claro → `boot.bin` (`G3PROBE2`), `eboot.elf`, `entry.bin`, `analysis.json`; `parse_self`, `read_self`, `nid`, `inspect_libc` (checa `_init_env` = `RET`); `load_sfo`/`read_sfo` reusam `core/sfo.rs`. |
| `runner/link_libc.rs` | `link_libc.py` | eboot + `libc.prx` → `boot-libc.bin` (`G3PROBE4`), `libc-link.json`; `module`, `encode_id`. |
| `runner/link_modules.rs` | `link_modules.py` | eboot + `libc.prx` + `libSceFios2.prx` → `boot-linked.bin` (`G3PROBE5`), `link.json`; `patch_fs_loads` reescreve `64 48 8b 04 25 00000000` (`fs:[0]`) para `gs:`; nomes canônicos `NID#lib`. |
| `runner/isa_rewrite.rs` | `isa_rewrite.py` | `scan_segment` com `iced-x86`; `isa-rewrite.bin` (`G3ISA1`: cabeçalho de 64 bytes com SHA-256 e tamanho da imagem, registros de 64 bytes ordenados, espelho de `IsaSite` em `src/isa_emulate.h`); grava a tabela mesmo sem sites, como o Python, e devolve os sites. |
| `runner/content_profile.rs` | `content_profile.py` | `param.sfo` + `--sku` → `content.bin` (28 bytes `G3CONT01`: full = 3, trial = 1), `content-profile.json`. |
| `core/patches.rs` (amplia) | `patches.py` | `compile_patches` (busca por assinatura, preenchimento `{rel:}`), `merge_patch_files` → `patches-merged.xml`, `selected_patches`, `default_enabled`, `patch_requirements`, `game_profile_for`, `write_patches` → `patches.bin` (`G3PATCH2`), `patch_status.json`. XML externo só na SHA de referência, como hoje. |
| `core/mods.rs` (amplia) | `mods.py` | `build_overlay` com as quatro estratégias de `mods.build_overlay` (symlink, junction, hard link, espelho), na ordem e com os critérios do Python; junction por `FSCTL_SET_REPARSE_POINT` em `core/platform/windows.rs` (o Python usa `_winapi.CreateJunction`); overlay removido no fim; `unpack_zips` com o marcador `.from-zip`. |
| `runner/glyphs.rs` + `runner/dxt.rs` | `glyphs.py` | `entries`, `resolve`, `parse_gamepad`, `build`, `patch`, `encode_region`: cópias de `R_UI.WAD`, `R_PERM.WAD`, `OLY150.WAD` com os ícones repintados (Xbox, Nintendo, teclado) a partir de `scripts/controller_diagrams/*.png` e das fontes de `launcher/`; `dxt.rs` é o port do encoder do Pillow; `stamp.json`; devolve `{guest: caminho}`. |
| `runner/pause_menu.rs` | `pause_menu.py` | `Script` (bytecode `SMI_R_UI`: cabeçalho little-endian, valores big-endian, opcodes `16`/`01`/`09`/`13`/`43`/`49`/`4a`/`47`), `resolve_label`, `patch` (Exit to Desktop no lugar de `WadChoose`, linha Options após HLDisplay com realocação de gotos, pais, tamanhos de subárvore, cabeçalho e entrada do WAD; FailSafe chama builtin `0xfff`), `display_health_bar` (`0xffd`/`0xffe`), `native_cheats` (`0xf00`–`0xf60`, entradas `gosliderbar4`–`9`), `build` com stamp (VERSION = 14, caminho, tamanho, mtime, idioma). |
| `runner/graphics_options.rs` | `graphics_options.py` | `prepare` (ini, `graphics_state.json`, resoluções extras; reversão com a mensagem `Graphics: the last restart never reached the game…` e reescrita do ini), `environment`, `patch_names`, `restrict_to_available`, `applied_engine_fps`. |
| `runner/guest_memory.rs` | `guest_memory.py` | `pool_mb`, `physical_memory_mb` (`GlobalMemoryStatusEx` no Windows; `None` nos demais sistemas, como o Python: o aviso fala do arquivo de paginação do Windows e nunca aparece no Linux), `memory_warning`; constantes 5056 MiB, teto 16384, flex 1024 como `src/runtime_memory.c`. |
| `runner/game_update.rs` | `game_update.py` | `overlay`, `remove`, `create_view(base, update, out)` (pasta temporária `game-update-XXXX` em `out/`, links do jogo base com o update por cima, nomes sem distinção de caixa, mesmas três mensagens de erro do Python), `remove_view`; sobre `make_link`/`is_link`/`remove_link` de `core/mods.rs`. |
| `core/save_backup.rs` | `save_backup.py` | Já existe (`backup`, `restore`); o runner chama `backup`. |
| `core/legacy.rs` | `legacy.py` | Já existe (`forward_environment`, `copy_game_settings`); o runner chama os dois. |
| `runner/discord.rs` | `discord_presence.py` | §3.5. |
| `runner/window_icon.rs` | `run.py` (Pillow) | `launcher/spartanport.png` → `out/window_icon.bmp` 64 × 64 RGBA, redução Lanczos, BMP escrito à mão (cabeçalho de 40 bytes, 32 bpp, os 54 primeiros bytes iguais aos do Pillow); pixels iguais aos do Pillow não são exigidos (só a janela os mostra). |

### 3.3 Linha de comando do executável

- `--run [--after <pid>] [args]`: o runner; devolve seu exit code (§6). `args` vão ao probe.
- `--play`: o runner com as configurações salvas e sem argumentos, como `play_without_window` do
  Python 2.7.0: `attach_stdio`, log em `<pasta de saves>/last_run.log`, conselho de saída gravado no
  log e, no Windows sem console, `MessageBoxW`.
- `--version`, `--script`, `--install-update`: como hoje (os dois últimos encaminhados ao Python).
- `startup::route` ganha `Route::Run` e `Route::Play`; `Route::Python` fica para os encaminhados.

### 3.4 PLAY e STOP

`core/launch.rs::run_py_command` vira `run_command(exe: &Path, args)` = `[exe, "--run", args…]`.
`spawn_game`, o leitor de saída, o job object (Windows) e `setsid`/`killpg` (Linux) não mudam. O
filho é o próprio exe (`std::env::current_exe`), nunca um Python. `SPARTANPORT_PYTHON` deixa de
influenciar PLAY (continua valendo para os encaminhados).

### 3.5 Discord

`runner/discord.rs` reproduz `discord_presence.py`: `CLIENT_ID` `1560048868677132431`,
`SPARTAN_DISCORD_ID` o substitui, `SPARTAN_DISCORD=0` desliga. Transportes: no Windows os named
pipes `\\?\pipe\discord-ipc-0` … `9`; no Linux os sockets `discord-ipc-0` … `9` sob
`XDG_RUNTIME_DIR`, `TMPDIR`, `TMP`, `TEMP` e `/tmp`, cada um também em
`app/com.discordapp.Discord` e `snap.discord`. O pipe do Windows é aberto `PIPE_NOWAIT` e consultado
com `PeekNamedPipe`, como no Python, sem thread de leitura (uma leitura pendente num pipe síncrono
trava a escrita e o fechamento). Quadros `<u32 opcode><u32 tamanho>` + JSON; opcodes
HANDSHAKE 0, FRAME 1, CLOSE 2, PING 3, PONG 4. Sequência: HANDSHAKE `{v: 1, client_id}` → FRAME
`evt: READY`; SET_ACTIVITY com o `pid` do probe e o mesmo payload de `activity(started)`; leitura
com 5 s de limite; nova tentativa a cada 15 s; ao fechar, limpa a atividade, envia CLOSE e espera
3 s pela thread. Sem cliente Discord: silêncio.

### 3.6 Dependências novas

`iced-x86 = "=1.21.0"` (features `std`, `decoder` e `instr_info`: o fluxo de controle e os
registradores), `skrifa` (sem `autohint_shaping`, como o FreeType padrão, sem HarfBuzz) e
`ab_glyph_rasterizer` para o texto dos glyphs, `image` só com `png` (o BMP do ícone é escrito à mão:
o encoder do crate grava um cabeçalho V4 em vez dos 54 bytes do Pillow), `sha2` e `roxmltree` já
presentes, `windows-sys` com `Win32_System_Ioctl`, `Win32_System_IO` e `Win32_Storage_FileSystem`
(junction), `Win32_System_Pipes` (Discord) e `Win32_System_SystemInformation` (memória física),
`nix` já presente (`setsid`, `kill`). Nenhum crate de DXT e nenhuma FreeType compilada.

### 3.7 Formatos e constantes partilhados com o runtime C/C++

| O quê | Onde o C lê | Onde o Rust escreve |
|---|---|---|
| `G3PROBE5` | `probe.c:996-1030` | `runner/link_modules.rs` |
| `G3PATCH2` (base `0x400000`, no máximo 4096 bytes por escrita, slots de ponteiro rebaseados, dados sobrando rejeitados) | `probe.c:726` | `core/patches.rs::write_patches` |
| `G3CONT01` (28 bytes) | `probe.c:941` | `runner/content_profile.rs` |
| `G3ISA1` (`IsaSite` 64 bytes, SHA-256 e tamanho da imagem, registros ordenados) | `src/isa_emulate.h:15`, `src/isa_rewrite.c:104-125` | `runner/isa_rewrite.rs` |
| Builtins `0xfff`, `0xffd`, `0xffe`, `0xf00..0xf60`; hook em `0x1d5f0f` | `src/menu_hook.h:8-14`, `probe.c:698` | `runner/pause_menu.rs` |
| DMEM 5056 MiB, teto 16384, flex 1024 | `src/runtime_memory.c:26-38,84` | `core/patches.rs`, `runner/guest_memory.rs` |
| Chaves do ini `render_resolution`, `engine_fps`, `*_readback`, `async_shaders`, `restart_unconfirmed`; `SPARTAN_ENGINE_FPS` | `gpu/shim/spartanport_settings.cpp:116-137,204,223` | `runner/graphics_options.rs` |
| Linhas do probe `CPU: missing …`, `Gamepad: type=… south=… name=…` | `probe.c:925`, `src/runtime_pad.c:845` | `runner/mod.rs`, `runner/glyphs.rs` |
| No máximo 8 `--mount` | `probe.c:899-918` | `runner/mod.rs` |

## 4. Interface

Nenhuma janela nova. O que muda para quem vê:

- No Log do launcher, as mesmas linhas de hoje (`prepare`, `Mods:`, `Graphics:`, `CPU:`, as do
  probe), sem as linhas de arranque do Python.
- `Add to Steam` grava o atalho com `<exe> --play` (já é o exe hoje; o `--play` passa a ser
  nativo).
- Com `discord_rpc` ligado, a presença aparece como hoje.

## 5. Fluxo de dados

`--run` segue a ordem de `run.py` (`run.py:162`):

1. `--after <pid>`: espera até 30 s o processo antigo sair (`OpenProcess` + `WaitForSingleObject`;
   no Linux `kill(pid, 0)` a cada 100 ms).
2. Diretório atual = pasta do port. `legacy::forward_environment` (`GOW3_*` → `SPARTAN_*`) e
   `legacy::copy_game_settings` (`gow3.ini` → `spartanport.ini`).
3. `data` = `SPARTAN_DATA_DIR` ou a pasta do port; cria `data/out`; `SPARTAN_CONFIG` padrão
   `data/spartanport.ini`; remove do ambiente cada chave listada em `SPARTAN_DERIVED`.
4. `mods::enabled_patch_files` → `patches::resolution_names` → `SPARTAN_EXTRA_RESOLUTIONS`.
5. `graphics_options::prepare(ini, out/graphics_state.json, extras)`; o resultado entra no
   ambiente; `SPARTAN_FSR411_DIR` padrão `data/fsr4_411`.
6. Probe: `SPARTAN_PROBE`, senão `bin/spartanport-probe[.exe]`, senão `out/…`; ausente → §6. No
   Windows, sem `SDL3.dll` ao lado do probe, `MSYS2_ROOT\clang64\bin` (padrão `C:\msys64`) entra
   no `PATH`.
7. `probe --cpu-features`: código 20 → repassa a saída e sai com 20; senão lê `CPU: missing …` e
   decide `isa_rewrite_wanted` (`SPARTAN_ISA_REWRITE` e a política de `probe.c:927`).
8. `<jogo>/eboot.bin` existe, senão `No eboot.bin in …`. Com `SPARTAN_PATCH_DIR`,
   `game_update::create_view` monta a vista em `out/game-update-XXXX` e imprime
   `Game and update combined from <pasta>`; a vista passa a ser o jogo e é removida no fim.
9. Em processo, o que eram subprocessos Python: `mods::build_overlay` (caminho do jogo ou do
   overlay), `prepare`, `link_libc`, `link_modules`, `isa_rewrite` (só se pedido; falha não fatal),
   `content_profile` com `SPARTAN_CONTENT_SKU`.
10. `window_icon::write` → `SPARTAN_WINDOW_ICON`; `SPARTAN_UPSCALER=none` salvo
    `SPARTAN_EXPERIMENTAL_UPSCALER=1`; `SPARTAN_READBACK_LINEAR=1` (todos `setdefault`).
11. `merge_patch_files` → `out/patches-merged.xml`; seleção = `selected_patches` +
    `default_enabled` + `graphics_options::patch_names` + `restrict_to_available`;
    `SPARTAN_PATCHES`, `SPARTAN_PATCHES_ONLY=1`.
12. Apaga `patch_status.json`, `write_patches` → `patches.bin`, relê o status: `SPARTAN_ENGINE_FPS`
    (`applied_engine_fps`, padrão 60), `SPARTAN_DMEM_MB`, `SPARTAN_VBLANK_HZ` (padrão 60).
13. `glyph_mounts`: `probe --gamepad` com 15 s de limite quando `SPARTAN_GLYPH_STYLE` é `auto`;
    `glyphs::build` → montagens; `pause_menu::build` sobre o `R_UI.WAD` resultante (ou o original)
    → `--pause-menu`. Falhas dos dois só no log. Se o probe `--gamepad` expira ou falha, não há
    montagens e o jogo fica com os próprios ícones, como no `run.py`.
14. `SPARTAN_DERIVED` com as chaves derivadas; aviso de `guest_memory` no log; cria `user_dir`;
    `SPARTAN_GPU_USER_DIR` padrão; `save_backup::backup` (falha só no log).
15. `SPARTAN_RESTART_COMMAND` (§2).
16. Probe: `[gamemoderun] probe out/boot-linked.bin --content-profile out/content.bin --patches
    out/patches.bin --app0 <jogo|overlay> --user <user_dir> --timeout ${SPARTAN_TIMEOUT:-0}
    [--isa-rewrite out/isa-rewrite.bin] [--mount /app0/EXEC/WAD/PS4_LE/<WAD>=out/glyphs/<WAD>]…
    [--pause-menu out/pause_menu/R_UI.WAD] <args>`; stdin nulo; `CREATE_NO_WINDOW` sem console;
    `MANGOHUD` e `SDL_APP_ID=spartanport` (Linux) como hoje. `discord::Presence::new(pid)`.
    Espera o fim; `finally`: fecha a presença, remove o overlay e a vista do update.

Variáveis lidas: `SPARTAN_DATA_DIR`, `_CONFIG`, `_DERIVED`, `_MODS_DIR`, `_MODS_CONFIG`,
`_MODS_ENABLED`, `_GAME_DIR` (padrão `../CUSA01623`), `_FSR411_DIR`, `_PROBE`, `_ISA_REWRITE`,
`_PATCH_DIR`, `_CONTENT_SKU`, `_EXPERIMENTAL_UPSCALER`, `_PATCHES`, `_PATCHES_ONLY`, `_PATCHES_DIR`,
`_PATCHES_CONFIG`, `_DMEM_MB`, `_VBLANK_HZ`, `_GLYPH_STYLE`, `_PAD_MAP`, `_KEY_MAP`, `_LANGUAGE`,
`_STREAM_MEMORY`, `_USER_DIR`, `_TIMEOUT`, `_MANGOHUD`, `_GAMEMODE`, `_DISCORD`, `_DISCORD_ID`,
`MSYS2_ROOT`, `PATH`, `GOW3_*`.

Variáveis gravadas (com `setdefault` salvo onde o Python sobrescreve): `SPARTAN_EXTRA_RESOLUTIONS`,
`_DEFERRED_READBACK`, `_STALE_READBACK`, `_ASYNC_SHADERS`, `_ENGINE_FPS`, `_GLYPHS`, `_DERIVED`,
`_RESTART_COMMAND`, `_UPSCALER`, `_READBACK_LINEAR`, `_WINDOW_ICON`, `_GPU_USER_DIR`, `_PATCHES`,
`_PATCHES_ONLY`, `_DMEM_MB`, `_VBLANK_HZ`, `_FSR411_DIR`, `MANGOHUD`, `SDL_APP_ID`.

Arquivos: `data/out/{boot.bin, eboot.elf, entry.bin, analysis.json, boot-libc.bin, libc-link.json,
boot-linked.bin, link.json, isa-rewrite.bin, content.bin, content-profile.json, patches-merged.xml,
patches.bin, patch_status.json, graphics_state.json, window_icon.bmp, glyphs/{*.WAD, stamp.json},
pause_menu/{R_UI.WAD, stamp.json}}`; overlay temporário `mod-game-XXXX/` (ou ao lado do jogo
noutro volume), removido ao fim; `data/spartanport.ini` só na reversão; `data/user/`;
`data/user/savedata_backups/<carimbo>/`; `data/mods/<zip>/` com `.from-zip`. Nenhum log próprio.

## 6. Erros

| Situação | Mensagem (inglês, igual ao Python) | Código |
|---|---|---|
| Probe ausente | `spartanport-probe[.exe] not found: build it with build.sh or use a packaged build.` | 1 |
| Sem eboot | `No eboot.bin in <jogo>` | 1 |
| Preparação | `prepare failed: …`, `Mods: …` | 1 |
| CPU sem instruções | a saída do probe (`STOP: …`) | 20 |
| Ctrl+C | — | 130 |
| Jogo fechou | — | o código do probe |

Não fatais, só no log: `isa_rewrite`, glyphs, pause menu, backup de saves, ícone da janela,
Discord. O launcher continua lendo `exit_hint` das linhas do probe; o `--play` grava o conselho
no `last_run.log` e mostra `MessageBoxW` no Windows sem console.

## 7. Testes

TDD: teste falhando antes do código. Os testes Python do runner são a lista de comportamentos;
`docs/specs/2026-10-22-rust-runner-parity.md` marca cada caso como portado, substituído ou
descartado, com o motivo, contra a referência 2.7.0 (`003e42b`), e
`tests/test_runner_parity_doc.py` (o checador do launcher, `tests/parity_doc.py`, parametrizado pelos
arquivos) mantém o mapa honesto. Arquivos cobertos e seus métodos de teste: `test_prepare.py` (13), `test_link_libc.py` (9),
`test_link_modules.py` (2), `test_isa_rewrite.py` (21), `test_run_isa.py` (4), `test_patches.py`
(30), `test_mods.py` (43), `test_mod_conflicts.py` (5), `test_glyphs.py` (37), `test_content.py`
(5), `test_guest_memory.py` (8), `test_graphics_options.py` (28), `test_save_backup.py` (10),
`test_pause_menu.py` (19), `test_native_cheats.py` (9), `test_legacy.py` (34),
`test_discord_presence.py` (26), `test_game_update.py` (6): 309 métodos e 308 linhas, pois
`test_graphics_options.py` define um nome duas vezes. Resultado final do mapa: 276 portados,
15 substituídos, 4 descartados (`ContentTests` testa o runtime C; `test_main_without_iced_x86` é só do
Python), nenhum descartado pela spec e 13 lacunas, listadas no mapa (a maioria, atalhos do Windows
que precisam do PowerShell). `test_runtime.py` testa o runtime C, não o runner: fica fora.

### 7.1 Unitários (`cargo test`, sem jogo)

Um teste Rust por caso Python, com as mesmas entradas e expectativas: SELFs sintéticos de
`test_prepare`, pacotes sintéticos de `test_probe.package`, `tests/data/isa_fixture.bin`,
`patches/CUSA01623.xml`, ini e estados de `test_graphics_options`, WADs e DDS sintéticos de
`test_glyphs`, scripts `SMI_R_UI` sintéticos de `test_pause_menu`.

### 7.2 Paridade de arquivos

`tools/runner_fixtures.py` roda os módulos Python 2.7.0 sobre entradas sintéticas e grava
`launcher-rust/fixtures/runner/<caso>/`, comparados byte a byte pelo Rust (LF forçado em
`.gitattributes` para os JSON e XML):

- `prepare`: SELF sintético → `boot.bin`, `eboot.elf`, `entry.bin`, `analysis.json`.
- `link`: eboot, `libc.prx` e `libSceFios2.prx` sintéticos → `boot-libc.bin`, `libc-link.json`,
  `boot-linked.bin`, `link.json`.
- `isa`: `tests/data/isa_fixture.bin` → `isa-rewrite.bin`.
- `content`: `param.sfo` sintético, SKUs full e trial → `content.bin`, `content-profile.json`.
- `patches`: ELF sintético com assinaturas plantadas e XML pequeno → `patches-merged.xml`,
  `patches.bin`, `patch_status.json`.
- `graphics`: ini e `graphics_state.json` de entrada → `graphics_state.json`, ini revertido,
  ambiente.
- `dxt`: PNGs sintéticos (gradientes, bordas, alfa) → blocos DXT1 e DXT5 do Pillow por faixa de
  4 linhas; o teste mais importante do sub-projeto.
- `icon`: `launcher/spartanport.png` → `window_icon.bmp` (comparado por tamanho e cabeçalho).

### 7.3 Dump real

Com `SPARTAN_GAME_DIR` apontando para o jogo (como `test_pause_menu.py` e `test_native_cheats.py`
fazem): `patches.bin` do eboot real igual ao do Python, `R_UI.WAD` do menu igual ao do Python,
`boot-linked.bin` real igual, glyphs sobre os WADs reais iguais. Sem dump, os testes pulam.

### 7.4 Processo real

`tests/runner.rs`: `--run` com um probe falso (`src/bin/fake-probe.rs`, como o `fake-game`):
exit code propagado (o caso 7 de `test_mods`), overlay removido, `--after` esperando um processo
que termina, reinício simulado (o probe falso imprime `Runtime: restarting through run.py` e
executa `SPARTAN_RESTART_COMMAND`), `--play` gravando `last_run.log`. `tests/ui_play.rs` continua
cobrindo PLAY e STOP com o `fake-game`.

### 7.5 Fronteira e CI

`core_never_mentions_iced` cobre `src/runner/`. O workflow `rust-launcher.yml` não muda:
fmt, clippy, `cargo test`, build release nos dois sistemas.

## 8. Escopo

**Entra:** `src/runner/` com os 13 módulos, a vista do update e o Discord; `--run`, `--after`, `--play` nativos;
PLAY pelo exe; remoção do `game_icon.ico` do Rust; `tools/runner_fixtures.py` e fixtures;
`-parity.md` do runner com checador; `fake-probe`; port do encoder DXT; junction em
`core/platform/windows.rs`.

**Fica fora:** Steam, atualização do jogo e auto-instalação (3); `run.sh`, AppImage, `package.sh`,
remoção de `run.py`/`scripts/`/Python do pacote, PyInstaller (4); workspace (5); a mensagem
`restarting through run.py` do probe em C; macOS.

## 9. Riscos

| Risco | Mitigação |
|---|---|
| Encoder DXT não idêntico | Port linha a linha de `BcnEncode.c` 12.3.0; fixtures do Pillow por bloco antes de qualquer glyph; qualquer byte diferente é falha. |
| `iced-x86` em versão diferente da binding | `=1.21.0` cravado no `Cargo.toml`; teste compara os sites do `isa_fixture.bin` com o `isa-rewrite.bin` do Python. |
| Junction sem privilégio ou volume diferente | As mesmas quatro estratégias e a mesma ordem do Python; o teste de overlay roda cada uma com a anterior forçada a falhar. |
| `--after` e job object no reinício | `fake-probe` que reinicia; o launcher mantém o filho novo no job como hoje (`a_restart_keeps_the_running_game`). |
| Reescrita do bytecode do menu | Scripts sintéticos nos unitários; o `R_UI.WAD` real no 7.3 antes da release. |
| Tamanho (~5.200 linhas de Python) | Plano em tarefas por módulo, cada uma com seus testes e fixture; PLAY troca para `--run` só na última tarefa, quando a cadeia inteira passa no dump real. |

## 10. Referências

- `run.py`, `scripts/*.py` em `003e42b`; mapa dos módulos, formatos e testes levantado em
  2026-10-22 (conversa).
- `docs/specs/2026-10-21-rust-launcher-design.md` (sub-projeto 1) e
  `docs/specs/2026-10-21-rust-launcher-parity.md`.
- Pillow 12.3.0, `src/libImaging/BcnEncode.c` (HPND); `iced-x86` 1.21.0.
- `packaging/windows/README.md` (reinício, `--run`, `--after`).
