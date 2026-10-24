# Cutover para o launcher Rust (sub-projetos 3 e 4 da migração Python → Rust)

Data: 2026-10-23. Estado: design aprovado em conversa; aguarda revisão do texto.

## 1. Objetivo

Fazer o pacote do SpartanPort (zip do Windows e AppImage do Linux) levar o launcher Rust
(`launcher-rust/`) no lugar do launcher Python congelado pelo PyInstaller, sem Python em nenhum
caminho do produto. Depois deste trabalho, o jogador recebe um único executável Rust que abre a
janela, joga (`--run`, `--play`), adiciona o jogo ao Steam, instala atualizações e troca vozes.

Sub-projetos (spec do launcher, §1): 1 launcher (feito), 2 runner (feito, PR #959), **3
integrações e 4 cutover do empacotamento (esta spec, juntos por decisão de 2026-10-23)**,
5 workspace Cargo. A presença no Discord e a vista do update (`game_update`), listadas no 3, já
entraram no 2.

O que ainda chama Python no exe Rust hoje (`8db864e`):

1. Add to Steam: `python -c` sobre `scripts/steam_shortcut.py` (`src/ui/pages/advanced.rs`).
2. `--install-update TARGET PID`: `Route::Python` encaminha ao `install_update` do Python.
3. `--script`: encaminhado ao Python; só o `run.py` congelado e o `smoke-test.ps1` o usam.

## 2. Decisões já tomadas

| Tema | Decisão |
|---|---|
| Escopo | Os sub-projetos 3 e 4 viram um só: o pacote passa a levar o launcher Rust. |
| Onde | Direto na branch `rust-launcher`, no PR #959. |
| Steam | Port de `steam_shortcut.py` em `core/steam.rs` e `core/steam_art.rs`. `src/runner/draw.rs` passa para `src/core/draw.rs` (`git mv`), porque `core/` não importa `runner/`. |
| Arte do Steam | Mesmo algoritmo e constantes do Python sobre `core/draw.rs`. Os testes checam estrutura (tamanhos, modos, os cinco arquivos, logo com e sem emblema), como os testes Python. Não há comparação de pixels: o Steam não lê os bytes. |
| Instalação do update | `--install-update TARGET PID` em Rust, com o contrato do Python. Copia o pacote novo por cima sem os `USER_FILES` e nunca apaga nada da pasta instalada. |
| Troca de vozes | `tools/xvag_voice_swap.py` vira o subcomando `--voice-swap` do exe. |
| Ponte Python | Somem `Route::Python`, `--script`, `python_command` e `forward_to_python`. |
| Python no repositório | `launcher/*.py`, `run.py`, `scripts/*.py`, `tools/xvag_voice_swap.py` e seus testes ficam congelados no repositório. Nenhum entra em pacote e nada os executa. |
| Nix | Saem `packaging/appimage.sh`, `packaging/default.nix`, `packaging/runtime-paths.nix` e `launcher/spartanport_vulkan.py`. O AppImage oficial é o de `packaging/linux/build.sh`. |
| Versão | O primeiro pacote Rust é o 2.8.0 (`Cargo.toml` e `VERSION` em `src/lib.rs`). |

## 3. Arquitetura

### 3.1 Módulos

| Rust | Python (congelado) | Conteúdo |
|---|---|---|
| `core/draw.rs` (movido de `runner/`) | — | As operações do Pillow já portadas. Ganha `FontSize::length(text)` (o `ImageDraw.textlength` do Pillow: soma dos avanços em 26.6 / 64). `runner/glyphs.rs` e os demais usuários passam a importar `crate::core::draw`. |
| `core/steam.rs` | `scripts/steam_shortcut.py` | VDF binário, app id, pastas do Steam, contas, Steam aberto, `add_to_steam`. |
| `core/steam_art.rs` | `scripts/steam_shortcut.py` | `make_art`, `cover`, `fit_face`, `spaced`, `usable_emblem`, `make_logo`, `darken_bottom`. |
| `core/update.rs` (amplia) | `install_update` do launcher | `USER_FILES`, `install_over`. |
| `core/psarc.rs` | classe `Psarc` | Leitor de PSARC só leitura. |
| `core/voice_swap.rs` | `tools/xvag_voice_swap.py` | XVAG, VAGp, `swap_voice`, `run`, a linha de comando. |
| `core/legacy.rs` (amplia) | — | `remove_python_leftovers` (§3.6). |
| `startup.rs` | `main` do launcher | Rotas novas `InstallUpdate` e `VoiceSwap`; sai `Python`. |

### 3.2 Linha de comando do executável

`--run`, `--install-update` e `--voice-swap` valem como primeiro argumento: o que vem depois é
deles. `--version` e `--play` valem em qualquer posição, como hoje.

- `--version`, `--run [args]` e `--play`: como hoje.
- `--install-update TARGET PID`: no Windows, com exatamente esses dois argumentos, instala (§3.4).
  Com outro número de argumentos, ou fora do Windows, abre a janela sem a migração do nome
  antigo, como o `main` do Python.
- `--voice-swap GAME SOURCE UPDATE [--language L] [--only FILE] [--reference R] [--dry-run]`
  e `--voice-swap --help` (§3.5).
- `--script` deixa de existir: vira um argumento qualquer e a janela abre.

### 3.3 Steam

`add_to_steam(game, command, icon, steam_root: Option<&Path>, running: Option<bool>) ->
Result<Added, SteamError>`. `command` é `(exe, options, start_dir)`. `Added` tem `accounts`,
`updated` e `app_id`. `SteamError` tem as variantes `Running`, `NoSteam`, `NoArt(PathBuf)` e
`Io(io::Error)`. Os dois `Option` são as costuras de teste do Python (`steam_root=`, `running=`).

Igual ao Python:

- **VDF binário:** `0` mapa (chave, entradas, `8`), `1` string, `2` u32 little-endian, `8` fim do
  mapa. `Value { Map(IndexMap<String, Value>), Str(String), U32(u32) }` guarda a ordem como o
  `dict`. Strings decodificadas em UTF-8 com substituição. Tipo desconhecido é erro com a
  posição (`unknown VDF type {kind} at {pos}`).
- **App id:** `crc32(exe + "SpartanPort") | 0x80000000`, com `exe` já entre aspas.
- **Steam aberto:** `tasklist /FI "IMAGENAME eq steam.exe" /NH` sem janela, e `steam.exe` na saída
  em minúsculas. No Linux, `pgrep -x steam` com código 0.
- **Raízes:** no Windows, `SteamPath` de `HKCU\Software\Valve\Steam` (`RegGetValueW`), depois
  `C:/Program Files (x86)/Steam` e `C:/Program Files/Steam`. No Linux, `~/.local/share/Steam`,
  `~/.steam/steam` e `~/.var/app/com.valvesoftware.Steam/.local/share/Steam`.
- **Contas:** pastas de `userdata/` cujo nome só tem dígitos e não é `0`, em ordem de nome, sem
  repetir o mesmo caminho canônico.
- **Entrada:** os 17 campos de `shortcut_entry` na mesma ordem e com os mesmos valores. `Exe` e
  `StartDir` vão entre aspas, e `icon` aponta para `<grid>/<id>_icon.png`.
- **Slot:** o primeiro cujo `appid` é o calculado ou cujo `AppName` é `SpartanPort` ou
  `God of War III` (`legacy::LEGACY_TITLE`). Achado, conta como `updated` e mantém `tags` e
  `LastPlayTime`. Sem slot, usa o maior nome numérico + 1 (ou `0`).
- **Arquivo:** sem `shortcuts.vdf`, ou com ele vazio, começa de `{shortcuts: {}}`. Um arquivo
  existente é copiado para `shortcuts.vdf.spartanport-backup` antes de ser regravado. Cada conta
  é gravada por vez, como no Python.

**Arte** (`make_art`, por conta, em `config/grid/`). Fundo: `sce_sys/pic1.png`, senão
`sce_sys/pic0.png`, senão `NoArt(<jogo>/sce_sys)`. Imagem do ícone: o `icon` pedido, senão
`sce_sys/icon0.png`. Ela é o emblema quando tem largura ≥ 200 e alfa < 40 no pixel (1, 1).
Arquivos:

| Arquivo | Tamanho | Conteúdo |
|---|---|---|
| `<id>_hero.png` | 1920 × 620, RGB | fundo em `cover` com foco (0,5; 0,52) |
| `<id>_logo.png` | 1000 de largura, RGBA | `make_logo(emblema, 1000)` |
| `<id>.png` | 920 × 430, RGB | fundo em `cover` mais o logo de 380 a 48 px da esquerda, centrado na altura |
| `<id>p.png` | 600 × 900, RGB | `darken_bottom(cover(foco 0,9; 0,5))` mais o logo de 500, centrado, 50 px acima da base |
| `<id>_icon.png` | 256 × 256 | a imagem do ícone em Lanczos |
| `<id>.json` | — | o texto exato `{"nVersion": 1, "logoPosition": {"pinnedPosition": "BottomLeft", "nWidthPct": 42, "nHeightPct": 56}}` |

`make_logo` segue o Python. Desenha em escala 2×, com o emblema a 30 % da largura e
`SPARTANPORT` em dourado (233, 214, 164). O texto vem de `fit_face` (três iterações para 94 % da
largura, espaço entre letras de 4 % do tamanho). Sombra preta (0, 0, 0, 220) deslocada 3 × 2 px,
blur gaussiano de raio 4, e redução Lanczos para a largura pedida.

Fontes, a primeira que existir: `C:/Windows/Fonts/georgiab.ttf`, `C:/Windows/Fonts/timesbd.ttf`,
os três caminhos de DejaVu Serif Bold e os dois de Noto Serif Bold do Python, e por fim
`<port>/launcher/Cinzel-Black.ttf`. O Rust não tem o `ImageFont.load_default` do Pillow; a
Cinzel vai em todo pacote.

**UI** (`advanced.rs`): `add_to_steam` chama `core::steam::add_to_steam` numa thread, com
`(launcher_path(), "--play", <pasta de launcher_path()>)` e o ícone
`<port>/launcher/spartanport.png`. Os três textos traduzidos de hoje continuam (Steam aberto,
Steam ausente, sem `pic1.png`), e `Io` mostra o erro em até 400 caracteres. Somem
`steam_bridge_command`, `python_literal`, `accounts_of` e `steam_outcome` (a leitura do stdout
do Python).

Correção: o `StartDir` passa a ser a pasta do programa, como `steam_launch_command` do Python.
A ponte de hoje passa `port_dir`, que dentro do AppImage é o mount temporário `/tmp/.mount_*`.

### 3.4 Instalação do update (Windows)

Contrato com o Python, nos dois sentidos. O launcher Python 2.6/2.7 baixa o primeiro pacote
Rust e executa `<download>\SpartanPort.exe --install-update <pasta instalada> <pid>`. O launcher
Rust faz o mesmo com qualquer pacote (`src/ui/update.rs::install_windows`, sem mudança).

`core/update.rs`:

- `USER_FILES`: `user`, `out`, `mods`, `spartanport.ini`, `settings.json`, `game_icon.ico`,
  `mods.json`, `mods_presets.json`, `patches.json` e `last_run.log`, na ordem do Python.
- `install_over(source, target, attempts, pause) -> io::Result<()>` é o `shutil.copytree(source,
  target, dirs_exist_ok=True, ignore=ignore_patterns(*USER_FILES))` do Python:
  - Uma entrada cujo nome está em `USER_FILES` é pulada em qualquer nível. No Windows a comparação
    ignora maiúsculas, como o `fnmatch` faz lá.
  - Pastas são criadas e arquivos sobrescritos; nada da pasta instalada é apagado.
  - Se a cópia falhar, ela recomeça inteira depois de `pause`, até `attempts` vezes.

`startup::install_update(target, pid)`, chamado por `Route::InstallUpdate`:

1. Espera o processo `pid` por até 60 s (`platform::wait_for_process`).
2. Chama `install_over(<pasta deste exe>, target, 30, 1 s)`.
3. Se der certo, inicia `target\SpartanPort.exe` com `target` como pasta atual. Código 0.
4. Se falhar, mostra um diálogo de erro (`rfd`, título `SpartanPort`) com "Could not install the
   update. Download it from the releases page." no idioma das configurações, senão no do
   Windows. Depois abre `RELEASES_PAGE`. Código 1.

A rota não roda `startup::prepare`: nem a migração do nome antigo nem a limpeza de
`spartanport-update`, como no Python (`test_the_update_installer_does_not_migrate`).

O menu de versões só lista releases carimbadas (`release-windows.json`). Por isso nenhuma
release Python aparece nele, e o caminho Rust → Python não acontece pelo launcher.

### 3.5 Troca de vozes

`--voice-swap` é o port de `tools/xvag_voice_swap.py`, com a mesma semântica:

- **`core/psarc.rs`:** cabeçalho `PSAR` com compressão, tamanho do TOC, tamanho da entrada,
  número de entradas e tamanho do bloco, tudo big-endian. Lista de blocos com 2, 3 ou 4 bytes
  conforme o tamanho do bloco. A entrada 0 lista os nomes, e cada nome é achado pelo MD5 do nome
  (com e sem `/` inicial). Um bloco do tamanho esperado está guardado sem compressão; os demais
  passam por zlib (`flate2`). Outra compressão é erro com o nome dela.
- **`core/voice_swap.rs`:** `read_xvag_header`, `replacement_frames` (VAGp ou XVAG), `swap_voice`
  (troca só os quadros e mantém o byte de flag de cada quadro do original), `is_inside`,
  `load_source`, `find_file` (sem diferença de caixa) e
  `run(game, source, update, language, only, dry_run, reference) -> Report { written, same,
  refused }`. Com algo escrito e sem `--dry-run`, copia `eboot.bin` e `sce_sys/param.sfo` do jogo
  para o update quando faltam. As mensagens de recusa são as do Python, e o motivo é o texto antes
  de `:`.
- **Linha de comando:** a análise dos argumentos é feita à mão, sem crate nova. Erro de argumento
  imprime o uso e sai com código 2, como o argparse. Erro de execução imprime
  `voice-swap: <erro>` e sai com 1. As linhas do relatório são as do Python
  (`N files written to X`, `N files would be written to X`,
  `N lines skipped: the recording is the original audio`,
  `N refused (reason), e.g. a, b, c`, em ordem decrescente de quantidade). Sucesso sai com 0.
  `attach_parent_console` mostra a saída num terminal do Windows.

`docs/VOICE_REPLACEMENT.md` (EN e PT-BR) troca `python tools/xvag_voice_swap.py …` por
`SpartanPort.exe --voice-swap …`, e no Linux por `spartanport-x86_64.AppImage --voice-swap …`.

### 3.6 Sobras do Python na pasta instalada

A instalação nunca apaga nada (§3.4). Então, depois do primeiro update de 2.7 para 2.8, a pasta
do jogador ainda tem o que só o pacote Python trazia. `legacy::remove_python_leftovers(port_dir)`
roda em `startup::prepare` quando `installed` (Windows, exe empacotado, fora de
`spartanport-update`):

- Só age se `<port>/_internal` for uma pasta. Essa pasta do PyInstaller prova que é uma
  instalação Python antiga. Um checkout nunca tem `_internal`, então nada dele é apagado.
- Apaga `_internal/`, `run.py`, `scripts/*.py`, `scripts/__pycache__/` e
  `tools/xvag_voice_swap.py`. `tools/` sai se ficar vazia.
- `scripts/controller_diagrams/` fica, porque o runner a lê (§4.1).
- Falhas são ignoradas e a limpeza tenta de novo na próxima abertura, como `remove_leftovers`.

### 3.7 Pasta do port e arquivos exigidos

- `paths::default_port_dir` reconhece uma pasta do port por `launcher/spartanport.png`, no lugar
  de `run.py`. Os dois existem num checkout, mas o pacote deixa de levar `run.py`. A busca pelos
  ancestrais (um build cargo dentro de `launcher-rust/target/…`) continua.
- `ui::play::missing_port_files` deixa de exigir `run.py` e continua exigindo
  `bin/spartanport-probe[.exe]`.

### 3.8 Dependências novas

- `crc32fast` (app id do Steam) e `flate2` (zlib do PSARC): já estão no `Cargo.lock` por causa do
  `zip`.
- `md-5` 0.10 (RustCrypto, a mesma família do `sha2`): nomes do PSARC.
- `windows-sys`: feature `Win32_System_Registry`.
- O `rfd` 0.17 já usa por padrão `xdg-portal` com `zenity` de reserva no Linux: o exe não linka
  GTK.

## 4. Empacotamento

### 4.1 Windows (`packaging/windows/package.sh`)

- Pré-requisitos: as saídas do `build.sh` e o `cargo` do rustup com o alvo
  `x86_64-pc-windows-msvc` (VS Build Tools). `CARGO` substitui o comando.
- `cargo build --release --locked --manifest-path launcher-rust/Cargo.toml`. Depois,
  `launcher-rust/target/release/spartanport-launcher.exe` vai para `SpartanPort.exe` (ícone e
  manifesto vêm do `build.rs`) e é copiado para `God of War III.exe`.
- `Play SpartanPort.exe` (de `play.c`) e a cópia `Play God of War III.exe` não mudam. Os dois nomes
  antigos ficam escritos no script, e um teste Rust confere que batem com `legacy::LEGACY_EXE` e
  `legacy::LEGACY_PLAY_EXE`.
- Saem: venv, PyInstaller, imports ocultos, `run.py`, `scripts/*.py`, `tools/` e a checagem do
  `CLIENT_ID` em `discord_presence.py` (o id é a constante `runner::discord::CLIENT_ID`).
- Ficam: `bin/` com as DLLs, `patches/`, os arquivos de `launcher/`,
  `scripts/controller_diagrams/` (lida por `runner/glyphs.rs`), `LICENSE`, `README.md`,
  `README-Windows.txt`, `docs/VOICE_REPLACEMENT.md`, `release.json`, a lista de proibidos, o zip e
  a atualização de `dist/spartanport-windows`.
- A lista de proibidos ganha `*.py` e `_internal`: nenhum Python entra no pacote.

`packaging/windows/smoke-test.ps1`:

- Sai a checagem `--script`.
- Entram três checagens: `SpartanPort.exe --version` imprime `SpartanPort <versão>` e sai com 0;
  `SpartanPort.exe --voice-swap --help` sai com 0; a árvore não tem `*.py`.
- Continuam: o carimbo, os proibidos, `spartanport-probe.exe --cpu-features` e a janela aberta
  por 15 s.

### 4.2 Linux (`packaging/linux/`)

- `build.sh`: `cargo build --release --locked`, instala `spartanport-launcher` em
  `usr/share/spartanport/`, passa as libs do exe pelo `bundle()` e deixa a checagem do glibc
  (2.39) valer também para ele. Saem PyInstaller, imports ocultos, `run.py` e `scripts/*.py`;
  entra `scripts/controller_diagrams/`.
- `AppRun`: não muda além do comentário (já executa `$share/spartanport-launcher`).
- `smoke-test.sh`: além da janela aberta no Xvfb, roda `AppRun --version`. Procura
  `panicked|GLIBC_` no log no lugar de `ImportError|Traceback`.
- `ci-deps.sh`: sai o venv de PyInstaller, Tk, Pillow e iced-x86. Entram o toolchain Rust e os
  pacotes que `rust-launcher.yml` instala (`libwayland-dev`, `libxkbcommon-dev` e
  `libgtk-3-dev`). O plano testa o build sem `libgtk-3-dev`; se passar, ele sai dos dois.

### 4.3 CI e release

- `windows-zip.yml`: entra `dtolnay/rust-toolchain@stable` antes do `package.sh`. O
  `setup-python` fica só para o step `Stamp` (`release_stamp.py` é ferramenta de build, não
  produto). O input `stamp` passa a ser ligado por padrão.
- `linux-appimage.yml`: mesma troca. O smoke continua na matriz de distros.
- `rust-launcher.yml`: não muda.

### 4.4 Desenvolvimento

- `run.sh` executa `launcher-rust/target/release/spartanport-launcher --run "$@"`. Se o exe
  faltar, roda antes `cargo build --release --manifest-path launcher-rust/Cargo.toml`.
  Continuam o `--software`, o nix-shell de dev, o `build.sh` e o passo do FSR 4. Sai o ramo
  `SPARTAN_PREBUILT`, que só o Nix usava.
- `README.md`: as seções de rodar a partir do código passam a usar cargo. A seção Python fica para
  as ferramentas e os testes que restam, com a nota de que launcher e runner Python estão
  congelados. `packaging/windows/README.md` e `README-Windows.txt` deixam de falar em Python e
  PyInstaller.

### 4.5 Removidos

`packaging/appimage.sh`, `packaging/default.nix`, `packaging/runtime-paths.nix` (gerado) e
`launcher/spartanport_vulkan.py`, mais as menções a eles nos READMEs.

## 5. Erros

| Situação | O que o jogador vê | Código |
|---|---|---|
| Add to Steam com o Steam aberto | o texto traduzido de hoje (feche o Steam) | — |
| Add to Steam sem Steam | "Steam was not found for this user." | — |
| Add to Steam sem `pic1.png` e `pic0.png` | o texto traduzido de hoje | — |
| Add to Steam, erro de arquivo | a mensagem do erro (até 400 caracteres) | — |
| Instalação falhou 30 vezes | diálogo de erro e a página de releases | 1 |
| `--voice-swap` com argumentos errados | o uso | 2 |
| `--voice-swap` com jogo, fonte ou update inválidos | `voice-swap: <erro>` | 1 |
| Sobras do Python em uso | nada (tenta na próxima abertura) | — |

## 6. Testes

TDD: teste falhando antes do código. Os testes Python são a lista de comportamentos.
`docs/specs/2026-10-23-rust-cutover-parity.md` marca cada caso como portado, substituído ou
descartado, com o motivo, contra a referência 2.7.0. `tests/test_cutover_parity_doc.py` (o
checador `tests/parity_doc.py`, parametrizado como em `test_runner_parity_doc.py`) mantém o mapa
honesto.

- **`test_steam_shortcut.py` (13):** VDF (3), plataforma (2), logo (2) e `add_to_steam` (6),
  com as mesmas entradas sintéticas (`pic1.png` 1920 × 1080 de cor sólida, ícone 256 × 256,
  contas `1111`, `2222` e `0`).
- **`test_xvag_voice_swap.py` (13):** XVAG e VAGp sintéticos, PSARC sintético com zlib e entradas
  fora de ordem, e a linha de comando com o relatório.
- **Linhas que mudam nos mapas existentes:**
  `test_launcher_portable_settings.py::…::test_user_files_includes_settings_and_icon`
  (`2026-10-21-rust-launcher-parity.md`) e
  `test_legacy.py::LauncherStartTest::test_the_update_installer_does_not_migrate`
  (`2026-10-22-rust-runner-parity.md`) passam de descartadas para portadas. As notas sobre
  `--script` e `--install-update` encaminhados ao Python são atualizadas.
- **Instalação:** `install_over` sobre pastas temporárias. O teste cobre `USER_FILES` em qualquer
  nível, a sobrescrita, um arquivo que existe só no destino e fica, e a nova tentativa depois de
  uma falha forçada.
- **Sobras:** `remove_python_leftovers` com e sem `_internal`, e `scripts/controller_diagrams/`
  preservada.
- **Rotas:** `tests/startup.rs` cobre `--install-update` (Windows, 2 argumentos ou não),
  `--voice-swap`, `--script` abrindo a janela, e `Route::Python` ausente.
- **Fronteira e CI:** `core_never_mentions_iced` continua. `rust-launcher.yml` roda fmt, clippy
  e `cargo test` nos dois sistemas. `windows-zip.yml` e `linux-appimage.yml` rodam os smoke tests
  dos pacotes, e os dois precisam passar antes da release.
- **Manual, antes da release:**
  - Instalar o pacote 2.8.0 por cima de uma cópia do 2.7.0 em `dist/`, pelo launcher Python.
    Confere: pasta limpa na primeira abertura, saves e configurações intactos, PLAY jogando.
  - Add to Steam com o Steam fechado e a arte conferida a olho no Steam.
  - `--voice-swap --dry-run` sobre o dump real.

## 7. Escopo

**Entra:** §3 e §4 inteiros, o mapa de paridade e seu checador, a versão 2.8.0, os READMEs e
`docs/VOICE_REPLACEMENT.md`.

**Fica fora:** apagar o Python do repositório (fica congelado); workspace Cargo (5); a mensagem
`restarting through run.py` do probe em C; macOS; a limpeza adiada do sub-projeto 2 (módulo único
para os helpers de JSON do Python, dividir `runner/mod.rs`, remover `mods::enabled_patch_files`,
varrer vistas `game-update-*` antigas, `Win32_UI_Shell` como dependência de dev).

## 8. Riscos

| Risco | Mitigação |
|---|---|
| Python 2.7 → Rust 2.8 é o único caminho de atualização do jogador | Contrato de §3.4 com teste das rotas, e o teste manual com o pacote 2.7.0 real antes da release. |
| `_internal/` em uso na primeira abertura | `_internal` só é usado pelo exe Python, que já fechou. Uma falha tenta de novo na próxima abertura. |
| iced no Xvfb do smoke Linux, sem GPU | O iced 0.14 cai do wgpu para o tiny-skia. O smoke só exige a janela viva por 15 s. |
| glibc do exe Rust acima de 2.39 | O build sai do Ubuntu 24.04 e a checagem do `build.sh` já cobre todo ELF do AppDir. |
| Cargo no shell MSYS2 do `package.sh` | `CARGO` substitui o comando. No CI, o toolchain fica no PATH do job antes do MSYS2. |
| Build cargo trava o PC local | `~/.cargo/config.toml` limita `jobs = 8`. |

## 9. Referências

- `scripts/steam_shortcut.py`, `tools/xvag_voice_swap.py` e `install_update` /
  `USER_FILES` de `launcher/spartanport_launcher_win.py` em `8db864e`.
- `docs/specs/2026-10-21-rust-launcher-design.md` (sub-projeto 1),
  `docs/specs/2026-10-22-rust-runner-design.md` (sub-projeto 2),
  `docs/specs/2026-10-22-version-menu-design.md` (carimbo e input `stamp`).
