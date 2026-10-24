<p align="center">
  <img src="packaging/icon/monogram.png" alt="SP" width="160"><br>
  <img src="docs/images/spartanport-logo.png" alt="SpartanPort" width="600">
</p>

# SpartanPort

<div align="center">
  <a href="https://trendshift.io/repositories/299469?utm_source=trendshift-badge&amp;utm_medium=badge&amp;utm_campaign=badge-trendshift-299469" target="_blank" rel="noopener noreferrer"><img src="https://trendshift.io/api/badge/trendshift/repositories/299469/daily?language=C%2B%2B" alt="FelipePrado0%2FSpartanPort | Trendshift" width="250" height="55"/></a>
</div>

[English](https://github.com/FelipePrado0/SpartanPort/blob/main/README.md) | **Português (Brasil)** | [Español](https://github.com/FelipePrado0/SpartanPort/blob/main/README.es.md)

**SpartanPort** é um port não oficial para PC de **God of War III Remastered** (PlayStation 4) para Windows 10/11
e Linux (AppImage experimental).

> **Nenhum arquivo do jogo está incluído.** É preciso um dump do seu próprio PS4 (testados:
> CUSA01623 e CUSA01715, versões 01.00 e 01.02). Este projeto não tem relação com a Sony Interactive Entertainment nem com o
> Santa Monica Studio, nem é endossado por elas. God of War é marca registrada da Sony Interactive Entertainment.

> **Antes gow3_pc.** Atualizar a partir da 2.5.0 pelo launcher mantém seus saves e configurações:
> na primeira abertura, as configurações do jogo (`gow3.ini`), as do launcher e, no Linux, a pasta de
> dados (`~/.local/share/gow3`) são copiadas para os nomes novos. Os arquivos antigos continuam onde
> estavam, e os atalhos existentes continuam funcionando. No Linux, a AppImage agora é
> `spartanport-x86_64.AppImage`: atualize as opções de inicialização do Steam que usam o nome antigo.

## Sumário

- [SpartanPort](#spartanport)
  - [Sumário](#sumário)
  - [Baixar e jogar](#baixar-e-jogar)
  - [Linux (AppImage, experimental)](#linux-appimage-experimental)
  - [O que funciona](#o-que-funciona)
  - [Disponibilidade dos recursos](#disponibilidade-dos-recursos)
  - [Configurações](#configurações)
  - [Teclado](#teclado)
  - [Importar saves do shadPS4](#importar-saves-do-shadps4)
  - [Overlays externos (Windows)](#overlays-externos-windows)
  - [Solução de problemas](#solução-de-problemas)
  - [Desempenho e compatibilidade](#desempenho-e-compatibilidade)
  - [Compilar a partir do código](#compilar-a-partir-do-código)
    - [Windows](#windows)
    - [Linux (Ubuntu 24.04)](#linux-ubuntu-2404)
  - [Testes](#testes)
    - [Python](#python)
    - [Testes C++ no Windows (MSYS2 CLANG64)](#testes-c-no-windows-msys2-clang64)
    - [Testes C/C++ no Linux](#testes-cc-no-linux)
    - [Verificações com o jogo preparado](#verificações-com-o-jogo-preparado)
  - [Como funciona](#como-funciona)
  - [Créditos](#créditos)
  - [Licença](#licença)

## Baixar e jogar

- Windows 10 (1903 ou mais novo) ou Windows 11, 64 bits.
- Processador de 64 bits com AVX: Intel Sandy Bridge (2011) ou mais novo, AMD FX ou mais novo. Em
  CPUs sem MOVBE, BMI1 ou LZCNT (Sandy/Ivy Bridge, AMD FX) o carregador reescreve as instruções do
  jogo que os usam para stubs emulados e registra uma linha `ISA:` no log; o desempenho nessas CPUs
  ainda não foi medido. CPUs mais antigas param na abertura com uma linha `STOP: this CPU lacks`
  no log.
- Placa de vídeo com Vulkan 1.3 e driver atualizado.
- Sua cópia do jogo (a pasta com `eboot.bin`, `sce_module` e `sce_sys`). Testados: CUSA01623 e
  CUSA01715, versões 01.00 e 01.02.

**Passos**

1. Baixe o `spartanport-windows.zip` na
   [página de releases](https://github.com/FelipePrado0/SpartanPort/releases/latest).
2. Extraia numa pasta de sua escolha, por exemplo `C:\Jogos\spartanport-windows`. Não abra direto de
   dentro do zip.
3. Abra o **`SpartanPort.exe`**. Se o Windows mostrar "O Windows protegeu o computador",
   clique em **Mais informações** e depois em **Executar assim mesmo**: o programa não tem
   assinatura digital.
4. Clique em **Choose the game folder**. Isso leva à tela **Game**. Em **Game folder**, clique
   em **Browse…** e escolha a pasta que tem o `eboot.bin`.
5. De volta à **Home**, aparecem a capa e "CUSA01623 · version 01.02". Ajuste os **Quick
   settings** se quiser: modo de tela, limite de FPS, resolução e FPS do motor.
6. Clique em **Save** para guardar alterações pendentes e depois em **PLAY**. A primeira
   abertura prepara a imagem do jogo; a compilação de shaders também pode afetar a abertura e
   o desempenho nas próximas partidas.

Patches e correções já vêm incluídos e são aplicados sozinhos: correção de texturas, pular vídeos
com X e 120 FPS vêm ligados. Mude resolução e FPS na **Home** ou em **Performance**.
A correção da tela preta na abertura é sempre solicitada, mesmo com uma seleção antiga de
patches salva; cada patch só é aplicado onde sua assinatura corresponde. Um patch de resolução
substitui a correção de texturas.

**Durante o jogo**

- **Insert** no teclado, ou **R3 + L2** no controle, abre o menu do port: trapaças,
  multiplicadores, imagem, desempenho e o contador de desempenho.
- Os shaders salvos carregam em segundo plano. Um aviso pequeno "Loading shader cache" no canto
  mostra o progresso.

**Das próximas vezes**

- O **`Play SpartanPort.exe`** abre o jogo direto, com as configurações salvas. Dá para criar
  um atalho na área de trabalho ou adicionar à Steam em **Adicionar um jogo que não é da Steam**.
- Saves, cache de shaders e log ficam na pasta `user`, ao lado do `SpartanPort.exe`. Guarde
  essa pasta ao atualizar.
- **Advanced → Check for updates** baixa e instala a versão nova. Saves e configurações são
  mantidos.

## Linux (AppImage, experimental)

A versão para Linux usa o mesmo runtime nativo e o mesmo launcher. Foi testada no Arch Linux com uma
Radeon RX 7800 XT (RADV) numa sessão Wayland.

**Você precisa de:** Linux x86-64 com glibc 2.39 ou mais nova, CPU com AVX, driver Vulkan 1.3 e
seu próprio dump do jogo. A abertura do launcher é verificada em Ubuntu 24.04, Debian 13,
Fedora 42, Arch e openSUSE Tumbleweed; isso não comprova gameplay em todas as GPUs.

**Baixar e rodar**

1. Baixe `spartanport-x86_64.AppImage` na
   [página de releases](https://github.com/FelipePrado0/SpartanPort/releases/latest).
2. Na pasta do arquivo, execute:
   ```bash
   chmod +x spartanport-x86_64.AppImage
   ./spartanport-x86_64.AppImage
   ```
3. Escolha a pasta do jogo, salve as configurações e pressione **PLAY**.

Se um download antigo mostrar `GLIBC_2.44 not found`, baixe novamente o AppImage recompilado.
O AppImage se atualiza sozinho: **Advanced → Check for updates** (ou o aviso ao abrir o launcher) baixa o AppImage novo do release, confere o SHA-256 e troca o arquivo. Precisa de uma pasta com permissão de escrita; senão abre a página de releases.
A corrupção gráfica no Steam Deck/RADV é um problema conhecido do AppImage 2.2.0 publicado.

- **Arquivos:** saves, configurações, cache de shaders e o registro ficam em `~/.local/share/spartanport`; as
  configurações do próprio launcher, em `~/.config/spartanport-launcher`.
- **Pasta do update:** se o update foi dumpado separado do jogo, escolha-a em **Jogo → Pasta do
  update**. Ela é colocada por cima da pasta do jogo sem alterar nenhuma das duas.
- **Avançado → Linux:** sistema de janelas (automático, Wayland ou X11), GameMode e MangoHud. Só as
  ferramentas instaladas no sistema podem ser ligadas.
- **Steam:** **Avançado → Adicionar à Steam** inclui "SpartanPort" como jogo de fora da
  Steam, com o ícone, as capas vertical e horizontal, o fundo da biblioteca e o logotipo feitos a
  partir das imagens do dump. Feche a Steam antes (ela reescreve os atalhos quando fecha). O botão
  também está no launcher do Windows (ainda sem teste lá).
- **Steam Deck / SteamOS (Gaming Mode):** se a Steam tentar executar o AppImage com Proton, abra
  **Propriedades → Compatibilidade** do atalho e selecione **Steam Linux Runtime** no lugar de Proton.
  O AppImage é um aplicativo nativo de Linux.
- **Entrada no menu:** o AppImage grava `~/.local/share/applications/spartanport.desktop` ao iniciar, para o
  launcher e a janela do jogo mostrarem o ícone no dock; **Avançado → Linux** desliga e remove a entrada.
  Quando outra ferramenta (Gear Lever, AppImageLauncher…) já criou uma entrada que inicia o mesmo
  arquivo, ele não cria a dele e remove a que tiver criado, sem duplicar.

## O que funciona

- Abertura, menus, cutscenes em vídeo (H.264) e gameplay.
- Áudio: música, vozes e efeitos (ATRAC9 e MP3).
- Controle (DualSense, Xbox e outros, via SDL3) e teclado ao mesmo tempo, os dois com botões
  configuráveis.
- Salvar e carregar: a lista de saves aparece na tela, como no PS4. O autosave também funciona.
  O launcher faz backup dos saves antes de cada partida.
- Patches: resolução de renderização de 480p, 720p, 900p, 1440p, 1800p ou 4K, correção de texturas
  corrompidas e pular vídeos com o botão X (comunidade); FPS do motor de 120 (kvicken), 100 (para telas de 100 Hz),
  144 (para telas de 144 Hz) ou 240 (adaptado por Felipe Prado).
- **Ultrawide (beta):** resoluções 21:9, 32:9 e 16:10 (3440x1440 testada) com campo de visão mais largo, em vez de
  uma imagem com zoom, já embutidas: escolha em **Resolução de renderização**, onde cada opção mostra a resolução
  exata e a proporção da tela. As cutscenes mantêm as bordas 16:9 (veja [docs/ultrawide.md](docs/ultrawide.md)).
  Os mods também podem trazer patches de jogo; o [mod Ultrawide](extras/mods/Ultrawide) é o mesmo para versões antigas.
  **Como instalar um mod:** coloque o `.zip` (sem extrair) na pasta `mods`, ao lado do `SpartanPort.exe`; o
  jogo extrai sozinho. O mod Ultrawide e um instalador de um clique para a 2.2.0 estão explicados em
  [extras/mods/Ultrawide](extras/mods/Ultrawide/README.md).
- **Compilação de shaders em segundo plano:** os pipelines salvos carregam e compilam enquanto
  você joga. Ainda podem ocorrer travadas, efeitos ausentes durante a compilação e quedas
  temporárias de FPS; a abertura depende do cache, do driver de vídeo e da CPU.
- **Launcher** com as cores do jogo, idiomas selecionáveis e inglês como fallback: cinco telas
  (**Home, Graphics, Performance, Game, Advanced**) e botão **Save** para alterações pendentes;
  ajustes rápidos na tela inicial, gráficos,
  desempenho, pasta do jogo e backups de save, patches, mods (ordem, presets e conflitos),
  controles com ícones de botões PlayStation ou Xbox, atualizações e diagnóstico.
- **Ícones de botões no jogo** (opt-in, desligado por padrão): os ícones PlayStation do jogo podem
  ser os do seu controle, Xbox ou Nintendo, ou as teclas do teclado com que você joga (tela Game, seção Controls).
- **Menu dentro do jogo** (Insert no teclado, R3+L2 no controle), com quatro abas:
  - **Game:** trapaças (vida, magia, item e Fúria de Esparta infinitos, orbes vermelhos no
    máximo, QTE automático: prompts de botão e de giro do analógico se resolvem sozinhos); multiplicadores de 0,1x a 100x para orbes vermelhos, verdes (vida), azuis (magia)
    e dourados (Fúria de Esparta) e para dano causado e recebido; barra de vida do inimigo.
  - **Image:** modo de tela (janela, sem borda, tela cheia), VSync e nitidez RCAS.
  - **Performance:** limite de FPS (de 30 a ilimitado), quadros na fila, compilação de shaders
    em segundo plano; resolução, FPS do motor e as opções de leitura da GPU valem com um único
    botão "Apply and restart".
  - **Overlay:** contador de desempenho com FPS, tempo de quadro, upscaler, GPU (nome e uso),
    CPU, RAM e VRAM, cada um opcional, com tamanho da fonte, canto, transparência e formato. Os
    números são o uso do próprio jogo, não do sistema inteiro.

## Disponibilidade dos recursos

Disponibilidade conferida na [release 2.2.0](https://github.com/FelipePrado0/SpartanPort/releases/tag/windows-v2.2.0) e no código em 14 de outubro de 2026.

| Recurso | Pacotes 2.2.0 publicados | Código atual |
|---|---|---|
| ZIP Windows / AppImage Linux experimental | Disponíveis | Scripts de compilação disponíveis |
| Correção da tela preta / ícones dos botões do controle | Incluídos | Incluídos |
| Discord Rich Presence | Não incluído | Opcional em Advanced, desligado por padrão; nome do jogo e tempo decorrido |
| Correção da fila esparsa no RADV | Não incluída | Integrada à `main` |
| FSR 1, FSR 3.1, FSR 4, TAA, DLSS | Desativados | Desativados no fluxo de configurações do launcher/jogo |
| Nitidez RCAS | Disponível | Disponível |

O AppImage Linux foi recompilado em 14 de outubro para glibc 2.39 ou mais nova. Os pacotes
Windows e Linux podem usar revisões diferentes do código. Consulte as notas da release para
mudanças em downloads posteriores.

## Configurações

As configurações ficam em `spartanport.ini` (o jogo e o menu do jogo) e em
`settings.json` (o launcher), na pasta do port. Saves, cache de shaders e `last_run.log`
ficam na pasta `user` do projeto.

Em **Performance → GPU data → Stream buffer memory**, escolha **System RAM (cached, default)**,
**System RAM (uncached)** ou **GPU memory (previous behavior)**. A RAM do sistema pode melhorar o
FPS em notebooks com placa de vídeo NVIDIA. A escolha é salva e vale na próxima partida,
inclusive pelo `Play SpartanPort.exe` ou pelo `--play`. Escolha **GPU memory** para voltar à
alocação anterior. Sem o launcher, `SPARTAN_STREAM_MEMORY` aceita `cached`, `host` ou `device`; com
o launcher, a escolha salva nele tem prioridade.

Em **Performance → GPU data → Game multisampling**, **Automatic** decide conforme a placa de
vídeo selecionada: usa uma única amostra nas imagens do jogo com os drivers de Windows da AMD
e da Intel, onde a leitura do buffer de profundidade de duas amostras como textura causava
linhas pretas na iluminação (família RX 6600 e Intel Arc). Nos demais drivers, como NVIDIA,
mantém a multiamostragem. Isso inclui GPUs integradas que usam os dois drivers afetados.

A opção automática mostra o nome da placa e a decisão, acompanhando **Graphics card**. A linha
`GPU: guest multisampling off` ou `on` no registro informa o que foi aplicado. Escolha **Off**
se aparecerem linhas em outra placa, ou **On** para manter a multiamostragem. A alteração vale
na próxima abertura. Sem o launcher, `SPARTAN_NO_MSAA=1` desliga e `0` liga; **Automatic** preserva
essa variável caso ela já exista no ambiente.

## Teclado

Padrão; mude na tela **Controls** do launcher, seção **Keyboard & Mouse Mapping**.

| Tecla | Botão |
|---|---|
| WASD | analógico esquerdo |
| Setas | analógico direito (câmera) |
| Espaço | Xis (Cross) |
| Shift esquerdo | Círculo |
| Clique esquerdo | Quadrado |
| Clique direito | Triângulo |
| Q e E | L1 e R1 |
| R e o botão de avançar do mouse | L2 e R2 |
| Z e X | L3 e R3 |
| F | Rage of Sparta (L3 + R3) |
| C | Agarrar (L1 + Círculo) |
| O botão de voltar do mouse | Esquiva |
| 1, 2, 3, 4 | direcional: cima, direita, baixo, esquerda (armas) |
| Roda do mouse para cima e para baixo | próxima arma e arma anterior (percorre as armas do direcional); nos menus do jogo (título, pausa e Power Up) sobe e desce a seleção, e volta às armas quando você anda ou esquiva |
| Tab | Options |
| P | clique esquerdo do touchpad (modo foto) |
| Esc | clique direito do touchpad (menu de pausa) |
| Insert | menu do port (R3+L2 no controle) |

**Rage of Sparta em uma tecla** (ligado por padrão) deixa Z e X sem uso. Os **Joystick prompts**
respondem aos eventos rápidos de girar o analógico sem analógico: digite o giro (meias-voltas D S A,
A S D, A W D ou D W A; voltas completas D S A W ou A S D W) ou, com *Adaptive mouse*, desenhe-o com
o mouse. Enquanto você joga, o mouse fica preso na janela do jogo; o menu do jogo, a tecla de pausa
(Esc) e outras janelas o liberam, e Esc de novo ou um clique na janela o prende outra vez. Com L2
pressionado (R), o mouse mira com o analógico direito, para a Cabeça de Hélio ou o Arco de Apolo:
*Relative* segue o movimento, *Absolute* aponta a partir de onde o mouse estava ao apertar L2, e
*Right stick sensitivity* define quanto mouse é preciso. Sem o launcher, o jogo usa as próprias
teclas: E e Q para Quadrado e Triângulo, 1 e 3 para L1 e R1, F para R2, C para R3, I, K, J, L para o
direcional, Enter para Options, Tab e Backspace para o touchpad.

Em um controle sem touchpad (Xbox e outros), View abre o menu de pausa (o clique direito do
touchpad) e L1 + View abre o modo foto (o clique esquerdo). No DualSense o touchpad funciona
como no PS4, e Share/Create funciona como View.

Os **ícones de botões no jogo** (desligados por padrão) são escolhidos na tela Controls do launcher, em
*Ícones dos botões no jogo*: *Igual ao meu controle*, *Xbox*, *Nintendo* ou *Teclado*. Eles trocam os ícones
PlayStation dos tutoriais, da lista de golpes, dos eventos rápidos e, nos estilos de controle, da lira das Musas
na Câmara da Chama; o desenho do controle no menu de pausa continua sendo um DualShock 4. *Teclado* mostra a tecla que aperta cada botão, seguindo o seu
mapeamento do teclado (uma tecla trocada aparece no próximo início); *Igual ao meu controle* também mostra
as teclas quando nenhum controle está conectado. O launcher grava uma cópia com patch de até três arquivos do
jogo (cerca de 410 MB) na pasta de dados do port e nunca altera a pasta do jogo. Um controle novo é reconhecido após
reiniciar. Sem o launcher, defina `SPARTAN_GLYPH_STYLE` como `auto`, `xbox`, `nintendo` ou `keyboard` antes de
iniciar o `run.sh`.

No menu: L1/R1 ou Q/E trocam de aba. Na lista de saves: setas ou direcional escolhem, Enter ou
Xis confirmam, Esc ou Círculo cancelam.

## Importar saves do shadPS4

Apenas **saves deste jogo no shadPS4** são compatíveis. Saves do
RPCS3 pertencem ao jogo de PS3 e não funcionam. Use o mesmo identificador do jogo
(este exemplo usa `CUSA01623`).

1. Feche o jogo e o shadPS4. Faça backup das duas pastas de saves.
2. No diretório de dados do shadPS4, procure `user\savedata\1\CUSA01623\` nas versões
   antigas, ou `home\<user-id>\savedata\CUSA01623\` nas versões mais recentes.
3. O destino é **`user\savedata\1\CUSA01623\` ao lado de `SpartanPort.exe`**.
   Exemplo: `C:\Games\spartanport-windows\user\savedata\1\CUSA01623\`. Com uma **Saves folder**
   personalizada no launcher, use `<pasta escolhida>\savedata\1\CUSA01623\`.
4. Copie cada pasta de slot para lá, mantendo o nome e os arquivos do jogo. Não crie
   outra pasta `CUSA01623` dentro dela. Exclua o `sce_sys` interno dos dados copiados.
5. Ao lado de cada slot, crie `<nome do slot>.sce_sys` se não existir. Você pode copiar
   o `sce_sys\icon0.png` do shadPS4 para lá. **Não renomeie `param.sfo` para `param.bin`**:
   os formatos são diferentes. Preserve qualquer `param.bin` existente do port. Sem ele,
   o slot aparece pelo nome da pasta; salvar novamente no port grava os metadados e
   atualiza as informações exibidas.
6. Abra o port e selecione **Carregar jogo**. Saves com o jogo concluído também podem ser importados.

O menu de saves do port exibe no máximo **32 entradas**. Se o slot importado não aparecer
numa lista longa, teste com uma **Saves folder** separada contendo apenas esse slot em
`savedata\1\CUSA01623\`, junto da pasta irmã de metadados. Preserve os saves originais
e restaure a configuração anterior de Saves folder depois do teste.

## Overlays externos (Windows)

O processo do jogo é **`bin\spartanport-probe.exe`**, não `SpartanPort.exe` (builds do código
usam `out\spartanport-probe.exe`). Adicione esse executável ao RTSS e ative a exibição na tela
do Afterburner. No instalador do ReShade, selecione esse executável e a API **Vulkan**.

Se o overlay continuar ausente, informe as versões do port e do overlay, a última versão
do port em que funcionou e envie `user\last_run.log`. Essas verificações não confirmam
uma correção para todas as configurações de overlay.

## Solução de problemas

- Tela preta ao abrir: use a release mais recente, que inclui o patch do deadlock na abertura.
  Se persistir, informe GPU, driver, build do jogo e a saída de **Advanced → Log**. Apagar o
  cache de shaders não é uma solução geral e exige reconstruí-lo na próxima abertura.
- O jogo fecha com "cannot create ... guest memory (error 1450)": o Windows ficou sem memória virtual para alocar o pool de memória do jogo. Aumente o arquivo de paginação: pressione **Win + R**, digite `SystemPropertiesAdvanced`, vá em Configurações de **Desempenho** → guia **Avançado** → **Memória virtual** Alterar, desmarque o gerenciamento automático, selecione a unidade **C:**, defina um tamanho personalizado (ex.: 16384 MB inicial, 24576 MB máximo), clique em **Definir**, **OK** e reinicie o PC.
- O **Idioma do jogo** do launcher não muda nada: ele só vale no primeiro início do jogo. Depois, o
  jogo restaura os próprios idiomas de texto e de falas do save de configurações, então troque no
  jogo: **Opções → Configurações de jogo → Texto de jogo** e **Opções → Áudio → Falas** (por exemplo,
  texto em português com vozes em inglês).
- O jogo não abre: veja **Advanced → Log**, ou envie o `user\last_run.log` junto com o relato.
  Se escolheu uma **Saves folder** personalizada, o `last_run.log` fica nessa pasta.
- Texturas amarelas/laranjas/brancas ou iluminação errada: experimente desligar **Performance →
  GPU data → Deferred GPU readbacks** e reiniciar o jogo. Esse contorno pode causar travadas.
- Falhas de renderização por GPU, crashes, problemas de áudio e bloqueios de progresso seguem
  em investigação. Consulte as [notas da release](https://github.com/FelipePrado0/SpartanPort/releases/latest)
  e as [issues](https://github.com/FelipePrado0/SpartanPort/issues) antes de relatar; quando relevante,
  inclua um save anterior ao problema.

## Desempenho e compatibilidade

Testes anteriores numa AMD Radeon RX 6700 XT, Ryzen 5 5600X e 32 GB registraram 144 FPS de
média (55 a 240) em 1080p com o motor a 240 FPS. São resultados de referência, não um
benchmark de cada release nem garantia de desempenho em outras máquinas.

**Testado e jogável do início ao fim** em 1080p nativo, com o motor a 240 FPS, na máquina de
teste. Há relatos de texturas corrompidas, crashes e bloqueios de progresso em outras
configurações, inclusive em 1080p. Consulte os problemas conhecidos nas notas da release.

## Compilar a partir do código

### Windows

Instale o [rustup](https://rustup.rs) (stable) e o Visual Studio Build Tools com a carga
"Desenvolvimento para desktop com C++"; o launcher é compilado com o cargo e não precisa de Python.
Os comandos de compilação abaixo usam MSYS2 CLANG64; os comandos do launcher identificados como
PowerShell rodam no Windows.

O desenvolvimento acontece na branch `spartanport`; a `main` recebe as versões já testadas.

1. Instale o MSYS2 (`winget install MSYS2.MSYS2`) e, no terminal **MSYS2 CLANG64**, os pacotes
   listados em [packaging/windows/README.md](https://github.com/FelipePrado0/SpartanPort/blob/main/packaging/windows/README.md).
2. Clone o repositório com os submódulos e compile:

   ```bash
   git clone --recursive https://github.com/FelipePrado0/SpartanPort
   cd SpartanPort
   git checkout spartanport   # development branch; omit to build main
   bash build.sh
   ```

3. Abra o launcher (no PowerShell, na pasta do projeto):

   ```powershell
   cd launcher-rust
   cargo run --release
   ```

4. Em **Game**, escolha a pasta do jogo e os patches. Ajuste **Graphics** e **Performance**,
   clique em **Save** para guardar alterações pendentes e depois em **PLAY**.

Para jogar com as configurações salvas sem abrir a janela do launcher (para um atalho ou a
Steam):

```powershell
cd launcher-rust
cargo build --release
.\target\release\spartanport-launcher.exe --play
```

Ou sem as configurações do launcher, no Git Bash ou num terminal MSYS2:

```bash
SPARTAN_GAME_DIR='D:\caminho\CUSA01623' bash run.sh
```

### Linux (Ubuntu 24.04)

Para um clone novo, execute os comandos abaixo num terminal Linux. O script de dependências
instala pacotes do sistema usando sudo e seleciona GCC 14 como compilador padrão. O launcher
precisa também do [rustup](https://rustup.rs) (stable). Ele é destinado ao Ubuntu 24.04; adapte as
dependências em outras distribuições. Se já clonou, comece na pasta do repositório e pule os
dois primeiros comandos.

```bash
git clone --recursive https://github.com/FelipePrado0/SpartanPort
cd SpartanPort
bash packaging/linux/ci-deps.sh
bash build.sh
cd launcher-rust
cargo run --release
```

Para gerar um AppImage depois de compilar:

```bash
bash packaging/linux/build.sh
./dist/spartanport-x86_64.AppImage
```

O `packaging/linux/build.sh` compila o launcher em Rust sozinho. Compilar numa distribuição
mais nova pode exigir uma glibc mais nova; os builds públicos usam Ubuntu 24.04. Na pasta do
repositório, use `bash run.sh` para iniciar diretamente (ele compila o que falta e executa o exe do
launcher com `--run`), com `SPARTAN_GAME_DIR=/caminho/para/CUSA01623` para indicar a pasta do jogo.
Para as configurações salvas do launcher sem a janela, execute
`launcher-rust/target/release/spartanport-launcher --play`.

### Launcher em Rust

O launcher é escrito em Rust (`launcher-rust/`, iced) e é o único que os pacotes trazem.
Compila com o [rustup](https://rustup.rs) (stable). No Windows, instale
o Visual Studio Build Tools com a carga "Desenvolvimento para desktop com C++"; no Ubuntu,
`sudo apt-get install build-essential pkg-config libwayland-dev libxkbcommon-dev`.

```bash
cd launcher-rust
cargo run --release
```

Aberto a partir de um checkout, ele acha sozinho as ferramentas de `out/`; `SPARTAN_PORT_DIR`
aponta para um port empacotado (com `bin/`). O PLAY inicia o próprio exe do launcher com `--run`,
então nada precisa de Python.

## Testes

Execute na raiz do repositório depois de compilar. Os primeiros testes C++ abaixo não precisam de um dump do jogo.

### Python

O código Python do launcher e do runner está congelado: não é empacotado nem executado pelo port.
Ele fica no repositório como referência de paridade do launcher em Rust. Os testes dele continuam
rodando, com Python 3.10 ou mais novo (no Windows, no PowerShell):

```powershell
python -m unittest discover -s tests -p "test_*.py"
```

### Launcher em Rust

```bash
cd launcher-rust
cargo test
```

Os snapshots das páginas em `launcher-rust/snapshots/` são comparados pixel a pixel só no Windows.
`python -m unittest discover -s tests -p "test_launcher_parity_doc.py"` confere que cada teste do
launcher Python tem sua linha em `docs/specs/2026-10-21-rust-launcher-parity.md`.

### Testes C++ no Windows (MSYS2 CLANG64)

```bash
source ./msys2-env.sh
ninja -C out/gpu perf-overlay-test warmup-inbox-test graphics-settings-test
out/gpu/perf-overlay-test.exe
out/gpu/warmup-inbox-test.exe
out/gpu/graphics-settings-test.exe
```

### Testes C/C++ no Linux

```bash
bash build.sh --test
ninja -C out/gpu perf-overlay-test warmup-inbox-test graphics-settings-test
out/gpu/perf-overlay-test
out/gpu/warmup-inbox-test
out/gpu/graphics-settings-test
```

`bash build.sh --test` executa os testes adicionais do runtime C somente no Linux. No Windows, a compilação termina antes desse bloco; use os alvos Ninja acima.

### Verificações com o jogo preparado

Depois de preparar seu próprio dump, os arquivos `out/eboot.elf` e `out/boot-linked.bin` são
necessários para as verificações abaixo. No MSYS2 CLANG64 do Windows, substitua o caminho de
exemplo pela pasta do seu jogo:

```bash
ninja -C out/gpu stat-multipliers-test
out/gpu/stat-multipliers-test.exe out/eboot.elf
out/spartanport-probe.exe out/boot-linked.bin --app0 "D:/Games/CUSA01623" --check-imports
```

No Linux, use `out/gpu/stat-multipliers-test` e `out/spartanport-probe`, sem `.exe`, e um caminho
Linux para o jogo. `--check-imports` confere os imports do sistema e encerra sem iniciar o
gameplay. Outros alvos estão em [gpu/CMakeLists.txt](https://github.com/FelipePrado0/SpartanPort/blob/main/gpu/CMakeLists.txt).

## Como funciona

A parte da CPU é nativa: o executável original do jogo roda direto no processador do PC (o PS4
usa a mesma arquitetura x86-64), e um runtime escrito para este jogo substitui as bibliotecas de
sistema do PS4, sem emulação de CPU. Os gráficos são traduzidos: rodam no núcleo de vídeo do
[shadPS4](https://github.com/shadps4-emu/shadPS4), que converte os comandos e shaders da GPU do
PS4 para Vulkan, com as correções de renderização do jogo da
[branch `eltutz` de cuesta4](https://github.com/cuesta4/shadPS4/tree/eltutz). Rodar o código do
jogo direto na CPU também é como o shadPS4 funciona; o que este projeto muda é a camada de
sistema e o trabalho específico do jogo (patches, cache de shaders em segundo plano, launcher e
menu no jogo).
É um fork do [port para Windows de Supermedo](https://github.com/Supermedo/bloodborne_pc) do
[bloodborne_pc de deadinside28](https://github.com/deadinside28/bloodborne_pc) (veja os
[Créditos](#créditos)).

- Os patches e os ganchos do menu (trapaças, multiplicadores, barra de vida) acham seus pontos
  por assinatura: os bytes originais em volta de cada um, onde quer que a versão do jogo tenha
  colocado esse código (`tools/make_signatures.py`). Um ponto que não é achado deixa só aquele
  patch ou gancho desligado; o jogo abre sem ele, e o log e o menu dizem o motivo.

| Parte | Onde | O que faz |
|---|---|---|
| Preparação | `launcher-rust/src/core`, `launcher-rust/src/runner` | Lê o `eboot.bin`, aplica as realocações e junta a `libc` e a `libSceFios2` do próprio jogo numa imagem |
| Loader | `src/probe.c` | Carrega a imagem, instala os ganchos conferidos (trapaças, multiplicadores, barra de vida) e inicia o código original do jogo |
| Runtime | `src/runtime_*.c` | Reimplementa as funções de sistema do PS4: memória, threads, arquivos, áudio, vídeo, controle, saves e serviços |
| GPU | `gpu/shadps4/` | O núcleo de vídeo do shadPS4, que traduz os comandos e shaders da GPU do PS4 para Vulkan, com as correções de cuesta4 para este jogo (branch `eltutz`) e as otimizações deste port (cache de shaders em segundo plano, leituras adiadas) |
| Menu do jogo | `gpu/shim/spartanport_overlay.cpp` | Menu e contador de desempenho em Dear ImGui; configurações em `gpu/shim/spartanport_settings.cpp` |
| Patches | `patches/CUSA01623.xml`, `launcher-rust/src/core` | Aplicados na memória ao iniciar, sem alterar os arquivos do jogo |
| Launcher | `launcher-rust/src/ui` | Configurações, patches, mods, controles e o botão de jogar |

O Python de `scripts/` e `launcher/` fica como a referência congelada da qual o código Rust foi portado; ele não é mais distribuído.

Mais documentação: [mods](https://github.com/FelipePrado0/SpartanPort/blob/main/docs/MODS.pt-BR.md), [patches](https://github.com/FelipePrado0/SpartanPort/blob/main/docs/PATCHES.pt-BR.md), [mapa do quadro e histórico de pesquisa de upscalers](https://github.com/FelipePrado0/SpartanPort/blob/main/docs/upscaler.md)
e [trabalho paralelo da GPU](https://github.com/FelipePrado0/SpartanPort/blob/main/docs/parallel_gpu.md).

## Créditos

Este projeto é um fork do port para Windows de Supermedo, que parte do original de deadinside28:

- **deadinside28**: [bloodborne_pc](https://github.com/deadinside28/bloodborne_pc), o port nativo
  original.
- **Supermedo** (Mohammed Albarghouthi): [bloodborne_pc para Windows](https://github.com/Supermedo/bloodborne_pc),
  o fork de onde este projeto vem.
- **cuesta4**: [fork do shadPS4 focado neste jogo](https://github.com/cuesta4/shadPS4/tree/eltutz)
  (branch `eltutz`). A renderização do jogo neste port depende das correções dele: a regravação
  de blends da GNM, o estado de profundidade e stencil e as correções de imagens que
  compartilham memória. cuesta4 também é coautor dos patches de resolução, correção de texturas,
  120 FPS e pular vídeos (abaixo). Para jogar este jogo no shadPS4, este é o build indicado.
- **Equipe do shadPS4**: o núcleo de vídeo do [shadPS4](https://github.com/shadps4-emu/shadPS4),
  que faz toda a tradução de GPU e shaders para Vulkan neste port.
- **[WilkPrintes](https://github.com/WilkPrintes)**: criou o design e o
  [site do SpartanPort](https://spartanport.com/pt). Agradecimento especial por dar uma casa ao projeto.

Patches: kvicken e cuesta4 (resolução, correção de texturas, 120 FPS e pular vídeos, a partir
dos patches de PS3 de illusion0001), publicados para o shadPS4 em
[shadps4-emu/ps4_cheats#145](https://github.com/shadps4-emu/ps4_cheats/pull/145) e adaptados a
este executável. Os patches de 144 e 240 FPS foram adaptados por Felipe Prado a partir do de 120 FPS; o de 100 FPS é
o mesmo patch com 100.
Trapaças: adaptadas dos [cheats GoldHEN](https://github.com/GoldHEN/GoldHEN_Cheat_Repository) de
Celogamez.

Também usados: [LibAtrac9](https://github.com/Thealexbarney/LibAtrac9),
[FFmpeg](https://ffmpeg.org), [SDL3](https://github.com/libsdl-org/SDL),
[Dear ImGui](https://github.com/ocornut/imgui), [sirit](https://github.com/shadps4-emu/sirit),
[magic_enum](https://github.com/Neargye/magic_enum), [miniz](https://github.com/richgel999/miniz),
[xbyak](https://github.com/herumi/xbyak),
[Vulkan Memory Allocator](https://github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator),
[FSR-Vulkan](https://github.com/FireBurn/FSR-Vulkan) e o AMD FidelityFX SDK,
[MSYS2](https://www.msys2.org), [LLVM](https://llvm.org) e [Pillow](https://python-pillow.org).

## Licença

GNU GPL v2 ou posterior ([LICENSE](https://github.com/FelipePrado0/SpartanPort/blob/main/LICENSE)). Componentes de terceiros mantêm suas licenças.
