# Menu de versões do launcher (extensão do updater do sub-projeto 1)

Data: 2026-10-22. Estado: implementado (branch rust-launcher, 2026-10-22).

## 1. Objetivo

O SpartanPort publica várias builds de teste (pre-releases do GitHub, uma por issue) entre as
versões estáveis. Hoje quem quer testar uma baixa o zip no GitHub por fora; o launcher só conhece
`releases/latest` e só oferece a estável mais nova. Esta spec troca o pill "Version X is
available" por um menu de versões no canto superior direito da Home, ao lado do seletor de idioma,
onde o usuário vê as estáveis e as builds de teste do seu sistema e instala a que quiser, inclusive
voltando para uma estável. O launcher passa a saber qual release está rodando.

Vale para o launcher Rust (`launcher-rust/`) na branch `rust-launcher`. O launcher Python não muda.

## 2. Decisões

| Tema | Decisão |
|---|---|
| Lugar | Botão no canto superior direito da Home, antes do idioma: `[ Release · 2.6.0 ▾ ] [ Português ▾ ]`. Mesmo comportamento do seletor de idioma (spec 4.5): hover 450 ms abre, sair 300 ms fecha, clique alterna. |
| Grupos | Dois cabeçalhos como as regiões do idioma: Releases e Pre-releases. |
| Quantidade | 3 estáveis mais novas; todas as pre-releases, mais nova primeiro, com scroll. |
| Pill | Sai. Estável mais nova que a instalada: o botão acrescenta `· 2.7.0 disponível` e a linha ganha a marca "novo". A checagem na abertura (`check_updates`) continua e alimenta o mesmo botão. |
| Identidade da release instalada | Carimbo `release.json` dentro do pacote, gravado pelos workflows a partir da tag. Sem carimbo: `Release · VERSION`. |
| Só launcher Rust | Entram na lista só releases que têm o asset `release.json`. As da era Python não têm e somem sozinhas. |
| Sistema | Decidido em compilação (`cfg!(windows)`), como hoje: Windows lista quem tem `.zip`, Linux quem tem `.AppImage`. O nome do sistema não aparece na interface. |
| Downgrade | Permitido. O diálogo de confirmação avisa quando a escolhida é mais antiga que a instalada. |
| Settings | Nenhuma chave nova. |
| Fora | Notas da release no menu, canal preferido persistido, filtro por issue, aviso de build de teste nova, macOS, launcher Python. |

## 3. Núcleo (`src/core/update.rs`)

### 3.1 Tag e identidade

```rust
pub enum Channel { Release, PreRelease }

pub struct ReleaseId {
    pub channel: Channel,
    pub version: String,       // "2.6.0"
    pub build: Option<String>, // "issue209-test1" numa build de teste
}
```

`ReleaseId::parse(tag, prerelease)`: a tag é `<sistema>-v<versão>[-<build>]` (`windows-v2.7.0`,
`windows-v2.6.0-issue209-test1`, `linux-v2.2.1-test1`). O prefixo de sistema é ignorado; a versão é
a primeira sequência de dígitos e pontos depois de `-v`; o que sobra depois do hífen é a build.
O canal vem do campo `prerelease` da API, não da tag. Tag sem `-v<dígitos>` é inválida e a release
é ignorada.

`Display` dá o rótulo: `Release · 2.6.0` e `Pre-release · 2.6.0 · issue209-test1`. "Release" e
"Pre-release" são traduzidos.

### 3.2 Lista

```rust
pub struct Release {
    pub id: ReleaseId,
    pub published: String,   // `published_at` da API, ISO 8601, para ordenar
    pub url: Option<String>, // pacote deste sistema
    pub page: String,        // html_url
}

pub fn list_releases(windows: bool) -> Result<Vec<Release>, UpdateError>;
```

Uma chamada `GET https://api.github.com/repos/FelipePrado0/SpartanPort/releases?per_page=100`
(`Accept: application/vnd.github+json`, timeout global 15 s, user agent de hoje). `parse_releases`
descarta drafts, releases sem o asset `release.json`, sem pacote do sistema (`.zip` no Windows,
`.appimage` no Linux) e com tag inválida; ordena por `published` decrescente. A constante
`RELEASES_API` passa a apontar para a lista; `latest_release` vira a primeira estável dela, e a
chamada a `releases/latest` sai.

`stables(&[Release]) -> Vec<&Release>` (até 3) e `prereleases(&[Release]) -> Vec<&Release>` são
funções puras sobre a lista.

### 3.3 Release instalada

`installed_release(port_dir, appdir) -> ReleaseId`: lê `release.json` em `port_dir` no Windows e em
`$APPDIR` no Linux (AppImage). Arquivo ausente ou inválido vira `Release · VERSION` sem aviso.

Formato do carimbo:

```json
{"channel": "pre-release", "version": "2.6.0", "build": "issue209-test1"}
```

`channel` é `release` ou `pre-release`; `build` é omitido numa estável.

### 3.4 Novidade

`newer_stable(list, installed) -> Option<&Release>`: a primeira estável cuja `version_tuple` é maior
que a de `installed.version`. Uma build de teste nunca é "nova": só aparece no menu. Quem roda
`2.6.0 · issue209-test1` vê "novo" só quando sair a 2.7.0. `is_newer` de hoje passa a comparar
contra o carimbo, não contra `VERSION`.

## 4. Interface

### 4.1 Estado e mensagens

```rust
pub enum UpdateState {
    None,
    Checking,
    Ready { releases: Vec<Release>, newer: Option<Release> },
    Downloading(Release, u8),
    Failed(String),
}
```

`Available(Release)` sai. `UpdateMessage::Install` passa a carregar a `Release` escolhida;
`Checked` passa a carregar `Result<Vec<Release>, String>`. `State` ganha `installed: ReleaseId`
(lido na abertura) e `version_menu: HoverMenu`.

Os timers de hover de `lang_menu.rs` (`open`, `hover_since`, `leave_since`, `Enter`, `Leave`,
`Toggle`, `Tick`) saem para `src/ui/hover_menu.rs` (`HoverMenu`, `HoverMessage`, `update`,
`timing`); `lang_menu` e `version_menu` usam o mesmo. Abrir um menu fecha o outro.

### 4.2 Botão (`src/ui/version_menu.rs`, `button_view`)

Texto de 12 pt com `▾`, mesmo estilo e padding do botão de idioma, à esquerda dele em
`home::top_right` (à direita, da direita para a esquerda). Rótulos por estado:

| Estado | Texto |
|---|---|
| sem novidade | `Release · 2.6.0 ▾` |
| estável nova | `Release · 2.6.0 · 2.7.0 disponível ▾` |
| build de teste instalada | `Pre-release · 2.6.0 · issue209-test1 ▾` (mais `· 2.7.0 disponível` quando houver) |
| baixando | `Baixando 2.7.0… 42%` (texto de hoje, não clicável, menu fechado) |

Desabilitado junto com o resto da Home (`home_interactive_enabled`). Sempre visível, inclusive
quando o launcher não consegue se instalar (`self_update` falso): nesse caso instalar abre a página
da release, como o pill faz hoje (`InstallPlan::OpenPage`).

### 4.3 Menu (`menu`)

Cartão sob o botão, mesma posição e largura mínima do menu de idioma. Linhas de 12,5 pt:

```
RELEASES
  2.7.0                 novo
✓ 2.6.0
  2.5.0
PRE-RELEASES
  2.6.0 · qte-bmi1-test1
  2.6.0 · startup-test1
  2.6.0 · issue209-test1
  …
```

- Releases: até 3; `✓` na instalada (mesma versão e build); "novo" à direita, em `RED_HI`, na
  estável de `newer`.
- Pre-releases: todas, numa `scrollable` de até 8 linhas visíveis; além disso, scroll. Lista vazia
  mostra uma linha apagada "Sem builds de teste para Linux" (no Windows "Sem builds de teste").
- Sem lista ainda (`None`): abrir o menu dispara `Check { manual: false }` e mostra "Buscando…".
  `Checking`: "Buscando…". `Failed`: "Não foi possível buscar", e o próximo abrir tenta de novo.
- Clique numa linha fecha o menu. Na instalada, só isso. Nas outras, `Install(release)`.

### 4.4 Instalar

`Install(release)` segue `install_decision` de hoje com a release escolhida: jogo rodando bloqueia;
sem pacote ou sem poder se instalar abre a página; pasta do AppImage só leitura pergunta antes de
abrir a página; senão o diálogo "Install version {} now? The launcher closes, installs it and opens
again. Saves and settings are kept." com o rótulo da release. Quando
`version_tuple(release.version) < version_tuple(installed.version)` o diálogo acrescenta a linha
"This version is older than the installed one." Build de teste sobre a estável de mesma versão não
é "mais antiga".

Download, SHA-256, `--install-update` no Windows e troca do AppImage no Linux não mudam. Ao
terminar o launcher fecha e o novo abre com o próprio carimbo.

### 4.5 Advanced

"Check for updates" continua e refaz a lista; nada mais muda na página. O resultado manual sem
estável nova mostra "You have the latest version ({})" com a versão instalada, como hoje.

### 4.6 Textos

Chaves novas em `assets/i18n.json`, traduzidas nos 14 idiomas como as demais: "Release",
"Pre-release", "Releases", "Pre-releases", "new", "{} available", "Fetching…", "Could not fetch
the versions", "No test builds", "No test builds for Linux", "This version is older than the
installed one.". "Version {} is available." e "Update" saem com o pill.

## 5. Carimbo e workflows

- `packaging/windows/package.sh` e `packaging/linux/build.sh`: quando `release.json` existe na raiz
  do repositório, copiam para a raiz do pacote (`spartanport-windows/release.json`; raiz do AppDir
  no AppImage). Sem o arquivo o pacote sai como hoje.
- `.github/workflows/windows-zip.yml` e `linux-appimage.yml`: um passo "Stamp" antes do build,
  só quando `inputs.tag` não é vazio, grava `release.json` a partir da tag: `version` e `build`
  pela regra de 3.1; `channel` por `gh release view "$TAG" --json isPrerelease` (release inexistente
  conta como pre-release, que é o que o passo de publicação cria). O passo de publicação anexa
  `release.json` à release junto com o pacote e o `.sha256`.
- Os smoke tests (`packaging/windows/smoke-test.ps1`, `packaging/linux/smoke-test.sh`) conferem
  que `release.json` está no pacote quando o build teve tag.
- Só pacotes novos vêm carimbados. Instalações antigas aparecem como `Release · VERSION` até a
  primeira troca pelo menu.

## 6. Erros

| Situação | Comportamento |
|---|---|
| busca da lista falha na abertura ou ao abrir o menu | "Não foi possível buscar" no menu; sem caixa |
| busca falha no "Check for updates" | "Could not check for updates: {}" como hoje |
| `release.json` ausente ou inválido no pacote | `Release · VERSION`, sem aviso |
| tag fora do padrão, draft, sem asset `release.json` ou sem pacote do sistema | release fora da lista |
| sem pre-releases para o sistema | linha apagada "Sem builds de teste…" |
| escolhida mais antiga | diálogo com a linha extra; instala se confirmar |
| escolhida é a instalada | menu fecha, nada acontece |
| download falha ou SHA-256 não bate | "Update failed: {}", volta a `Ready` |
| limite de 60 chamadas/hora da API sem token | tratado como falha de busca |

## 7. Testes

### 7.1 Núcleo (`cargo test`)

- `ReleaseId::parse`: estável, build de teste, `linux-v`, tag antiga `windows-v2.1.7-teste3`,
  tag inválida; `Display` dos dois canais.
- `parse_releases`: descarta draft, sem `release.json`, sem pacote do sistema, tag inválida;
  ordena por data; Windows pega o zip e Linux o AppImage; `stables` corta em 3; `prereleases`
  mantém todas.
- `installed_release`: carimbo válido, ausente, corrompido, `build` omitido.
- `newer_stable`: estável mais nova, igual, instalada em build de teste, lista vazia.

### 7.2 Interface (`iced_test`, `tests/ui_home.rs`)

- Botão com cada rótulo de 4.2; desabilitado com a Home; baixando não clicável.
- Menu aberto: cabeçalhos, 3 estáveis, `✓`, "novo", pre-releases, linha de lista vazia,
  "Buscando…", "Não foi possível buscar".
- Clique: instalada só fecha; outra manda `Install(release)`; diálogo com e sem a linha de mais
  antiga; abrir o menu de versões fecha o de idioma e vice-versa.
- Snapshots da Home (`tests/ui_snapshots.rs`) refeitas com o botão novo.

### 7.3 Pacote

Smoke tests dos dois sistemas conferindo `release.json` quando há tag (rodam no CI).

## 8. Riscos

- Uma página de 100 releases: as mais antigas somem quando o repositório passar disso. Aceito: a
  lista mostra 3 estáveis e as builds de teste recentes.
- Sem token a API limita a 60 chamadas por hora por IP. O launcher busca na abertura (quando
  `check_updates`), ao abrir o menu sem lista e no botão manual; a lista fica na sessão.
- O rótulo cresce em idiomas longos (`Pre-release · 2.6.0 · issue209-test1 · 2.7.0 disponível`);
  a linha do canto passa a usar `wrap()`, como os botões da Advanced, para não cortar o idioma.
- Nenhuma release tem o asset `release.json` até os workflows mudarem: até lá o menu lista só a
  instalada. O cutover de empacotar o launcher Rust segue no sub-projeto 4.

## 9. Referências

- `docs/specs/2026-10-21-rust-launcher-design.md` §4.5 (seletor de idioma) e §5.6 (atualização).
- `launcher-rust/src/ui/lang_menu.rs`, `launcher-rust/src/ui/update.rs`,
  `launcher-rust/src/core/update.rs`.
- `.github/workflows/windows-zip.yml`, `.github/workflows/linux-appimage.yml`.

## 10. Desvios

O que o código faz de diferente do texto acima. As seções anteriores seguem como o design
aprovado; este é o registro.

### 10.1 Previstos no plano

- `Release.url` é uma `String`, não uma `Option`: `parse_releases` descarta toda release sem o
  pacote deste sistema (§3.2).
- `UpdateState::Ready(Vec<Release>)` guarda a lista; `newer` não é armazenado, e sim derivado por
  `update::newer` a partir de `newer_stable(releases, &state.installed)`.
- O botão manual "Check for updates" (Advanced), quando há estável mais nova, só muda o texto do
  botão; nenhum diálogo abre.
- `ReleaseId::label(lang)` e `ReleaseId::short()` substituem uma implementação de `Display`, porque
  o rótulo depende do idioma.
- `core/release.rs` (identidade da tag, leitura da lista, `stables`/`prereleases`/`newer_stable`,
  `version_tuple`) foi separado de `core/update.rs`, que mantém a chamada HTTP, o leitor do
  carimbo e o código de instalação.

### 10.2 Encontrados na implementação

- Uma checagem que falha mantém a lista já exibida: `UpdateState::Failed` só é gravado a partir de
  `None`, `Checking` ou `Failed`; sobre `Ready(list)` a lista permanece (o modal de erro ainda
  aparece na checagem manual); `Downloading` nunca é alterado (a §4.1 dizia "Failed, exceto
  Downloading"). Commit 46db24e.
- `ModalKind` leva `#[allow(clippy::large_enum_variant)]`: `Message` cresceu com
  `UpdateMessage::Install(Release)`, `UpdateMessage::Confirmed(Release)` e
  `VersionMessage::Pick(Release)`, e `YesNo { yes, no }` guarda duas delas. Pôr as respostas em
  `Box` em todo lugar custava mais do que os bytes economizados. Commit 59cc916.
- `tests/ui_navigation.rs` (`keys_do_nothing_under_a_dialog`) foi adaptado para
  `UpdateMessage::Confirmed(release)`; o plano não listava esse arquivo.
- `State::new` ignora um `APPDIR` vazio: `APPDIR=""` cai em `port_dir/release.json`. Commit 46db24e.
- `BUTTON_ID` fica num `container` em volta do botão de versão, porque o `Button` do iced 0.14 não
  tem `id`. Os testes de layout medem os limites do `container`.
- "Não clicável com a tela escurecida" é testado com uma gaveta larga (`Page::Log`): um modal não
  desabilita a Home (`home_interactive_enabled` só olha para a gaveta larga), e "o clique não emite
  nada" é verificado como "nenhum `VersionMenu(Toggle)`", porque o simulador ainda emite `Enter` e
  `Leave` de hover.
- O texto do botão em português é `Lançamento · 2.6.0 · 2.7.0 disponível ▾` (a palavra do canal vem
  de `UI_EXTRA`, e "{} available" vira "{} disponível").
- As constantes de posição do menu de idioma (`EDGE_TOP`, `MENU_DROP`, `EDGE_RIGHT`, `MENU_WIDTH`)
  passaram a `pub(crate)` em `lang_menu.rs` e são usadas também por `version_menu::menu`: os dois
  menus aparecem no mesmo ponto, alinhados à direita, e só um fica aberto por vez.
- `ROW_HEIGHT = 29.0` é um teto: uma linha real mede cerca de 28,25 pt (texto de 12,5 pt, padding
  `[5, 12]`, espaçamento 2), então o limite das pre-releases (8 × 29 = 232) passa a valer a partir
  da 9ª linha e deixa ver uma faixa de uns 6 px dela. O teste fixa `height(9) == height(20)`,
  `height(8) < cap` e `height(7) < height(8)`. Commits 46c1910 e 5126021.
- As linhas apagadas (`Fetching…`, `Could not fetch the versions`, `No test builds` e
  `No test builds for Linux`) são `text` simples com o padding da linha, não botões.
- `packaging/release_stamp.py`: um valor inválido em `--prerelease` sai com código 2 (argparse); só
  uma tag inválida sai com código 1.
- Job de smoke do Linux: o `docker run` de `smoke-test.sh` repassa `-e SPARTAN_RELEASE_TAG`; um
  `env:` no nível do step não chega ao contêiner. Commit 669d95e.
- Workflow do Windows: o step `Stamp` roda em `shell: bash` (Git Bash), não no shell `msys2` do
  job, porque o `gh` não está no PATH do msys2. Os dois workflows imprimem um `::warning::` quando
  `gh release view` falha, antes de carimbar a build como pre-release (a §5 previa um fallback
  silencioso). Commit 2e77113.
- O asset do carimbo na release é um por sistema: `release-windows.json` e `release-linux.json`
  (`release::stamp_asset`), e `parse_releases` exige o deste sistema. O arquivo dentro do pacote
  continua `release.json` (`release::STAMP_FILE`, em `port_dir` ou `$APPDIR`). A §3.2 e a §5 diziam
  `release.json` para os dois: uma release costuma levar os dois pacotes, então o primeiro sistema
  a publicar marcaria a release como da era Rust também para o outro, e os dois jobs disputariam o
  mesmo asset. Commit b4b214f.
- Os dois workflows ganharam o input `stamp` (booleano, desligado por padrão): o step `Stamp`, o
  `SPARTAN_RELEASE_TAG` do smoke test e o upload do carimbo só acontecem com ele ligado. Ele só
  deve ser ligado quando o pacote levar o launcher em Rust (sub-projeto 4): uma release carimbada
  antes disso listaria no menu um pacote com o launcher em Python. Commit b7f37f3.
- No step `Stamp`, uma falha do `gh release view` que não seja "not found" falha o step com um
  `::error::`; só a release inexistente conta como pre-release. Substitui o `::warning::` do
  commit 2e77113, que carimbava como pre-release uma estável cuja consulta falhou. Commit b7f37f3.
- A release instalada é reconhecida por versão e build (`ReleaseId::same_build`), sem olhar o
  canal: o canal do carimbo fica congelado no build do pacote, e o da lista segue o `prerelease`
  atual da API. Uma release promovida ou rebaixada, ou uma estável carimbada como pre-release,
  continua com `✓` e não se reinstala. Vale para o `✓`, para `Pick` e para `Install`.
  Commit 6f7b745.
- Durante um download o menu fica fechado e inerte (a §4.2 passa a ser cumprida):
  `version_menu::update` ignora `Enter`, `Leave`, `Toggle`, `Tick` e `Pick` e fecha uma lista
  aberta, e `button_view` devolve o botão sem o `mouse_area`. Commit 43f96a1.
- `Decision::NoLinuxBuild` saiu: no Linux a lista já descarta as releases sem AppImage, então uma
  checagem manual cuja estável mais nova só tem pacote Windows diz "You have the latest version".
  Commit 59cc916.
- `UpdateState::Downloading { releases, release, percent }` substitui o `Downloading(Release, u8)`
  da §4.1, para a lista continuar no menu durante o download. As mensagens
  `UpdateMessage::OpenPage(String)` e `UpdateMessage::Confirmed(Release)` levam a página e a release
  escolhidas, porque o diálogo responde a qualquer linha do menu. Commit 59cc916.
