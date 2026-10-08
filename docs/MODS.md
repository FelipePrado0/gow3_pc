# Mods de arquivos

O launcher tem a página **Mods & patches**: escolha da pasta, chave geral, ativação de cada mod
e setas de ordem de carga. As mudanças valem no próximo início. O mod mais abaixo na lista tem
prioridade; quando dois mods trocam o mesmo arquivo, o log mostra qual venceu.

A pasta padrão é `mods/` no diretório de dados. Cada mod fica na sua própria pasta:

```text
mods/
  Nome do mod/
    dvdroot_ps4/...
```

Também são aceitos:

- `Nome do mod/app0/dvdroot_ps4/...`;
- uma pasta a mais por fora, como a de um arquivo extraído numa pasta com o nome dele:
  `Nome do mod/Nome do mod v1.2/dvdroot_ps4/...` (readme e imagens ao lado são ignorados).

Maiúsculas e minúsculas nos nomes do mod não importam: um arquivo com outra caixa troca o
arquivo do jogo de mesmo nome. O log mostra quantos arquivos o mod trocou e quantos acrescentou:
`Mods: 12 game files replaced, 0 added`. Se o mod deveria trocar arquivos e aparece
`0 game files replaced`, confira os caminhos dentro dele. Arquivos ZIP/7z precisam ser extraídos
antes. Dois mods que trocam o mesmo arquivo não são mesclados: o último vence inteiro.

Uma pasta `<pasta do jogo>-mods/dvdroot_ps4/...` ao lado da pasta do jogo (convenção do shadPS4)
entra antes dos mods da lista. A chave geral também a desliga.

Os arquivos do jogo nunca são alterados. A cada início é montada uma árvore de links em
`out/mod-game-*` no diretório de dados, apagada quando o jogo termina. `/app0` fica só para
leitura; os saves continuam na pasta de sempre.

Mods que trocam `eboot.bin`, `sce_module` ou `sce_sys`, injetores de DLL e plugins de script não
são suportados. Patches de código ficam em `patches/` / `GOW3_PATCHES`. Links simbólicos dentro
do mod são recusados.

Pelo terminal:

```sh
GOW3_GAME_DIR=/caminho/CUSA01623 GOW3_MODS_DIR=/caminho/mods python run.py
GOW3_GAME_DIR=/caminho/CUSA01623 GOW3_MODS_ENABLED=0 python run.py
```

Ordem e desativações ficam em `mods.json` no diretório de dados:

```json
{"order": ["Primeiro", "Segundo"], "disabled": ["Primeiro"]}
```

Pastas novas entram ativadas, no fim da lista, em ordem alfabética. `GOW3_MODS_CONFIG` muda o
caminho desse arquivo.

# Patches de terceiros (XML do shadPS4)

Pasta `patches/` no diretório de dados (no launcher, grupo **Third-party patches**, onde também
dá para escolher outra pasta). Servem arquivos XML no formato shadPS4/GoldHEN: entram os
`Metadata` da versão do jogo (01.02) para `eboot.bin`, e arquivos com `TitleID` de outros jogos
são ignorados. A ativação padrão vem do `isEnabled` do arquivo; a escolha do launcher fica em
`patches.json`:

```json
{"enabled": ["arquivo.xml/Nome do patch"], "disabled": ["arquivo.xml/Outro patch"]}
```

Tipos de linha aceitos: `bytes`, `bytes16/32/64`, `float32/64`, `utf8`, `utf16`. Um patch com
linhas `mask` (busca por padrão) ou com endereço fora do eboot é ignorado inteiro, com aviso no
log. Os patches de terceiros são aplicados depois dos embutidos
(`patches/God_of_War_III_Remastered.xml`): onde os dois escrevem, vence o de terceiros. Pelo
terminal: `GOW3_PATCHES_DIR=/caminho GOW3_PATCHES_CONFIG=/caminho/patches.json python run.py`.
