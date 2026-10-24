# Patches do jogo

[English](PATCHES.md) | **Português** | [Español](PATCHES.es.md)

Patches mudam o código e os dados do jogo **na memória**, quando o jogo abre. Eles corrigem bugs,
mudam a resolução de renderização, aumentam a taxa de quadros e pulam vídeos. A pasta do jogo nunca é
alterada: os bytes do patch são escritos na cópia do programa que o port carrega. Para trocar
texturas, sons ou textos, veja o [guia de mods](MODS.pt-BR.md).

Os nomes do launcher aparecem em português, com o nome em inglês entre parênteses. O menu dentro do
jogo (Insert ou R3 + L2) é em inglês.

**Índice:** [Patches embutidos](#patches-embutidos) · [Onde escolher](#onde-escolher) ·
[O que acontece ao abrir o jogo](#o-que-acontece-ao-abrir-o-jogo) · [Lendo o log](#lendo-o-log) ·
[Quando um patch não entra](#quando-um-patch-não-entra) ·
[Escrevendo um patch](#escrevendo-um-patch) · [Patches em mods](#patches-em-mods) ·
[Patches de terceiros](#patches-de-terceiros-shadps4-e-goldhen) · [Créditos](#créditos)

## Patches embutidos

Os patches embutidos ficam em `patches/CUSA01623.xml`. Eles são para o CUSA01623 e o
CUSA01715, versão 01.02, e acham cada ponto por **assinatura** (os bytes originais em volta do ponto),
então também funcionam em outros builds cujo código bate.

| Patch | O que faz | Quem liga | Autor |
|---|---|---|---|
| **Bug Fix - Startup Black Screen** | Corrige a tela preta ao abrir com o contador de FPS rodando: o índice de ritmo dos quadros começa em 2 em vez de 0 | Sempre aplicado; não aparece no launcher | SpartanPort |
| **Bug Fix - Texture Corruption Fix** | Corrige texturas corrompidas dando ao jogo uma video arena maior (mais memória de vídeo) | Aplicado sempre que a resolução de renderização é **Native**; um patch de resolução o substitui | cuesta4 |
| **Skip Any Video With X Button** | Permite pular qualquer cinemática com o X | Página **Jogo**, ligado por padrão | cuesta4 |
| **Skip Intro** | Pula os logos ao abrir | Página **Jogo**, desligado por padrão | cuesta4 |
| **Resolution Patch - &lt;tamanho&gt;** (21 tamanhos, de 480p a 5120 × 2160) | Define a resolução de renderização e a video arena; os tamanhos ultrawide também alargam o campo de visão (Hor+) | **Resolução de renderização** (página **Desempenho** do launcher, ou o menu do jogo); um por vez | kvicken + cuesta4 (480p, 720p, 1440p, 1800p, 4K); SpartanPort (900p e os tamanhos ultrawide, 16:10 e 4:3) |
| **Frame Rate Patch - 240 FPS** | Sobe o limite de quadros do motor e corrige a swing energy para bater com a simulação original de 60 Hz | Aplicado em toda abertura; a **Taxa de quadros do motor** escolhida (60, 100, 120, 144, 180 ou 240) é escrita ao vivo | kvicken, SpartanPort |
| Frame Rate Patch - 120 FPS / 144 FPS / 100 FPS | O mesmo patch com limite de 120, 144 ou 100 | O launcher não usa mais; ficam no arquivo para outros usos | kvicken / kvicken, SpartanPort |

Observações:

- **Resolução e correção de textura.** Os dois mudam o tamanho da video arena, então o jogo usa um ou
  outro, nunca os dois. Em **Native**, a correção de textura entra sempre, mesmo com o checkbox dela
  desmarcado na página **Jogo**.
- **Memória.** Uma nota de patch como "Requires setting DMEM to 3072 MB" faz o port reservar essa
  memória direta a mais, além dos 5056 MB do jogo original. Nada precisa ser ajustado à mão.
- **Taxa de quadros.** A taxa do motor muda com o jogo rodando (menu do jogo, **R3 + L1 + R1** ou
  **F9**): toda abertura usa o patch de 240 FPS e o port escreve a taxa escolhida no jogo. 144 FPS é
  voltado para telas de 144 Hz; com VSync, ele mantém a apresentação alinhada a essa taxa. Se o patch
  de 240 FPS não for encontrado no seu `eboot.bin`, entra o maior patch de taxa de quadros encontrado;
  se nenhum for encontrado, o motor fica nos 60 FPS originais.

## Onde escolher

Página **Jogo**, **Patches do jogo** (*Game patches*): os patches que dá para ligar e desligar, com
autor e nota. Patches de resolução e de taxa de quadros não aparecem aqui: eles seguem o
**Desempenho**.

![A seção Patches do jogo da página Jogo do launcher](images/game-patches.pt-BR.png)

- **Resolução de renderização** (*Render resolution*): página **Desempenho** do launcher, ou o menu
  do jogo (Insert ou R3 + L2 → **Performance**). Uma resolução nova vale na próxima abertura
  (**Apply and restart** no jogo).
- **Taxa de quadros do motor** (*Engine frame rate*): página **Desempenho** do launcher, ou o menu do
  jogo, onde muda ao vivo.
- **Patches em mods** vêm com o mod enquanto ele está ativado ([Patches em mods](#patches-em-mods)).
- **Patches de terceiros** ficam na página **Mods**
  ([Patches de terceiros](#patches-de-terceiros-shadps4-e-goldhen)).

## O que acontece ao abrir o jogo

1. O launcher monta a lista: as escolhas da página **Jogo**, as correções sempre ligadas, os patches
   de resolução e de taxa de quadros do **Desempenho** e os patches dos mods ativados que vêm ligados
   por padrão.
2. O `scripts/patches.py` procura cada ponto de cada patch no seu `eboot.bin`, pela assinatura. Um
   patch só entra quando **todos** os pontos dele são encontrados; senão ele fica de fora inteiro e o
   log diz por quê.
3. As escritas vão para `out/patches.bin`, e o carregador as aplica no código do jogo na memória antes
   de o jogo rodar. Os patches de terceiros entram depois dos embutidos.
4. A memória e o VBlank que as notas dos patches aplicados pedem são ajustados para essa abertura.

## Lendo o log

O `user/last_run.log` (ou **Avançado → Registro** no launcher) mostra o que entrou:

```text
Patches: CUSA01623 01.02, eboot.bin sha256 d85c8135d330c3b601bf5dc3f1dd86bd6119fb8a9fce372bed2c9785f9a79299
Patches: CUSA01623; 10 writes from ['Bug Fix - Texture Corruption Fix', 'Skip Intro', 'Skip Any Video With X Button', 'Bug Fix - Startup Black Screen', 'Frame Rate Patch - 240 FPS']
Engine frame rate: 120 FPS, live up to 240
```

| Linha | Significado |
|---|---|
| `Patches: <título> <versão>, eboot.bin sha256 …` | O jogo e o build exato do seu `eboot.bin` |
| `Patches: <nome> skipped (signature not found in this eboot.bin)` | O código desse patch não está no seu build; ele ficou de fora |
| `Patches: <nome> is not available for this game build: skipped` | O arquivo de patches não tem um patch com esse nome para esta versão |
| `Patches: using Frame Rate Patch - <n> FPS instead` | O patch de 240 FPS não existe; entra um menor |
| `Patches: <nome> from the mod file <mod>/<arquivo>.xml` | O patch de um mod entrou, ou substituiu um embutido de mesmo nome |
| `Patches: external <arquivo>.xml/<nome> (<n> writes)` | Um patch de terceiros entrou |
| `Patches: skipped <arquivo>.xml/<nome>: <motivo>` | Um patch de terceiros ficou de fora (linha não suportada ou endereço fora do eboot) |
| `Patches: <título>; <n> writes from [...]` | Os patches que entraram |

## Quando um patch não entra

| O que aparece | Por quê | O que fazer |
|---|---|---|
| `skipped (signature not found …)` | Seu `eboot.bin` é um build com código diferente naquele ponto | Nada a ajustar: o jogo abre sem esse patch. Informe o sha256 do log numa issue |
| `signature found more than once` | O padrão não é único no seu build | O autor do patch precisa aumentar o padrão |
| O patch de 240 FPS fica de fora e o motor fica em 60 FPS | Nenhum patch de taxa de quadros bate com o seu build | Informe o sha256 do log |
| Um patch de terceiros não faz nada | Patches de terceiros só valem para o build de referência (abaixo) | Confira a linha do sha256 no log |
| O jogo não abre e o log diz `choose one resolution patch, not [...]` | O `SPARTAN_PATCHES` (terminal) cita dois patches de resolução | Cite só um; o launcher sempre passa o escolhido em **Resolução de renderização** |
| O jogo fecha ao abrir depois de acrescentar um patch | O patch escreve os bytes errados para o seu build | Desligue-o; patches com assinatura são mais seguros que endereços fixos |

## Escrevendo um patch

Os patches usam o formato XML do shadPS4/GoldHEN. Um arquivo tem os title IDs a que se aplica e um
`Metadata` por patch:

```xml
<?xml version="1.0"?>
<Patch>
    <TitleID>
        <ID>CUSA01623</ID>
        <ID>CUSA01715</ID>
    </TitleID>
    <Metadata Title="SpartanPort" Name="Skip Any Video With X Button" Note="Allows skipping any cinematic with Cross." Author="cuesta4" PatchVer="1.0" AppVer="01.02" AppElf="eboot.bin" isEnabled="true">
        <PatchList>
            <Line Type="bytes" Address="0x0041b2a3" Value="7542" Pattern="e8 ?? ?? ?? ?? 31 c9 80 3d ?? ?? ?? ?? 00 75 4a" At="14"/>
            <Line Type="bytes" Address="0x0041b2e7" Value="b901000000909090" Pattern="8b 00 eb 05 e8 ?? ?? ?? ?? 85 c0 0f 95 c0 0f b6 c8" At="9"/>
        </PatchList>
    </Metadata>
</Patch>
```

**Atributos do `Metadata`**

| Atributo | Significado |
|---|---|
| `Name` | O nome do patch, como o launcher e o log mostram. Um nome que começa com `Resolution Patch - <largura>x<altura>` vira uma opção de **Resolução de renderização** |
| `Note` | Aparece abaixo do patch no launcher. "Requires setting DMEM to N MB" e "Requires setting VBlank to N FPS" são lidos pelo port |
| `Author`, `PatchVer` | Aparecem no launcher (autor) e ficam como referência |
| `AppVer` | A versão do jogo a que o patch se aplica: `01.02` |
| `AppElf` | `eboot.bin` (patches para outros arquivos são ignorados) |
| `isEnabled` | `true`: ligado por padrão |

**Atributos do `Line`**

| Atributo | Significado |
|---|---|
| `Type` | `bytes` (hex, espaços permitidos), `bytes16`, `bytes32`, `bytes64` (um número, escrito em little-endian), `float32`, `float64`, `utf8` e `utf16` (texto, com terminador zero acrescentado) |
| `Value` | O que é escrito. Em `bytes`, `{rel:NOME:TAIL}` escreve o deslocamento relativo de 4 bytes até o alvo `NOME` (veja abaixo) |
| `Pattern` | A **assinatura**: os bytes originais em volta do ponto, em hex, com `??` nos bytes que podem mudar entre builds (deslocamentos relativos, destinos de salto). Tem que bater exatamente uma vez no `eboot.bin` |
| `At` | Onde a escrita começa, contando em bytes a partir do início do `Pattern` |
| `Address` | O ponto no build de referência (base `0x400000`). Só é usado quando a linha não tem `Pattern` |
| `Target` | Escreve num alvo com nome em vez do ponto da própria linha: dados para os quais o código aponta, como uma variável que não tem bytes próprios para procurar |

**Alvos.** Um `Target` dá nome a um endereço de que o patch precisa, achado pelo seu próprio
`Pattern`/`At` (ou `Line="N"`: o ponto da linha N, contando a partir de 0). Com `Ref="1"`, os 4 bytes
achados ali são um deslocamento relativo, e o alvo é o endereço para onde ele aponta (`Tail`: quantos
bytes da instrução vêm depois desse deslocamento). O patch de 240 FPS usa isso para apontar uma
instrução para a constante 60,0 do próprio jogo:

```xml
<Line Type="bytes" Address="0x0048ff1a" Value="488d05{rel:sixty:0}" Pattern="c5 fa 11 85 98 fb ff ff 48 8d 05 ?? ?? ?? ?? c5 fa 10 00" At="8"/>
<Target Name="sixty" Address="0x006fc0d8" Pattern="c5 fa 59 85 9c fd ff ff c5 fa 59 05 ?? ?? ?? ?? e8 ?? ?? ?? ?? c5 f9 70 c0 00" At="12" Ref="1" Tail="0"/>
```

**Assinaturas.** O `tools/make_signatures.py` as gera a partir do build de referência. Um bom padrão
é longo o bastante para ser único e troca por `??` todo byte que muda entre builds, como os 4 bytes
depois de um `e8` (call) ou um deslocamento relativo ao rip. Linhas `mask` (`Type="mask"`, a busca por
padrão de alguns arquivos do shadPS4) não são suportadas.

**Testando.** Coloque o arquivo na pasta `patches/` de um mod ([Patches em mods](#patches-em-mods)),
abra o jogo e leia o log: o patch tem que aparecer na linha `writes from [...]` e não numa linha
`skipped`.

## Patches em mods

Um mod pode trazer patches: arquivos XML na pasta `patches/` dele, no formato acima. Um mod pode ter
só patches, sem arquivos do jogo.

```text
mods/
  Meu patch/
    mod.json
    patches/
      meu-patch.xml
```

- Os patches do mod valem enquanto o mod está ativado; **Carregar mods** desligado também os desliga.
- Eles entram junto com os embutidos, na ordem de carga. Um patch com o mesmo nome de um embutido (ou
  de um mod mais acima na lista) o **substitui**.
- Um arquivo cuja lista `TitleID` não tem nenhum dos IDs do jogo (`CUSA01623`, `CUSA01715`) é
  ignorado. Um arquivo sem `TitleID` é aceito.
- `isEnabled="true"` liga o patch junto com o mod. Os patches de resolução e de taxa de quadros são a
  exceção: eles sempre seguem o **Desempenho**. Um `Resolution Patch - <largura>x<altura>` vira uma
  opção de **Resolução de renderização** enquanto o mod está ativado, e entra quando você o escolhe;
  veja o [mod Ultrawide](ultrawide.md).
- O painel do mod mostra "patches de jogo" para mods que trazem patches.

## Patches de terceiros (shadPS4 e GoldHEN)

Arquivos de patch de terceiros vão na pasta `patches/` do diretório de dados (na página **Mods** do
launcher, **Patches de terceiros** (*Third-party patches*) os lista e **Pastas → Pasta de patches**
escolhe outra pasta).

![A seção Patches de terceiros da página Mods do launcher, com dois patches de exemplo](images/third-party-patches.pt-BR.png)

- Cada patch aparece como `<arquivo>.xml/<nome do patch>`. Se ele vem ligado por padrão depende do
  `isEnabled` do arquivo; suas escolhas ficam em `patches.json`:

  ```json
  {"enabled": ["arquivo.xml/Nome do patch"], "disabled": ["arquivo.xml/Outro patch"]}
  ```

- Só entram patches da versão `01.02` para `eboot.bin`; arquivos com `TitleID` de outros jogos são
  ignorados.
- **Eles só valem para o build de referência**, o `eboot.bin` com sha256
  `d85c8135d330c3b601bf5dc3f1dd86bd6119fb8a9fce372bed2c9785f9a79299`, porque usam endereços fixos.
  Com outro build eles não entram; a primeira linha `Patches:` do log mostra o seu sha256.
- Tipos de linha aceitos: `bytes`, `bytes16/32/64`, `float32/64`, `utf8`, `utf16`. Um patch com linha
  `mask` ou com endereço fora do `eboot.bin` fica de fora inteiro, com aviso no log.
- Eles entram depois dos embutidos: onde os dois escrevem, vence o de terceiros.
- Pelo terminal: `SPARTAN_PATCHES_DIR=/caminho SPARTAN_PATCHES_CONFIG=/caminho/patches.json bash run.sh`.

Para usar um patch de terceiros em qualquer build, converta-o para o formato embutido, com
assinaturas, e distribua-o num mod ([Patches em mods](#patches-em-mods)).

## Créditos

- **kvicken** e **cuesta4**: os patches de resolução, correção de textura, 120 FPS e pular vídeos,
  publicados para o shadPS4, a partir dos patches de PS3 de **illusion0001**.
- **SpartanPort** (Felipe Prado): o porte desses patches para este build e para assinaturas, as versões
  de 100, 144 e 240 FPS, a correção da tela preta ao abrir e os tamanhos ultrawide, 16:10 e 4:3
  (`tools/make_ultrawide_patch.py`).
