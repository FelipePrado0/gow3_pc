# Setup inicial do launcher Rust

Data: 2026-10-22. Estado: design aprovado em conversa; aguarda revisão do texto.
Branch: `rust-launcher`, sobre o launcher da spec `2026-10-21-rust-launcher-design.md`.

## 1. Objetivo

Quem abre o launcher Rust sem `settings.json` vê uma sequência guiada de três passos (pastas,
placa de vídeo, vídeo) antes da Home e sai dela com o jogo pronto para PLAY, sem procurar as
opções nas nove abas. O setup grava os mesmos arquivos e as mesmas chaves que as páginas
Game, Performance e Graphics já gravam; nenhuma chave nova entra em `settings.json` nem em
`spartanport.ini`.

Contexto: outra frente da mesma branch muda o default de `game_dir` para vazio (hoje o Windows
sugere `<pai do port>/CUSA01623`). Esta spec assume `game_dir` vazio na primeira abertura.

## 2. Decisões já tomadas

| Tema | Decisão |
|---|---|
| Gatilho | Só quando `settings.json` não existe depois da migração portátil. Arquivo corrompido não abre o setup: mantém o aviso único de hoje. |
| Pastas | `game_dir` (obrigatória, validada como no PLAY) e `update_dir` (opcional). Saves ficam no padrão da pasta de dados; `user_dir`, `mods_dir` e `patches_dir` ficam fora. |
| Vídeo | `display_mode`, `render_resolution` (com o filtro de aspecto da Performance), `fps_limit` e `engine_fps`. |
| Placa | `gpu_id`, lista do `spartanport-gpu-capabilities` que o splash já roda. |
| Sair no meio | "Skip" grava os defaults e o setup nunca mais aparece. Fechar a janela no meio não grava: o setup volta na próxima abertura. |
| Onde aparece | Tela cheia entre o splash e a Home: sem barra de abas, sem PLAY. Fundo igual ao do splash. |
| Formato | Três passos com Back/Next e indicador "Step N of 3"; Finish no último; Skip visível em todos. |
| Validação | Next só ativa quando a pasta do jogo passa na checagem do PLAY; o que falta aparece listado sob a pasta. |
| Implementação | Módulo `ui/setup.rs` no padrão de `splash.rs` e `play.rs`; os controles mandam os `Message::Set`, `Browse` e `Browsed` de hoje e editam `state.settings` e `state.ini` direto. |

## 3. Gatilho e estado

### 3.1 Detecção

`core::settings::read_app_settings` passa a dizer em que estado encontrou o arquivo:

```rust
pub enum SettingsFile {
    /// Não existe, mesmo depois de `migrate_portable_settings`.
    Missing,
    /// Lido e aplicado sobre os defaults.
    Read,
    /// Existe mas não é um objeto JSON legível (spec principal, §6).
    Unreadable,
}

pub fn read_app_settings(paths: &Paths) -> (Settings, SettingsFile);
```

`load_app_settings` continua devolvendo só `Settings`. `State::new` guarda o resultado em
`state.settings_file` e não abre o setup; `State::open` (o construtor que `app::run` usa) chama
`new` e cria o setup quando o resultado é `Missing`. `Unreadable` segue o caminho do aviso único,
sem setup. O ini não
participa da decisão: um ini presente sem `settings.json` ainda abre o setup, e os valores do
ini aparecem pré-selecionados no passo 3.

### 3.2 Estado

```rust
pub struct State {
    // ...
    /// O setup inicial, enquanto está aberto; `None` em todas as outras aberturas.
    pub setup: Option<SetupState>,
}

pub struct SetupState {
    pub step: Step,
    /// Os defaults da abertura (`persist::gather` em `State::open`): o que Skip grava.
    pub defaults: Persisted,
}

pub enum Step {
    Folders,
    Gpu,
    Video,
}
```

O setup vive atrás do splash. O splash roda como hoje (GPUs, pasta do jogo, atualização) e o
voo do monograma termina no canto. Quando `splash.done` e `state.setup` existe, `view` desenha o
setup no lugar da Home: sem `home::block`, sem barra de abas. Home, gaveta e PLAY só aparecem
quando `state.setup` volta a `None`.

"Nunca mais aparece" é consequência de `settings.json` existir depois de Finish ou Skip. Nenhum
flag é gravado.

### 3.3 Testes existentes

Os testes de hoje constroem o estado com `State::new` numa pasta temporária sem `settings.json`
e continuam sem setup, porque só `State::open` o cria. Os testes do setup usam `State::open`. Os
snapshots atuais não mudam.

## 4. Interface

### 4.1 Fundo

Igual ao splash: `SPLASH_BG` sólido, o fogo desenhado por cima do fundo com intensidade de
splash (1,0; `intensity_for` recebe `splash || setup aberto`), sem a arte, o véu e a névoa da
Home. O monograma fica no canto onde o voo o deixou (superior esquerdo; superior direito em
árabe). O seletor de idioma continua no canto superior direito (esquerdo em árabe), já que a
primeira abertura é o momento de escolher o idioma. A pílula de atualização não aparece durante
o setup; a oferta de atualização, quando chega, abre o diálogo de sempre por cima do cartão.

### 4.2 Cartão

Cartão central de 640 px de largura, estilo `theme::container_card` (o mesmo do modal),
centralizado na janela inteira (a barra não existe aqui). De cima para baixo:

1. Título "Initial setup" em `CINZEL_BOLD`, 22 px.
2. Linha "Step 1 of 3 · Folders" em `text_muted`.
3. Uma frase de introdução do passo, em `text_muted`.
4. Corpo do passo (abaixo).
5. Rodapé: "Skip" à esquerda (`button_flat`, texto `text_muted`); à direita, "Back"
   (`button_flat`, ausente no passo 1) e "Next" (`button_flat`); no passo 3, "Finish" com
   `button_play` no lugar de "Next".
6. Linha final em `text_muted`: "You can change all of this later in the Graphics,
   Performance and Game tabs."

Em árabe o `row(rtl, …)` de sempre espelha as linhas; Skip vai para a direita e Back/Next para
a esquerda.

### 4.3 Passo 1, Folders

Introdução: "Choose the folder of your game dump. The update folder is optional."

Duas linhas com o widget `folder` de hoje (caminho, Browse…, Open), com os mesmos rótulos,
dicas e `browse_prompt` da página Game: a pasta do jogo (`game_dir`) e a pasta da atualização
(`update_dir`). Sob cada pasta, quando a checagem falha, uma lista em `palette::RED_HI` com as
linhas de `play::describe_missing` precedida de "Missing or empty files:". Pasta de update
vazia é válida e não mostra lista. Pastas iguais mostram, sob a pasta de update, a frase do PLAY
"Choose two separate folders for the game and its update, or leave the update folder empty."

Next ativa (recebe `on_press`) só quando:

- `missing_game_files(game_dir, REQUIRED_GAME_FILES)` está vazia;
- `update_dir` está vazia ou `missing_game_files(update_dir, REQUIRED_UPDATE_FILES)` está vazia;
- `update_dir` está vazia ou `!same_folder(update_dir, game_dir)`.

A checagem roda em `setup::folder_problems(state, key)` a cada `view`; são leituras de poucos
arquivos, como `home::refresh_facts` já faz a cada troca de pasta.

### 4.4 Passo 2, Graphics card

Introdução: "The game runs on this graphics card."

Uma linha `select_pairs("gpu_id", …)` com as pairs de `performance::gpu_pairs` (Automatic,
depois as placas na ordem do jogo), rótulo e dica iguais aos da página Performance. Enquanto
`state.gpus` está vazia, a linha "Graphics card not checked: the game picks one." aparece sob o
seletor em `text_muted`, e a lista tem só Automatic. Quando `Check::Gpus` chega depois do splash, `state.gpus` muda e o passo
redesenha sozinho. Next sempre ativo.

### 4.5 Passo 3, Video

Introdução: "Resolution and frame rate. The in-game menu (Insert or R3+L2) changes the same
values."

Quatro linhas `select`, com rótulos e dicas das páginas Graphics e Performance:

| Chave | Lista | Origem |
|---|---|---|
| `display_mode` | `DISPLAY_MODES` | Graphics |
| `render_resolution` | `performance::rows` (filtro de aspecto, tamanhos dos mods) | Performance |
| `fps_limit` | `FPS_LIMITS` | Performance |
| `engine_fps` | `ENGINE_FPS_CHOICES` | Performance |

A linha "Aspect ratio" da Performance acompanha a resolução, como lá. Finish sempre ativo.

### 4.6 Teclado

Com o setup aberto: ← e → não trocam aba, Enter não dá PLAY, Esc não faz nada. Enter emite
`Next` quando Next está ativo, e `Finish` no passo 3. Um diálogo aberto continua tomando todas
as teclas, como hoje. Tab fica fora, como na spec principal (§4.6, desvio R67).

### 4.7 Textos novos

Entram em `UI_EXTRA` (R33: inglês e português no mínimo; os demais idiomas quando houver
tradução). Rótulos, dicas e `browse_prompt` que já existem na tabela JSON são reaproveitados
pela chave inglesa, nunca duplicados.

| Inglês | Português |
|---|---|
| Initial setup | Configuração inicial |
| Step {} of {} | Passo {} de {} |
| Video | Vídeo |
| Skip | Pular |
| Back | Voltar |
| Next | Avançar |
| Finish | Concluir |
| Choose the folder of your game dump. The update folder is optional. | Escolha a pasta do dump do jogo. A pasta da atualização é opcional. |
| The game runs on this graphics card. | O jogo roda nesta placa de vídeo. |
| Resolution and frame rate. The in-game menu (Insert or R3+L2) changes the same values. | Resolução e taxa de quadros. O menu do jogo (Insert ou R3+L2) muda os mesmos valores. |
| Missing or empty files: | Arquivos ausentes ou vazios: |
| Graphics card not checked: the game picks one. | Placa de vídeo não verificada: o jogo escolhe uma. |
| You can change all of this later in the Graphics, Performance and Game tabs. | Tudo isso pode ser mudado depois nas abas Graphics, Performance e Game. |

"Folders" e "Graphics card" já estão na tabela JSON (rótulo de seção da página Game e da linha
da Performance); os nomes dos passos 1 e 2 usam essas chaves e não entram em `UI_EXTRA`.

## 5. Fluxo de dados

### 5.1 Mensagens

```rust
pub enum SetupMessage {
    Next,
    Back,
    Skip,
    Finish,
}
// Em `Message`: Setup(SetupMessage)
```

Os controles do cartão mandam `Message::Set`, `Message::Browse` e `Message::Browsed` como as
páginas: `apply_set`, `game::folder_chosen` e `game::game_changed` (facts da Home, patches,
mods, troféus, tamanhos dos mods) rodam iguais. Nenhum caminho paralelo de edição.

### 5.2 Next e Back

Trocam `setup.step`. O botão Next só tem `on_press` quando `setup::can_advance(state)` é
verdadeiro, então `Next` nunca chega com o passo inválido; se chegar por teclado, `update`
confere `can_advance` e ignora.

### 5.3 Finish

`persist::save(state)`: grava `settings.json`, `spartanport.ini`, `mods.json` e `patches.json`
como o botão Save, e atualiza `saved`. Com sucesso, `state.setup = None` e a Home aparece com
PLAY. Com falha, o diálogo de erro do Save (`SAVE_FAILED`) abre e o setup continua aberto com o
estado preservado.

### 5.4 Skip

Restaura `state.settings` e `state.ini.values` a partir de `setup.defaults` (o `Persisted`
colhido em `State::open`, isto é, os defaults da abertura; `state.saved` não serve porque a troca
de idioma o recolhe de novo), roda `game::game_changed`, e segue igual ao Finish. O idioma escolhido no seletor durante o setup é preservado: `ui_language` é regravado
depois da restauração com o valor de `state.lang`.

### 5.5 Fechar a janela

`Message::CloseRequested` com `state.setup` presente fecha sem gravar, pelo mesmo gate que
`settings_unreadable` já usa em `persist::collect_unless_unreadable`. Na próxima abertura
`settings.json` continua ausente e o setup volta ao passo 1.

### 5.6 Idioma

Hoje escolher um idioma no seletor chama `persist::save`, que grava `settings.json`. Com o setup
aberto isso criaria o arquivo antes do Finish, então a escolha chama só `persist::rebuild`
(janela no novo idioma, nada gravado); `setup` e `step` são mantidos. O idioma é gravado no
Finish ou no Skip; fechar a janela antes disso o perde, como qualquer outra escolha.

### 5.7 Checagens do splash

Inalteradas. `Check::Gpus` entra em `state.gpus`; `Check::GameFolder` com `game_dir` vazio dá
`game_ready = Some(false)`, que o PLAY revalida depois do setup de qualquer forma.

## 6. Erros

| Situação | Comportamento |
|---|---|
| `settings.json` ausente | setup abre no passo 1 |
| `settings.json` corrompido | sem setup; aviso único de hoje |
| pasta sem `eboot.bin`, dump incompleto, update incompleta | Next desativado; lista vermelha de `describe_missing` sob a pasta |
| pastas de jogo e update iguais | Next desativado; frase do PLAY sob a pasta de update |
| `spartanport-gpu-capabilities` ausente ou timeout | lista só com Automatic; linha "Graphics card not checked: the game picks one." |
| diálogo de pasta cancelado | nada muda |
| falha ao gravar no Finish ou Skip | diálogo `SAVE_FAILED`; setup aberto; estado preservado |
| oferta de atualização chega durante o setup | diálogo de sempre por cima do cartão |
| janela fechada no meio | nada gravado; setup volta na próxima abertura |

## 7. Testes

### 7.1 Núcleo (`cargo test`, sem janela)

Em `core::settings`:

- arquivo ausente dá `SettingsFile::Missing` e os defaults;
- arquivo legível dá `Read` com os valores aplicados;
- `{not json`, `[1, 2]` e vazio dão `Unreadable`, não `Missing`;
- com um `settings.json` legado em `%APPDATA%\gow3-launcher`, a migração roda antes e o
  resultado é `Read`.

### 7.2 Interface (`iced_test`, `tests/ui_setup.rs`)

- Sem `settings.json`, `State::open` nasce com `setup` no passo `Folders` e a view não tem barra
  nem PLAY; com `settings.json` presente (legível ou não) nasce sem `setup`; `State::new` nunca
  o cria.
- Passo 1: Next sem `on_press` com `game_dir` vazio; ativa com `dump_with_patches`; desativa
  com `update_dir` igual ao dump; a lista vermelha mostra o `describe_missing` do dump vazio.
- `Browsed("game_dir", dump)` troca a pasta e os facts da Home (título do jogo) acompanham.
- Passo 2: as pairs são Automatic mais `gpu_rows`; com `gpus` vazia só Automatic e a linha
  "Graphics card not checked: the game picks one.".
- Passo 3: `Set("fps_limit", "60")` muda `state.ini.values`.
- Finish escreve `settings.json` e `spartanport.ini` com os valores escolhidos e limpa `setup`.
- Skip escreve os defaults (`game_dir` vazio, ini default), mantém `ui_language` trocado, limpa
  `setup`.
- `CloseRequested` com setup aberto não cria `settings.json`.
- ← → e Enter com setup aberto não mudam `page` nem iniciam o jogo; Enter com Next ativo avança.
- Troca de idioma mantém `setup` e `step`, muda `state.lang` e não cria `settings.json`.

### 7.3 Snapshots

`setup1`, `setup2`, `setup3` em 1280×800 e 1920×1080, `en`, `pt` e `ar` (18 PNGs), no esquema
de `ui_snapshots.rs`: `show_fixed_paths`, `gpu_rows` de fixture no passo 2, dump de fixture no
passo 1.

### 7.4 Processo real

Fica na Task 39 do plano principal, com autorização do usuário: abrir o executável sem
`settings.json`, concluir o setup, conferir os arquivos gravados.

## 8. Escopo

Fora desta spec: `user_dir`, `mods_dir` e `patches_dir` no setup; reabrir o setup por um botão
em Advanced; gamepad direto; Tab entre os controles; qualquer chave nova; mudanças na Home, nas
páginas ou no splash além do gate do `view` e do `intensity_for`.

## 9. Referências

- Spec principal: `docs/specs/2026-10-21-rust-launcher-design.md` (§4.2 splash, §4.6 teclado,
  §5.3 Salvar, §6 erros).
- Paridade e regras (R33, R35, R67): `docs/specs/2026-10-21-rust-launcher-parity.md`.
- Código reaproveitado: `launcher-rust/src/ui/widgets/folder.rs`, `widgets/select.rs`,
  `pages/performance.rs` (`gpu_pairs`, `rows`), `pages/game.rs` (`browse_prompt`,
  `folder_chosen`, `game_changed`), `play.rs` (`describe_missing`), `persist.rs` (`save`,
  `rebuild`, `collect_unless_unreadable`), `splash.rs` (`SPLASH_BG`, `layers`).
