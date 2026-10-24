# Mods de arquivos

[English](MODS.md) | **Português** | [Español](MODS.es.md)

Mods trocam texturas, modelos, sons, textos e vídeos do jogo sem alterar a pasta do jogo. O
launcher monta, a cada início, uma visão do jogo com os arquivos dos mods por cima e a remove
quando o jogo fecha.

**Índice:** [Página Mods](#a-página-mods-do-launcher) ·
[O que dá para mudar](#o-que-dá-para-mudar-arquivo-por-arquivo) · [Exemplos](#exemplos) ·
[Problemas comuns](#problemas-comuns) · [Direitos autorais](#compartilhar-mods-e-direitos-autorais) · [Como montar um mod](#como-montar-um-mod) ·
[Onde os mods entram](#onde-os-mods-entram) ·
[Mods com patches de jogo](#mods-com-patches-de-jogo) ·
[Patches do jogo](PATCHES.pt-BR.md)

## A página Mods do launcher

![A página Mods com três mods de exemplo: dois trocam o R_UI.WAD, a lista de conflitos está aberta e o painel mostra o mod selecionado](images/mods-page.pt-BR.png)

*Três mods de exemplo. "Kratos Gold Armor" e "Clean HUD" trocam o `R_UI.WAD`: o aviso abaixo da
lista mostra o arquivo, e "Clean HUD" vence por estar mais abaixo.*

Os nomes abaixo são os do launcher em português, com o nome em inglês entre parênteses.

- **Carregar mods** (*Load mods*): chave geral. Desligada, nenhum mod nem patch de mod entra.
- **Ordem de carga** (*Load order*): a lista de mods, com uma caixa para ativar cada um e setas para
  mudar a ordem. **O mod mais abaixo na lista vence** quando dois mods trocam o mesmo arquivo. O
  contador mostra quantos estão ativos ("3 de 5 ativos").
- **Conflitos**: quando mais de um mod ativo troca o mesmo arquivo, aparece um aviso abaixo da
  lista ("⚠ 2 arquivos são alterados por mais de um mod. Vence o mais abaixo na lista.") e um ⚠ ao
  lado de cada mod envolvido. **Detalhes** (*Details*) abre a lista de arquivos, com os mods na
  ordem de carga (`EXEC/WAD/PS4_LE/R_UI.WAD: A > B`, B vence). Dois mods nunca são mesclados num
  mesmo arquivo: o vencedor entra inteiro.
- **Painel do mod**: clicar no nome de um mod abre um painel com imagem, nome, autor, versão,
  descrição, quantos arquivos ele troca, se traz patches de jogo e os conflitos dele. **Abrir pasta
  do mod** e **Site** abrem a pasta e o site (quando o mod informa um). Clicar de novo fecha o painel.
- **Presets**: guardam a ordem e quais mods estão desligados. Digite um nome e clique em **Salvar**;
  **Carregar** aplica um preset salvo e **Excluir** apaga. Ao carregar, mods que não existem mais
  são ignorados e mods novos entram no fim, ativados. Os presets ficam em `mods_presets.json` no
  diretório de dados.
- **Patches de terceiros** e **Pastas** (pasta de mods e pasta de patches) ficam na mesma página.
- **Atualizar** (*Refresh*) relê a pasta de mods depois de copiar um mod novo com o launcher aberto.

As mudanças valem no próximo início do jogo.

## O que dá para mudar, arquivo por arquivo

Um mod troca **arquivos inteiros**. O carregador nunca edita dentro de um arquivo: para mudar uma
textura de um WAD, o mod traz o WAD inteiro com essa textura alterada. A tabela mostra o que a pasta
do jogo tem (contagens do CUSA01623 01.02, inglês dos EUA) e o que cada tipo de arquivo exige. Os
formatos vêm dos próprios arquivos (cabeçalhos `XVAG` e `GNF`, H.264 nos vídeos `.BSF`); o que cada
pasta tem vem dos nomes das pastas e dos arquivos, e o tempo das legendas nos `.TXT` das
cinemáticas vem dos números que eles têm. Nem todo tipo de arquivo foi trocado no jogo.

| Pasta do jogo | O que tem | Formato | Um mod pode trocar? |
|---|---|---|---|
| `EXEC/WAD/PS4_LE/` | Quase todo o jogo: modelos, texturas, fases, menus, HUD e textos (410 `.WAD`) | Pacote WAD, little-endian | **Sim.** Traga o WAD inteiro editado, com o mesmo nome |
| `EXEC/SOUNDS/STREAMS_PS4/` | Música e efeitos sonoros (945 arquivos) | `.XVAG` (contêiner de áudio da Sony) | **Sim**, como um `.XVAG` válido, no mesmo formato do original |
| `<IDIOMA>/SOUNDS/STREAMS_PS4/` | Vozes e diálogos, uma pasta por idioma (cerca de 1.200 arquivos cada) | `.XVAG` | **Sim**, por idioma: `ENGLISH/` não muda `PORTUGUESE/` |
| `EXEC/CINEMATICS/` | Cinemáticas pré-renderizadas (48 `.BSF`) e o tempo das legendas (35 `.TXT`) | `.BSF` = vídeo H.264 puro; `.TXT` = número da linha, primeiro quadro, último quadro | **Sim.** Mantenha a resolução e a taxa de quadros do original; o `.TXT` conta em quadros do vídeo |
| `EXEC/INTRO/` | Logos e imagens da abertura (18 arquivos) | `.GNF` (textura do PS4) | **Sim**, como `.GNF` |
| `EXEC/photomode/` | Imagens do modo foto (15 arquivos) | `.gnf` | **Sim**, como `.gnf` |
| `EXEC/ICONS/` | Imagens da lista de saves (`ICON0.PNG`, `ICON_AUTO.PNG`, `NEWSAVE.PNG`, ...) | `.PNG` | **Sim**, como PNG |
| `EXEC/DLC/` | WADs e listas do conteúdo para download | `.WAD`, `.TXT` | **Sim**, como os outros WADs |
| `eboot.bin`, `sce_module/`, `sce_sys/` | O programa, as bibliotecas e os dados do pacote | Executáveis do PS4 | **Não.** O carregador recusa; mudanças de código usam [patches de jogo](#mods-com-patches-de-jogo) |

### Qual WAD tem o quê

Os nomes dos WADs seguem um padrão. Alguns grupos em `EXEC/WAD/PS4_LE/`:

| Arquivos | O que têm |
|---|---|
| `R_UI.WAD` | Interface: menus, HUD, ícones de botão e o script dos menus |
| `R_PERM.WAD` | Pelo nome, recursos mantidos o jogo todo; tem mais uma cópia do botão de QTE |
| `SHELL.WAD` | O menu principal |
| `R_LANG_EN.WAD`, `R_LANG_PT.WAD`, `R_LANG_BR.WAD`, ... (16 arquivos) | Textos de menus e legendas, um por idioma |
| `R_HERO00.WAD` a `R_HERO08.WAD` | Pelo nome, o herói (Kratos) |
| `R_W_CESTUS*`, `R_W_ELECTRIC*`, `R_W_EXPLOSIVE*`, `R_W_GODMODE*`, `R_W_SOUL*` | Pelo nome, armas e seus níveis de melhoria |
| `R_ZEUS*`, `R_HADE*`, `R_POSH*`, `R_CYCL*`, `R_GRUN*`, `R_SOUL*`, ... | Pelo nome, personagens e inimigos |
| `OLY*`, `HDS*`, `POS*`, `LAB*`, `CRN*`, `BAS*`, `CAV*`, `ICA*`, `GAZ*`, ... | Pelo nome, fases e áreas, em partes (`BAS100.WAD`, `BAS125.WAD`, ...) |
| `CHALLE*` | Pelo nome, as fases de desafio |

"Pelo nome" quer dizer que o grupo foi identificado só pelo nome dos arquivos; confira dentro do
WAD antes de contar com isso.

### Dentro de um WAD

Um WAD é uma lista de entradas, cada uma com um cabeçalho de 64 bytes (tag, flags, tamanho e um
nome de até 56 caracteres) e os dados, alinhados a 16 bytes. As texturas são entradas chamadas
`TX<hash>.<hash>.dds` com um prefixo curto seguido de um arquivo DDS comum (DXT1 ou DXT5), e os
materiais as acham por esse nome. Um mod de textura que mantém o tamanho e o formato DXT do original
é o tipo mais seguro.

Os WAD de fase (`OLY150.WAD` e parecidos, não `R_UI.WAD` nem `R_PERM.WAD`) começam com uma entrada
`EntityCount` (tag 0) cujo campo de tamanho guarda uma contagem de entidades, não um comprimento, e
depois do cabeçalho não vêm dados. Um leitor que pula `size` bytes ali cai no meio do arquivo e não
acha nada depois: até a versão 2.5.0 o próprio leitor do port fazia isso, então ele mudava texturas no
`R_UI.WAD` e no `R_PERM.WAD`, mas em nenhuma fase. `scripts/glyphs.py` (`entries()`) e
`tools/wad_textures.py` agora leem esse marcador como um cabeçalho sozinho, e qualquer textura de um
WAD de fase pode ser achada e mudada do mesmo jeito que as da interface.

O port muda texturas assim: os ícones de botão (Xbox, Nintendo, teclado) são desenhados em cópias do
`R_UI.WAD`, do `R_PERM.WAD` e do `OLY150.WAD` (a lira das Musas na Câmara da Chama), mantendo o
tamanho e o formato de cada textura, de modo que só os pixels mudam (`scripts/glyphs.py`). Numa
textura de fase só os blocos de 4 × 4 cujos pixels mudaram são codificados de novo, em cada nível de
mip, e o resto da textura fica byte a byte igual ao original.

### Achar uma textura do mundo

Algo pintado numa superfície do mundo costuma estar pintado em várias texturas ao mesmo tempo, uma
por camada do material, no mesmo lugar de cada uma: a cor (diffuse), o brilho (specular, em tons de
cinza), a luz que emite (uma máscara de brilho, muitas vezes menor), o relevo (um normal map: DXT5, X
no alfa e Y no verde) e às vezes um decal desenhado por cima. Mude uma e deixe outra, e o desenho
antigo continua aparecendo por baixo ou por cima. As quatro letras gregas da lira estão em cinco
texturas do `OLY150.WAD`; as coloridas que ainda apareciam sobre as letras novas vinham do decal.

Para achar todas, ache o WAD da sala (`--grep` com palavras da sala: `--grep Muse Lyre`) e monte
folhas só com a parte de cada textura onde está o desenho, num tamanho legível:
`--crop 0.75,0,1,0.25` pega o quarto superior direito (esquerda, topo, direita, base, em frações).
Toda textura que mostra as mesmas formas ali é uma camada a mudar.

`tools/wad_textures.py` acha uma textura olhando: monta folhas de contato numeradas com todas as
texturas dos WAD que você nomear (`python tools/wad_textures.py "<jogo>\EXEC\WAD\PS4_LE" OLY1* -o sheets`),
monta folhas com uma parte de cada textura (`--crop`), extrai as numeradas como `.dds` (`--extract`),
procura palavras nos nomes de entrada de todos os WAD para achar o WAD de uma sala (`--grep Muse Lyre`)
e ordena as texturas pequenas que parecem ícones de botão (`--find`). É uma ferramenta de desenvolvimento que fica
no repositório: rode com Python a partir de um checkout do código-fonte.

Os WADs de `PS4_LE` são little-endian. A versão de PS3 rodava num console big-endian, então não se
espera que os WADs dela funcionem aqui.

### O que um mod não faz

- Mudar a lógica do jogo (dano, física, IA, câmera, tempos): fica no executável. Use
  [patches de jogo](#mods-com-patches-de-jogo).
- Trocar `eboot.bin`, `sce_module` ou `sce_sys`: o carregador recusa.
- Injetar DLL ou script.
- Mesclar dois mods no mesmo arquivo: o mais abaixo na lista troca o arquivo inteiro.
- Mudar um arquivo que não existe no seu jogo: um mod feito para outra região (por exemplo, o
  `R_LANG_UK.WAD` europeu) acrescenta um arquivo que ninguém lê, e o log mostra `added` em vez de
  `replaced`.

## Exemplos

**Um mod de textura ou modelo.** O mod traz o WAD editado no caminho do jogo:

```text
mods/
  Kratos Gold Armor/
    mod.json
    preview.png
    EXEC/WAD/PS4_LE/R_HERO00.WAD
```

**Uma fala só em português.** O arquivo vai na pasta do idioma, com o nome do original:

```text
mods/
  Vozes em portugues/
    PORTUGUESE/SOUNDS/STREAMS_PS4/3004CA_ALT1.XVAG
```

**Uma correção de legenda.** O texto de cada idioma fica no seu `R_LANG_*.WAD`. Troque o do idioma
em que você joga e confirme no jogo que é esse o arquivo lido: há mais de um arquivo de português
(`R_LANG_PT.WAD` e `R_LANG_BR.WAD`).

**Uma cinemática com legendas novas.** O vídeo é `EXEC/CINEMATICS/<NOME>.BSF` e o tempo das
legendas é `EXEC/CINEMATICS/<NOME>.TXT`; o texto das legendas fica nos `R_LANG_*.WAD`.

Para achar o caminho exato de um arquivo, procure-o na pasta do jogo: o caminho dentro do mod tem
que ser o mesmo a partir de `EXEC/` ou da pasta de idioma (maiúsculas e minúsculas não importam).

## Problemas comuns

| O que aparece | Por quê | O que fazer |
|---|---|---|
| O mod não aparece na lista | A pasta não tem `EXEC`, pasta de idioma, `dvdroot_ps4` ou `patches/`, ou uma pasta por fora deles também tem outras pastas ou arquivos | Mova os arquivos para que `EXEC/` fique logo dentro da pasta do mod, ou deixe só essa pasta (e readme ou imagens) em cada pasta por fora |
| O log mostra `0 game files replaced` | Os caminhos dentro do mod não batem com os do jogo | Compare com a pasta do jogo, a partir de `EXEC/` |
| O log mostra `added` em vez de `replaced` | O arquivo não existe no seu jogo (outra região ou versão) | Use um mod feito para a sua versão, ou renomeie para o arquivo do seu jogo |
| A mudança de um mod não aparece | Outro mod mais abaixo na lista troca o mesmo arquivo | Abra **Detalhes** abaixo da lista ou mude a ordem |
| O jogo fecha ou mostra modelos quebrados depois de um mod | O arquivo é de outra versão (PS3, outra versão do jogo) ou foi salvo em outro formato | Desative o mod; use arquivos feitos para esta versão, mantendo o formato e o tamanho das texturas do original |
| Um ZIP não foi extraído | Já existe uma pasta com o mesmo nome, ou o zip tem caminhos perigosos | Apague ou renomeie essa pasta, ou extraia o zip você mesmo; o log mostra o motivo |

## Compartilhar mods e direitos autorais

Os arquivos do jogo pertencem aos seus detentores de direitos. Um mod feito como este guia descreve
traz arquivos inteiros do jogo (um WAD, um XVAG, um vídeo) com as suas mudanças dentro, então
compartilhá-lo também compartilha o conteúdo original desses arquivos. Compartilhe só o que você
tem direito de compartilhar: suas próprias texturas, sons e textos, ou arquivos cujo dono permite.
O launcher não baixa nem compartilha mods: ele só coloca os mods da sua pasta de mods por cima da
sua própria cópia do jogo.

## Como montar um mod

A pasta padrão é `mods/` no diretório de dados: no Windows, ao lado do `SpartanPort.exe`; no
Linux (AppImage), em `~/.local/share/spartanport`. **Pastas → Pasta de mods** (*Folders → Mods folder*) escolhe outra.

Cada mod fica na sua própria pasta e espelha a árvore do jogo. Neste jogo os dados ficam em `EXEC/`
e numa pasta por idioma (`ENGLISH/`, `PORTUGUESE/`, ...):

```text
mods/
  Nome do mod/
    EXEC/WAD/PS4_LE/R_UI.WAD
    PORTUGUESE/SOUNDS/...
```

Também são aceitos:

- `Nome do mod/dvdroot_ps4/EXEC/...` (convenção do shadPS4): o `dvdroot_ps4/` é descartado e o
  resto vai para a raiz do jogo;
- `Nome do mod/app0/EXEC/...`;
- pastas por fora de qualquer um desses, como a de um arquivo extraído numa pasta com o nome dele:
  `Nome do mod/Nome do mod v1.2/EXEC/...`. Cada pasta por fora só pode ter essa pasta, mais
  arquivos que descrevem o mod (`.txt`, `.md`, `.json`, `.ini` e imagens), que são ignorados.

Uma pasta sem `EXEC`, pasta de idioma, `dvdroot_ps4` ou `patches/` não aparece na lista. Um ZIP
assim é extraído, mas o log avisa `not a mod (no EXEC, language folder, dvdroot_ps4 or patches)`.

### Pastas reconhecidas

Pastas de idioma sempre reconhecidas: `ENGLISH`, `FRENCH`, `GERMAN`, `ITALIAN`, `SPANISH`,
`PORTUGUESE`, `DUTCH`, `POLISH` e `RUSSIAN`.

Além delas, valem as pastas de dados que existem na raiz do jogo selecionado, como `JAPANESE/`
numa versão asiática. Esse reconhecimento é feito só para o jogo selecionado: uma pasta de outra
instalação não vale aqui, e `eboot.bin`, `sce_module` e `sce_sys` nunca entram. A regra vale
tanto para a lista do launcher quanto para a aplicação dos mods.

Um mod feito para outra região (por exemplo, o `R_LANG_UK.WAD` da versão europeia) não troca nada
num jogo que não tem esse arquivo: o log mostra `added` em vez de `replaced`.

### Nome, autor, imagem e site

O painel do mod lê um `mod.json` opcional na pasta do mod, ao lado de `EXEC/`. Todos os campos são
opcionais e devem ser texto:

```json
{
  "name": "Kratos Gold Armor",
  "author": "Seu nome",
  "version": "1.0",
  "description": "Armadura dourada para o Kratos. Troca o R_HERO00.WAD.",
  "url": "https://example.com/kratos-gold-armor"
}
```

| Campo | Aparece como | Sem ele |
|---|---|---|
| `name` | O nome do mod na lista e no painel | O nome da pasta |
| `author` | "por …" abaixo do nome | Nada |
| `version` | "v…" ao lado do autor | Nada |
| `description` | O texto do painel | Os primeiros 400 caracteres de um `readme.txt` ou `readme.md` |
| `url` | O botão **Site** (só aparece quando o link começa com `http://` ou `https://`) | Sem botão |

**A imagem é um arquivo, não um campo.** Coloque `preview.png` (ou `.jpg`/`.jpeg`) na mesma pasta.
Sem ela, o launcher usa `cover.*`, depois `screenshot.*`, depois qualquer outra imagem da pasta. O
painel mostra a imagem com no máximo 268 pixels de largura (e no máximo 4:3 de altura); uma imagem
16:9 de 1280 × 720 basta. Um `mod.json` que não abre é ignorado, como se não existisse.

A pasta pronta:

```text
mods/
  Kratos Gold Armor/
    mod.json
    preview.png
    readme.txt
    EXEC/WAD/PS4_LE/R_HERO00.WAD
```

### Mods em ZIP

Um mod em **ZIP** pode ser largado direto na pasta de mods: ao abrir o launcher ou iniciar o jogo
ele é extraído numa pasta com o nome do zip, e extraído de novo quando o zip muda. Uma pasta de
mesmo nome que você criou não é substituída. Caminhos perigosos, links e zips que passam de
16 GiB descompactados são recusados. Arquivos 7z/RAR precisam ser extraídos antes.
`tools/zip_mod.py` empacota uma pasta de mod.

### Nomes de arquivo e log

Maiúsculas e minúsculas nos nomes do mod não importam: um arquivo com outra caixa troca o arquivo
do jogo de mesmo nome. Links simbólicos e junctions dentro do mod são recusados.

O log mostra quantos arquivos cada mod tem (`Mods: Nome do mod: 12 files`), qual mod venceu cada
arquivo disputado (`Mods: EXEC/...: A -> B`) e o total: `Mods: 12 game files replaced, 0 added`.
Se o mod deveria trocar arquivos e aparece `0 game files replaced`, confira os caminhos dentro dele.

## Onde os mods entram

Os arquivos do jogo nunca são alterados. A cada início o launcher monta uma visão do jogo numa
pasta `mod-game-*` e a apaga quando o jogo termina. `/app0` fica só para leitura; os saves
continuam na pasta de sempre.

- A visão fica em `out/` no diretório de dados. No Windows sem permissão de link simbólico (Modo
  de Desenvolvedor desligado), se o diretório de dados e o jogo estiverem em discos diferentes, ela
  é criada ao lado da pasta do jogo, porque hard links só funcionam no mesmo disco.
- Com link simbólico disponível, pastas e arquivos do jogo entram como links. Sem ele, no Windows,
  pastas viram junctions e arquivos viram hard links (cópia, quando o arquivo está em outro disco).
- Numa pasta de rede (SMB), que recusa links e junctions, as pastas são espelhadas: pastas reais
  com os arquivos em hard link ou copiados. Os arquivos do jogo e do mod continuam intactos.

A pasta `mod-game-*` aponta para os arquivos do jogo. Se ela sobrar depois de um travamento, não a
apague com ferramentas que seguem links (como `rm -rf` no Git Bash ou no WSL): isso pode apagar os
arquivos do jogo de verdade.

Uma pasta `<pasta do jogo>-mods/EXEC/...` ao lado da pasta do jogo (convenção do shadPS4) entra
antes dos mods da lista. A chave geral também a desliga.

## Ordem e configuração

Ordem e desativações ficam em `mods.json` no diretório de dados:

```json
{"order": ["Primeiro", "Segundo"], "disabled": ["Primeiro"]}
```

Pastas novas entram ativadas, no fim da lista, em ordem alfabética. `SPARTAN_MODS_CONFIG` muda o
caminho desse arquivo.

Os presets da página Mods ficam em `mods_presets.json`, um item por nome, com as mesmas duas listas:

```json
{
  "Só vozes": {"order": ["Vozes em portugues", "Kratos Gold Armor"], "disabled": ["Kratos Gold Armor"]}
}
```

Pelo terminal:

```sh
SPARTAN_GAME_DIR=/caminho/CUSA01623 SPARTAN_MODS_DIR=/caminho/mods bash run.sh
SPARTAN_GAME_DIR=/caminho/CUSA01623 SPARTAN_MODS_ENABLED=0 bash run.sh
```

## Mods com patches de jogo

Um mod também pode trazer **patches de código**: arquivos XML na pasta `patches/` dele, no formato
dos patches embutidos. Um mod pode ter só patches, sem arquivos do jogo:

```text
mods/
  Nome do mod/
    patches/
      meu-patch.xml
```

Eles valem enquanto o mod está ativado (**Carregar mods** desligado também os desliga), e um patch
com o mesmo nome de um embutido o substitui. Um `Resolution Patch - <largura>x<altura>` vira uma
opção de **Render resolution**; o mod [Ultrawide](ultrawide.md) (`extras/mods/Ultrawide`) é o
exemplo. O formato, as regras e como testar um patch estão no
[guia de patches](PATCHES.pt-BR.md#patches-em-mods).

## Ícones e imagem do controle

A opção de ícones em **Controls** já prepara os prompts e a imagem do controle no menu de pausa ao
escolher Xbox ou Nintendo (ou detectar um desses controles em automático). Não precisa instalar um
mod. O estilo PlayStation mantém os arquivos originais; o estilo de teclado mantém o desenho
original do controle. As cópias são criadas no cache local, sem modificar o dump do jogador.

Os desenhos e a ferramenta em `tools/controller_diagrams/` vieram da contribuição de Kristian
Andreev na [PR #234](https://github.com/FelipePrado0/SpartanPort/pull/234), sob GPL-2.0-or-later.
`scripts/controller_diagrams/*.png` contém apenas esses desenhos, sem arquivos do jogo. Para
regenerá-los, a ferramenta usa Pillow, NumPy e scikit-image, com a fonte Cinzel distribuída no
projeto (OFL). Essas dependências de desenho não são necessárias para usar os PNGs no launcher.

# Patches de terceiros

Arquivos de patch do shadPS4 e do GoldHEN vão na pasta `patches/` do diretório de dados e aparecem
na página **Mods** do launcher, em **Patches de terceiros**. Eles só valem para um build do `eboot.bin`: veja o
[guia de patches](PATCHES.pt-BR.md#patches-de-terceiros-shadps4-e-goldhen).
