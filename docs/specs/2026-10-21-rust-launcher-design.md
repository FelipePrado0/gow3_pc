# Launcher em Rust (sub-projeto 1 da migração Python → Rust)

Data: 2026-10-21. Estado: design aprovado em conversa; aguarda revisão do texto.

## 1. Objetivo

Substituir o launcher Python/Tk (`launcher/spartanport_launcher_win.py` e módulos, versão 2.6.0,
commit `541739b`) por um launcher em Rust com iced 0.14: binário único, abertura rápida, memória
limitada enquanto o jogo roda, interface nova no estilo launcher de jogo com a identidade
SpartanPort, e testes automatizados que substituem os 4.326 linhas de testes Python do launcher.

Este é o primeiro de cinco sub-projetos da decisão "todo Python do produto vira Rust":

1. **Launcher (esta spec).** Cria a pasta `launcher-rust/` e a UI completa.
2. **Runner.** `run.py` e a preparação (`prepare`, `link_libc`, `link_modules`, `isa_rewrite`,
   `patches`, `mods`, `glyphs`, `content_profile`, `guest_memory`, `graphics_options`,
   `save_backup`, `pause_menu`, `legacy`).
3. **Integrações.** `steam_shortcut`, `discord_presence`, `game_update`, auto-instalação da
   atualização do launcher.
4. **Packaging cutover.** `package.sh`, AppImage, smoke tests; remoção do PyInstaller e do Python.
5. **Cargo workspace** quando houver um segundo crate.

Cada sub-projeto tem spec, plano e implementação próprios. Nada é empacotado antes do 4.

## 2. Decisões já tomadas

| Tema | Decisão |
|---|---|
| Toolkit | iced 0.14 (wgpu com fallback tiny-skia, cosmic-text para árabe e CJK, `iced_test` headless). Tauri e Dioxus descartados por dependerem de webview (WebKitGTK frágil no Linux e no SteamOS). Slint ficou como segunda opção. |
| Alvos | Windows 10/11, Linux (AppImage), SteamOS em Gaming Mode. macOS só compila; sem runtime, fora de escopo (ADR 0001). |
| Desenho | GPU por padrão. Com o jogo rodando: log em lotes, janela escondida com `close_on_play`, intensidade do fogo zero sem janela visível. |
| Estrutura | Um crate (`spartanport-launcher`) em `launcher-rust/`, com `src/core/` proibido de importar iced. |
| Layout | Opção A2: arte em tela cheia, barra inferior com as abas à esquerda, PLAY à direita, gaveta deslizante pela esquerda. |
| Páginas largas | W1: gaveta de 72 % para Performance, Trophies, Controls, Mods e Log (Performance desde R95); 46 % para as demais. |
| Log | L2: página comum na gaveta larga, sem fundo preto. |
| Splash | S2: monograma SP sobre o fundo escuro, sem fogo (R96), só os textos das checagens (sem barra), monograma voa para o canto superior esquerdo e substitui o logotipo. |
| Idioma | Seletor no canto superior direito, abre por hover (0,45 s) ou clique, agrupado por Américas, Europa, Ásia e Oriente Médio, sem ícone de globo. |
| Home | Bloco à direita, de cima para baixo: logotipo SpartanPort (54 px), PLAY, requisitos em uma linha, linha de ajustes, dica. |
| Fundo | Gradiente vermelho de base, arte a 70 % de opacidade, véu escuro, névoa vermelha inclinada 7 ° (mais baixa à esquerda, mais alta à direita), calor pulsando, brasas e chamas subindo. |
| Referência visual | `docs/specs/assets/rust-launcher/launcher-mockup-v13.html` (abrir no navegador). Ajustes finos continuam durante a implementação, registrados nesta spec. |
| Base de código | `main` avançado por fast-forward para o SpartanPort 2.6.0 (`541739b`) em 2026-10-21. |

## 3. Arquitetura

### 3.1 Onde vive

- `launcher-rust/Cargo.toml`: pacote `spartanport-launcher`, binário `SpartanPort` (Windows:
  `SpartanPort.exe`; Linux: `spartanport-launcher`), os mesmos nomes que o PyInstaller produz hoje.
- `launcher/` (Python) fica intocado até o sub-projeto 4.
- Sem workspace na raiz até existir um segundo crate.

### 3.2 Módulos

```
launcher-rust/
├── Cargo.toml
├── build.rs                 # ícone e manifesto no Windows (winresource)
├── assets/                  # embutidos com include_bytes!
│   ├── background.png
│   ├── spartanport.png      # monograma SP (splash, canto da Home, ícone da janela)
│   ├── spartanport-logo.png
│   ├── spartanport.ico
│   ├── Cinzel-Black.ttf, Cinzel-Bold.ttf, OFL-Cinzel.txt
│   └── BodoniModa.ttf, OFL-BodoniModa.txt
├── fixtures/                # arquivos gerados pelo launcher Python 2.6.0 (paridade)
├── snapshots/               # imagens de referência do iced_test
└── src/
    ├── main.rs              # argumentos; sem argumento abre a janela
    ├── core/                # nunca importa iced
    │   ├── settings.rs      # settings.json, defaults, escrita atômica, migração legacy
    │   ├── ini.rs           # spartanport.ini preservando linhas e comentários
    │   ├── game.rs          # param.sfo, perfil do jogo, arquivos obrigatórios, ready_to_play, sce_sys
    │   ├── gpu.rs           # spartanport-gpu-capabilities --list, monitores
    │   ├── launch.rs        # SPARTAN_* a partir dos settings, comando, processo, grupo, pipe
    │   ├── log.rs           # ring buffer de 6.000 linhas, last_run.log, dicas de saída
    │   ├── i18n.rs          # 14 idiomas, detecção do idioma do sistema
    │   ├── trophies.rs      # .trp (ícones), saves (desbloqueados), reset
    │   ├── mods.rs          # mods.json, presets, conflitos
    │   ├── patches.rs       # patches.json, patches do jogo e de terceiros
    │   ├── save_backup.rs   # listar e restaurar
    │   └── update.rs        # GitHub Releases, download com .sha256
    └── ui/
        ├── app.rs           # State, Message, update, view, subscription
        ├── theme.rs         # paleta, fontes, estilos
        ├── splash.rs, home.rs, drawer.rs, lang_menu.rs, fire.rs
        ├── pages/           # graphics, performance, game, trophies, controls, mods, advanced, log
        └── widgets/         # chip, section, row, select, checkbox, button_icons
```

`src/main.rs` reconhece `--play`, `--run` e `--install-update`, mas neste sub-projeto apenas
repassa para o Python (seção 3.5).

### 3.3 Paleta e tipografia

| Nome | Valor | Uso |
|---|---|---|
| `bg` | `#0d0b0b` | fundo da janela |
| `panel` | `#161212` | barra inferior, gaveta |
| `card` | `#1d1717` | linhas selecionadas, selects, botões |
| `line` | `#2a2222` | bordas |
| `text` | `#e6e0d8` | texto |
| `muted` | `#8a817a` | dicas, subtítulos |
| `red` | `#a4161a` | PLAY, checkbox marcado |
| `red_hi` | `#c81e25` | hover, seções, aba ativa |
| `silver` | `#c9c4bd` | chips |
| `gold` | `#b8862b` | troféus de ouro |

Cinzel Black nos títulos de página e no PLAY, Cinzel Bold nas abas e cabeçalhos de seção, Bodoni
Moda onde o launcher 2.6.0 já a usa, fonte do sistema para o resto. CJK e árabe pelas fontes do
sistema via cosmic-text.

### 3.4 Toolchain e dependências

- Rust estável via rustup; alvos `x86_64-pc-windows-msvc` (VS Build Tools com C++; o runner do
  GitHub já tem) e `x86_64-unknown-linux-gnu`. Versões fixadas no `Cargo.toml`.
- `iced` 0.14 (features `wgpu`, `tiny-skia`, `image`, `canvas`, `advanced`), `iced_test` 0.14,
  `rfd` (diálogos de pasta e mensagens), `image` (PNG), `serde`/`serde_json`, `ureq` (GitHub),
  `sha2`, `thiserror`, `winresource` (build), `windows-sys` só em `core/launch`.
- `unsafe` permitido apenas em chamadas Win32 isoladas dentro de `core/launch` (job de processos,
  `CREATE_NO_WINDOW`).

### 3.5 Transição até o runner existir

Para jogar, o launcher executa `python run.py` do checkout: `SPARTANPORT_PYTHON` se definido,
senão `py -3` no Windows e `python3` no Linux. Botões que dependem de módulos Python sem linha de
comando (Adicionar ao Steam, instalar a atualização) chamam o Python por uma ponte mínima
`python -c` até o sub-projeto 3. A ponte vive em `core/launch.rs` e some no sub-projeto 4.

### 3.6 Integração com o repositório

- `build.sh` não muda.
- Novo workflow `.github/workflows/rust-launcher.yml`: `cargo fmt --check`, `cargo clippy -D
  warnings`, `cargo test` (inclui snapshots) em `windows-2025` e `ubuntu-24.04`; `cargo build
  --release` publicado como artefato.
- `.gitignore` ganha `launcher-rust/target/`, `!launcher-rust/assets/*.png`,
  `!docs/specs/assets/**/*.png` e `.superpowers/`.

## 4. Interface

### 4.1 Estado e mensagens (Elm)

`State`: `settings` (valores atuais), `saved` (último gravado), `page` (Home ou uma das 8),
`drawer` (fechada, 46 %, 72 %), `splash` (fase e checagens concluídas), `game` (parado,
preparando, rodando com PID, saída com código), `log` (ring buffer), `gpus`, `monitors`,
`trophies`, `mods`, `patches`, `update` (nenhuma, disponível, baixando %, erro), `lang`,
`lang_menu` (fechado, hover pendente, aberto), `scale`, `effects` (fogo ligado ou não).

`Message`: navegação (`ShowPage`, `CloseDrawer`), edição (`Set(key, value)`, `Browse(folder)`),
ações (`Play`, `Stop`, `Save`, `CheckUpdate`, `InstallUpdate`, `ClearShaderCache`, `AddToSteam`,
`CreateShortcut`, `RestoreBackup`, `MoveMod`, `SavePreset`, `LoadPreset`, `DeletePreset`,
`ResetTrophies`, `ResetControls`), eventos de fundo (`GpusListed`, `MonitorsListed`,
`GameOutput(lines)`, `GameExited(code)`, `UpdateFound`, `UpdateProgress`, `SplashCheckDone`),
relógio (`Tick`), idioma (`LangHover`, `LangOpen`, `LangPick`), janela (`Resized`, `Focused`).

`update()` é a única função que altera o estado; `view()` é função pura do estado; `subscription()`
une o relógio do fogo, a leitura do pipe e os eventos de teclado.

### 4.2 Splash

Fundo `#070606` sem fogo (R96), monograma SP (200 px) com brilho vermelho, texto da checagem corrente
abaixo. O splash existe para duas checagens (R97), cujos textos se revezam nessa ordem, cada um por
pelo menos metade do mínimo (0,4 s) mesmo com as respostas já dadas, e nenhum sai antes da sua
checagem responder (R86): "Checking the settings…" (`settings.json` e `spartanport.ini` legíveis: a
leitura feita ao abrir, R69; um arquivo ilegível ainda traz o aviso de sempre) e "Checking for
updates…" (só consulta com `check_updates` ligado; desligado, o texto aparece e a vez termina no
mínimo). A lista de GPUs e a checagem da pasta do jogo rodam em paralelo, sem texto próprio e sem
segurar o splash; os resultados alimentam os chips da Home e o PLAY quando chegam. O splash termina
quando os dois textos tiveram a vez, com mínimo de 0,8 s e máximo de 3 s; qualquer tecla ou clique
pula. Transição de 0,7 s: o monograma voa para o canto
superior esquerdo (60 px) com `Animation` e easing `EaseOut`, o fundo escuro e os textos somem, a
Home aparece. O monograma fica no canto e é a única identificação do launcher na janela.

### 4.3 Home

Fundo em cinco camadas, de baixo para cima:

1. Base: gradiente `#1b1414` → `#0d0b0b` com brilho radial vermelho em (68 %, 38 %) e cinza em
   (22 %, 78 %).
2. Arte a 70 % de opacidade, `cover`, posicionada em (70 %, 30 %): a arte própria do launcher,
   `launcher-rust/assets/background.png`, decidida pelo usuário em 2026-10-22.
3. Véu escuro uniforme de 32 %, escurecimento pela esquerda (88 % → 0 % até 58 % da largura),
   vermelho `#8c1014` subindo do rodapé.
4. Névoa vermelha e preta do rodapé girada −7 ° (encosta no chão à esquerda, sobe à direita).
5. Calor laranja pulsando em 2,8 s (blend aditivo), brasas e chamas: nascem numa linha que sobe até
   22 % da altura no lado direito, vivem até 70 % mais lá. Shader wgpu; versão em `canvas` quando o
   renderer é tiny-skia. Intensidade 0 no splash (R96: o fogo começa com a Home e aparece durante
   o voo do monograma, à medida que o fundo escuro do splash some), 0,8 na Home, 0,3 sem foco, 0
   com a janela escondida ou com a opção "Efeitos" desligada.

Canto superior direito: pílula "Version X is available · Update" quando há atualização, e o
seletor de idioma (nome do idioma atual e seta). Bloco inferior direito, alinhado à direita:
logotipo SpartanPort (54 px), botão PLAY em Cinzel Black (vira RUNNING desabilitado com STOP ao
lado), chips em uma linha sem quebra (saves, título e versão do jogo, placa de vídeo; a linha de
ajustes substitui o antigo chip de requisitos), linha de ajustes (resolução de saída, FPS, modo de
janela, monitor: só chaves que a página Graphics oferece; upscaler e preset voltam quando ela os
oferecer), dica "Insert ou R3+L2", que dá lugar ao status do jogo enquanto ele prepara, roda ou
copia os arquivos de sistema e, com o jogo parado, ao aviso dos patches que a última partida não
achou no eboot.bin (R85).

### 4.4 Barra e gaveta

Barra inferior de 52 px com as abas Home, Graphics, Performance, Game, Trophies, Controls, Mods,
Advanced, Log em Cinzel Bold; aba ativa com fundo `card` e sublinhado `red_hi`; à direita, a
versão e o sistema (`v2.6.0 · Windows` ou `· Linux`). A gaveta desliza da esquerda em 220 ms
(`cubic-bezier(.2,.8,.2,1)`), largura 46 % ou 72 % (a troca de uma para a outra desliza com o mesmo
tempo e curva), cabeçalho com título em Cinzel Black e subtítulo, corpo com rolagem. Abrir a gaveta desfoca e
escurece o fundo; com 72 % o bloco do PLAY esmaece a 35 % e para de responder. Clicar no fundo,
Esc ou a aba Home fecham. Um fio de 1 px em `red` marca o topo da barra, a borda interna da gaveta e,
na página Log, a separação entre os botões e o texto (R88).

O conteúdo das páginas segue o launcher 2.6.0 linha a linha: as mesmas seções, controles, dicas,
textos e chaves de settings de `build_graphics`, `build_display`, `build_performance`,
`build_game`, `build_save_backups`, `build_patches`, `build_third_party_patches`,
`build_trophies`, `build_controls`, `build_mods_page`, `build_linux_options`, `build_advanced` e
`build_log`. Advanced ganha uma linha nova, "Efeitos" (fogo ligado ou desligado). Cada ajuste é uma
linha do mockup (`.row`): o nome à esquerda e o controle (lista, caixa de seleção, botão) na ponta
direita, com a dica embaixo; a caixa de seleção vem depois do nome, que também a alterna. Uma
pasta é um campo editável de uma linha entre o nome e os botões Procurar e Abrir (mockup
`.folder`, R89): digitar tem o mesmo efeito de escolher no diálogo.

### 4.5 Seletor de idioma

Botão com o nome do idioma atual; hover de 0,45 s ou clique abre a lista; sair do botão e da
lista fecha em 0,3 s. Três grupos com cabeçalho em Cinzel vermelho, traduzidos: Américas (English,
Português (Brasil), Español), Europa (Français, Deutsch, Italiano, Polski, Русский, Türkçe), Ásia e
Oriente Médio (العربية, 中文, 日本語, 한국어, Bahasa Indonesia). Escolher grava `ui_language`,
reconstrói as views sem perder página, log nem jogo em execução, e atualiza a linha "Launcher
language" em Advanced. Árabe liga o layout RTL.

### 4.6 Teclado e controle

Esc fecha a gaveta; ← e → trocam de aba; Enter na Home dá PLAY; Tab percorre os controles da
página aberta. No Steam Deck o Steam Input traduz o gamepad para essas teclas; o launcher não lê
gamepad diretamente neste sub-projeto. Janela mínima 1280 × 800.

### 4.7 Idiomas, fontes e escala

As tabelas de `spartanport_lang.py` e `spartanport_trophies.py` são convertidas uma vez para JSON
por `tools/launcher_i18n.py` e embutidas; o inglês é a chave, entrada ausente cai no inglês, o
russo continua ao lado do inglês como hoje. Escala: 1x (padrão) ou 1,5x pela opção GUI Scale,
absolutas em qualquer desktop (R90; sem Auto); um "auto" gravado antes vale 1x e um "2", 1,5x;
aplica depois do Save.

## 5. Fluxo de dados

### 5.1 Abertura

`main` lê `SPARTAN_DATA_DIR` (padrão: pasta do executável; o AppRun aponta para
`~/.local/share/spartanport`). Migração portátil igual ao 2.6.0: se `settings.json` não existe em
`DATA_DIR`, copia de `%APPDATA%\gow3-launcher` ou `~/.config/gow3-launcher`, com `game_icon.ico`.
Settings são os defaults sobrescritos pelo JSON; chaves desconhecidas são preservadas na gravação,
para o launcher da branch `native-renderer` e o público coexistirem. `spartanport.ini` é lido
linha a linha e regravado mantendo comentários e ordem; só as chaves conhecidas mudam.
`mods.json`, `mods_presets.json` e `patches.json` seguem o mesmo princípio. Toda gravação é
atômica: arquivo temporário na mesma pasta e rename.

### 5.2 Checagens do splash

As duas com texto no splash são as configurações e a atualização (R97); GPUs e pasta do jogo
rodam em segundo plano.

- Configurações: a leitura de `settings.json` e `spartanport.ini` feita ao abrir (R69).
- GPUs: `spartanport-gpu-capabilities --list` em `bin/` ou `out/`, timeout de 5 s, lista vazia
  se falhar.
- Pasta do jogo: `ready_to_play(game_dir, update_dir)` (param.sfo, eboot, arquivos obrigatórios,
  saves).
- Atualização: GET em `releases/latest`, timeout de 10 s, silencioso em erro.

### 5.3 Edição e Salvar

Cada controle manda `Set(key, value)`. Salvar ativa quando `settings != saved`; grava
`settings.json`, `spartanport.ini` e `mods.json` e atualiza `saved`. Trocar a pasta do jogo refaz
a checagem e recarrega patches, mods e troféus.

### 5.4 PLAY

Validações na ordem do 2.6.0: pasta com eboot; oferta de copiar `sce_sys` de uma extração de PKG;
arquivos obrigatórios do jogo e da atualização; pastas de jogo e atualização distintas. Depois:
backup dos saves; escrita dos textos dos troféus na pasta `user`; montagem das 33 variáveis
`SPARTAN_*` a partir dos settings, idêntica a `game_environment`; abertura de `last_run.log`;
início de `python run.py` com stdout e stderr num pipe, `CREATE_NO_WINDOW` no Windows e novo
grupo de processos no Linux. Uma thread lê o pipe linha a linha, grava no `last_run.log` e manda
`GameOutput` em lotes de até 250 ms. `update` empurra as linhas no ring buffer, reconhece "Entering
original x86-64 code" (status: rodando), "restarting through run.py" (reiniciando) e as dicas de
saída. `close_on_play` esconde a janela 5 s após o início se o processo ainda vive.
`GameExited(code)` escreve a linha de saída, fecha o grupo, restaura PLAY; se a janela estava
escondida, encerra o launcher.

### 5.5 STOP

Encerra o grupo de processos (job no Windows; `SIGTERM` no grupo no Linux) e espera a thread do
pipe terminar.

### 5.6 Atualização

A oferta mostra versão e notas. Aceitar baixa o pacote com progresso, confere o `.sha256`
publicado ao lado e delega a instalação ao Python 2.6.0 (`--install-update <versão> <pid>` no
Windows; substituição do AppImage no Linux). O launcher Rust só se instala sozinho a partir do
sub-projeto 3.

### 5.7 Troféus

O `.trp` do dump dá os ícones; os saves dão os desbloqueados; "Reset trophies" apaga o registro do
port. Recarrega ao abrir a página e ao fim do jogo.

## 6. Erros

Nenhum erro derruba o launcher nem some em silêncio: cada falha vira chip, status, linha no Log ou
caixa de mensagem com o texto que o 2.6.0 usa hoje, traduzido pela mesma tabela. `unwrap` e
`expect` proibidos fora de testes; `Result` com `thiserror` por módulo do `core`.

| Situação | Comportamento |
|---|---|
| `settings.json` ausente | defaults |
| `settings.json` ou `spartanport.ini` corrompido | aviso único, defaults em memória, arquivo intacto até o usuário Salvar |
| falha ao gravar | mensagem com o erro, estado preservado, Salvar continua ativo |
| migração legacy falha | ignorada |
| sem eboot, dump ou atualização incompletos, pastas iguais | caixa com a lista de arquivos, página Game aberta, não inicia |
| cópia de `sce_sys` interrompida | "Could not copy the files" com o erro |
| `spartanport-gpu-capabilities` ausente ou timeout | lista vazia, chip "not checked", opções automáticas |
| Python ou `run.py` ausente | "Could not start" com o erro, PLAY volta |
| pipe morre antes do processo | `GameExited` chega com o código real |
| saída com código ≠ 0 | linha de saída mais a dica reconhecida |
| `last_run.log` sem espaço | para de gravar o arquivo, continua drenando o pipe |
| STOP com processo morto | só limpa o estado |
| checagem de atualização falha | silenciosa na abertura, "Could not check for updates" quando manual |
| download falha ou SHA-256 não bate | "Update failed" com o motivo, arquivo apagado |
| wgpu sem dispositivo | tiny-skia, fogo em canvas |
| fonte sem glifo | fallback por glifo do cosmic-text |
| `.trp` ausente ou inválido | 35 nomes com ícones genéricos por tier |
| mod sem `mod.json` válido | listado pelo nome da pasta com aviso |
| `mods_presets.json` corrompido | lista vazia com aviso |
| fechar a janela com o jogo rodando | o jogo continua, como hoje |

## 7. Testes

TDD: teste falhando antes do código. Os testes Python do launcher são a lista de comportamentos;
`docs/specs/2026-10-21-rust-launcher-parity.md` marca cada caso de `tests/test_launcher_*.py`,
`tests/test_trophies.py` e `tests/test_button_icons.py` como portado, substituído ou descartado,
com o motivo, contra a referência 2.6.0 (`541739b`).

### 7.1 Núcleo (`cargo test`, sem janela)

- Paridade de arquivos: `tools/launcher_fixtures.py` roda o launcher Python 2.6.0 sobre entradas
  conhecidas e grava `settings.json`, `spartanport.ini`, `mods.json`, `patches.json` e
  `last_run.log` em `launcher-rust/fixtures/`; o Rust tem que produzir bytes iguais.
- Paridade de ambiente: as 33 variáveis `SPARTAN_*` para cada fixture, comparadas com a saída de
  `game_environment` capturada em JSON.
- Parsers com casos válidos, truncados, vazios e inválidos: param.sfo, `.trp`, `--list` de GPUs
  e monitores, `.sha256`, JSON de releases, migração legacy.
- Ring buffer: limite de 6.000 linhas, lotes, frases de status, dicas de saída.
- Checagem da pasta: dump completo, incompleto, extração de PKG com `sce_sys` ao lado, pastas
  iguais.
- i18n: cada tabela tem o número de chaves do inglês; nenhuma chave usada no código falta.
- Fronteira: teste que falha se `src/core/` contiver a string `iced`.

### 7.2 Interface (`iced_test`, headless)

- Splash: termina só com as três checagens, respeita 0,8 s e 3 s, pula com tecla.
- Navegação: cada aba abre a gaveta certa com a largura certa; Esc fecha; ← → percorrem; Enter
  dá PLAY só na Home.
- Edição: mudar um controle ativa Salvar; Salvar grava e desativa; trocar idioma preserva página,
  log e jogo em execução.
- PLAY: validações bloqueiam com a mensagem certa; um binário de teste que imprime e sai produz
  log, status "rodando" e saída com código.
- Seletor de idioma: hover abre após 0,45 s, sair fecha, clique alterna, grupos na ordem certa.
- Snapshots: Home, cada página e o splash em 1280 × 800 e 1920 × 1080, EN, PT e AR, comparados
  no CI com tolerância para fontes; imagens de referência em `launcher-rust/snapshots/`.

### 7.3 Processo real

Abrir o launcher Rust da worktree, jogar uma rota curta, fechar, conferir `last_run.log` e os
arquivos gravados. Só então o sub-projeto conta como pronto. O jogo do usuário nunca é aberto nem
fechado sem autorização.

## 8. Escopo

**Entra:** `launcher-rust/` compilando em Windows e Linux com as 8 páginas, splash, Home, gaveta,
Log, seletor de idioma, 14 idiomas, troféus, mods, patches, backups de saves, verificação da
pasta, PLAY/STOP com log ao vivo, oferta de atualização, escala, teclado; mesmos arquivos e chaves
do 2.6.0 (alternar entre o launcher Python e o Rust não perde nada); testes da seção 7; workflow
de CI; `.gitignore`; fixtures; mockup v13 em `docs/specs/assets/rust-launcher/`.

**Fica fora:** runner, preparação, integrações nativas, packaging, remoção do Python
(sub-projetos 2 a 4); leitura direta de gamepad; macOS; a opção "renderer
nativo" da branch `native-renderer` (entra quando essa branch receber o merge do `main`).

## 9. Riscos

| Risco | Mitigação |
|---|---|
| iced 0.14 recente; `iced_test` e `Animation` podem mudar na 0.15 | versões fixadas; atualização só por decisão |
| árabe RTL: cosmic-text faz shaping e bidi, o espelhamento do layout é nosso | snapshot em árabe desde o início |
| shader de fogo em laptops híbridos e no Deck | intensidade por foco e visibilidade; opção "Efeitos" |
| toolchain: rustup e VS Build Tools no Windows | documentado em `launcher-rust/README.md`; CI cobre |
| paridade byte a byte do `spartanport.ini` | fixtures pegam qualquer desvio |
| launcher Python público muda durante o port | `-parity.md` registra a versão de referência; cada merge reabre a lista |
| mockup difere do iced em antialias, desfoque e partículas | igualdade de layout, cores, tempos e comportamento; não pixel a pixel; ajustes finos registrados aqui |

## 10. Referências

- Mockup: `docs/specs/assets/rust-launcher/launcher-mockup-v13.html` (a arte do olho é
  `launcher-rust/assets/background.png`, R72; o mockup a lê como `background.avif` ao lado dele, e
  qualquer imagem 16:9 com esse nome mostra a Home com arte).
- Launcher Python de referência: `launcher/spartanport_launcher_win.py` 2.6.0, commit `541739b`.
- Roadmap do renderer nativo: `docs/plans/2026-10-18-native-renderer-roadmap.md` (regras de
  operação herdadas: spec antes de código, TDD, nunca abrir ou fechar o jogo sem autorização,
  screenshots PT e EN para UI nova).
- ADR 0001 (`docs/adr/`): macOS fora de escopo.
- iced 0.14: https://github.com/iced-rs/iced/releases, https://docs.rs/crate/iced_test/latest
