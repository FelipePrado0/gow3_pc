# God of War III Remastered para PC

[English](README.md) | **Português (Brasil)**

Port nativo de **God of War III Remastered** (PlayStation 4) para Windows 10 e 11.

O executável original do jogo roda direto no processador do PC (o PS4 usa a mesma arquitetura
x86-64). Um runtime escrito para este jogo substitui as bibliotecas de sistema do PS4, e os
gráficos são traduzidos para Vulkan. Não é um emulador genérico: o projeto atende um único jogo.
É um fork do [port para Windows de Supermedo](https://github.com/Supermedo/bloodborne_pc) do
[bloodborne_pc de deadinside28](https://github.com/deadinside28/bloodborne_pc), com correções de
God of War III do [fork do shadPS4 de cuesta4](https://github.com/cuesta4/shadPS4) (veja os
[Créditos](#créditos)).

> **Nenhum arquivo do jogo está incluído.** É preciso um dump do seu próprio PS4 (CUSA01623,
> versão 01.02). Este projeto não tem relação com a Sony Interactive Entertainment nem com o
> Santa Monica Studio. God of War é marca registrada da Sony Interactive Entertainment.

Na máquina de teste (AMD Radeon RX 6700 XT, Ryzen 5 5600X, 32 GB), o jogo roda a 144 FPS de
média (55 a 240) em 1080p com o motor a 240 FPS, e a 108 FPS de média em 4K com FSR 1.

**Testado e jogável do início ao fim** em 1080p nativo, com o motor a 240 FPS. Em outras
resoluções de renderização algumas texturas aparecem quebradas; a correção está em andamento.

O andamento está em
[docs/gow3/ROADMAP.md](docs/gow3/ROADMAP.md).

## O que funciona

- Abertura, menus, cutscenes em vídeo (H.264) e gameplay.
- Áudio: música, vozes e efeitos (ATRAC9 e MP3).
- Controle (DualSense, Xbox e outros, via SDL3) e teclado ao mesmo tempo, os dois com botões
  configuráveis.
- Salvar e carregar: a lista de saves aparece na tela, como no PS4. O autosave também funciona.
  O launcher faz backup dos saves antes de cada partida.
- Patches: resolução de renderização de 480p, 720p, 1440p, 1800p ou 4K, correção de texturas
  corrompidas e pular vídeos com o botão X (comunidade); FPS do motor de 120 (kvicken) ou 240
  (adaptado por Felipe Prado).
- **Abertura imediata:** o cache de shaders salvo carrega em segundo plano enquanto você joga, e
  os shaders novos também são compilados em segundo plano, sem parar o jogo.
- **Launcher** com as cores do jogo, em inglês com traduções parciais: ajustes rápidos na tela inicial, tela,
  desempenho, pasta do jogo e backups de save, patches, mods (ordem, presets e conflitos),
  controles com ícones de botões PlayStation ou Xbox, atualizações e diagnóstico.
- **Menu dentro do jogo** (Insert no teclado, R3+L2 no controle), com quatro abas:
  - **Game:** trapaças (vida, magia, item e Fúria de Esparta infinitos, orbes vermelhos no
    máximo); multiplicadores de 0,1x a 100x para orbes vermelhos, verdes (vida), azuis (magia)
    e dourados (Fúria de Esparta) e para dano causado e recebido; barra de vida do inimigo.
  - **Image:** modo de tela (janela, sem borda, tela cheia), VSync, ampliação FSR 1 e nitidez
    RCAS.
  - **Performance:** limite de FPS (de 30 a ilimitado), quadros na fila, compilação de shaders
    em segundo plano; resolução, FPS do motor e as opções de leitura da GPU valem com um único
    botão "Apply and restart".
  - **Overlay:** contador de desempenho com FPS, tempo de quadro, upscaler, GPU (nome e uso),
    CPU, RAM e VRAM, cada um opcional, com tamanho da fonte, canto, transparência e formato. Os
    números são o uso do próprio jogo, não do sistema inteiro.

## Baixar e jogar

**Você precisa de**
- Windows 10 (1903 ou mais novo) ou Windows 11, 64 bits.
- Uma placa de vídeo com Vulkan 1.3 e driver atualizado.
- Sua própria cópia de God of War III Remastered: **CUSA01623 com a atualização 01.02**, numa
  pasta que contenha `eboot.bin`, `sce_module` e `sce_sys`. Nenhum arquivo do jogo vem neste
  download.

**Passos**
1. Baixe o `gow3-windows.zip` na
   [página de releases](https://github.com/FelipePrado0/gow3_pc/releases/latest).
2. Extraia numa pasta de sua escolha, por exemplo `C:\Jogos\gow3-windows`. Não abra direto de
   dentro do zip.
3. Abra o **`God of War III.exe`**. Se o Windows mostrar "O Windows protegeu o computador",
   clique em **Mais informações** e depois em **Executar assim mesmo**: o programa não tem
   assinatura digital.
4. Clique em **Choose the game folder**. Isso leva à tela **Game**. Em **Game folder**, clique
   em **Browse…** e escolha a pasta que tem o `eboot.bin`.
5. De volta à **Home**, aparecem a capa e "CUSA01623 · version 01.02". Ajuste os **Quick
   settings** se quiser: modo de tela, limite de FPS, resolução e FPS do motor.
6. Clique em **PLAY**. A primeira partida demora um pouco mais, enquanto a imagem do jogo é
   preparada. As próximas abrem rápido.

Patches e correções já vêm incluídos e são aplicados sozinhos: correção de texturas, pular vídeos
com X e 120 FPS vêm ligados. Mude resolução e FPS na **Home** ou em **Performance**.

**Durante o jogo**
- **Insert** no teclado, ou **R3 + L2** no controle, abre o menu do port: trapaças,
  multiplicadores, imagem, desempenho e o contador de desempenho.
- Os shaders salvos carregam em segundo plano. Um aviso pequeno "Loading shader cache" no canto
  mostra o progresso.

**Das próximas vezes**
- O **`Play God of War III.exe`** abre o jogo direto, com as configurações salvas. Dá para criar
  um atalho na área de trabalho ou adicionar à Steam em **Adicionar um jogo que não é da Steam**.
- Saves, cache de shaders e log ficam na pasta `user`, ao lado do `God of War III.exe`. Guarde
  essa pasta ao atualizar.
- **Advanced → Check for updates** baixa e instala a versão nova. Saves e configurações são
  mantidos.

**Se algo der errado**
- Tela preta ao abrir: **Advanced → Clear shader cache** e abra de novo.
- O jogo não abre: veja a tela **Log**, ou envie o `user\last_run.log` junto com o relato.

## Requisitos

- Windows 10 (1903 ou mais novo) ou Windows 11, 64 bits.
- Placa de vídeo com Vulkan 1.3 e driver atualizado.
- Sua cópia do jogo: CUSA01623 com a atualização 01.02 aplicada (a pasta com `eboot.bin`,
  `sce_module` e `sce_sys`).
- Os patches só são aplicados ao `eboot.bin` com que foram conferidos byte a byte (sha256
  `d85c8135d330c3b601bf5dc3f1dd86bd6119fb8a9fce372bed2c9785f9a79299`), e os ganchos do menu
  (trapaças, multiplicadores, barra de vida) só onde o código que eles alteram tem os bytes
  esperados. Fora isso o jogo abre sem eles, e o log e o menu dizem o motivo.
- Para compilar a partir do código: [MSYS2](https://www.msys2.org) e [Python 3](https://www.python.org) com Pillow
  (`pip install pillow`, usado no ícone e na capa do jogo).

## Compilar a partir do código

O desenvolvimento acontece na branch `gow3`; a `main` recebe as versões já testadas.

1. Instale o MSYS2 (`winget install MSYS2.MSYS2`) e, no terminal **MSYS2 CLANG64**, os pacotes
   listados em [packaging/windows/README.md](packaging/windows/README.md).
2. Clone o repositório com os submódulos e compile:

   ```bash
   git clone --recursive https://github.com/FelipePrado0/gow3_pc
   cd gow3_pc
   git checkout gow3   # o trabalho mais recente; pule para compilar a main
   bash build.sh
   ```

3. Abra o launcher (no PowerShell, na pasta do projeto):

   ```powershell
   python launcher/gow3_launcher_win.py
   ```

4. Em **Game**, escolha a pasta do jogo. Ajuste **Display**, **Performance** e **Patches** se
   quiser e clique em **PLAY**.

Para jogar com as configurações salvas sem abrir a janela do launcher (para um atalho ou a
Steam):

```powershell
python launcher/gow3_launcher_win.py --play
```

Ou sem o launcher:

```powershell
$env:GOW3_GAME_DIR = 'D:\caminho\CUSA01623'
python run.py
```

As configurações ficam em `gow3.ini` (o jogo e o menu do jogo) e em
`%APPDATA%\gow3-launcher\settings.json` (o launcher). Saves, cache de shaders e `last_run.log`
ficam na pasta `user` do projeto.

### Teclado

Padrão; mude na tela **Controls** do launcher.

| Tecla | Botão |
|---|---|
| WASD | analógico esquerdo |
| Setas | analógico direito (câmera) |
| Espaço | Xis (Cross) |
| Shift esquerdo | Círculo |
| E | Quadrado |
| Q | Triângulo |
| 1 e 3 | L1 e R1 |
| R e F | L2 e R2 |
| Z e C | L3 e R3 |
| I, K, J, L | direcional |
| Enter | Options |
| Tab | touchpad |
| Insert | menu do port (R3+L2 no controle) |

No menu: L1/R1 ou Q/E trocam de aba. Na lista de saves: setas ou direcional escolhem, Enter ou
Xis confirmam, Esc ou Círculo cancelam.

## Testes

```bash
python -m unittest discover -s tests
bash build.sh --test
ninja -C out/gpu stat-multipliers-test perf-overlay-test warmup-inbox-test graphics-settings-test
out/gpu/stat-multipliers-test.exe out/eboot.elf
out/gpu/perf-overlay-test.exe
out/gpu/warmup-inbox-test.exe
out/gpu/graphics-settings-test.exe
```

Os testes em C++ são alvos do [gpu/CMakeLists.txt](gpu/CMakeLists.txt); os que conferem o
código do jogo recebem o executável preparado (`out/eboot.elf`) como argumento.

Para conferir se todas as funções de sistema que o jogo usa estão implementadas, sem abrir o
jogo:

```bash
out/gow3-probe.exe out/boot-linked.bin --app0 <pasta do jogo> --check-imports
```

## Como funciona

| Parte | Onde | O que faz |
|---|---|---|
| Preparação | `scripts/` | Lê o `eboot.bin`, aplica as realocações e junta a `libc` e a `libSceFios2` do próprio jogo numa imagem |
| Loader | `src/probe.c` | Carrega a imagem, instala os ganchos conferidos (trapaças, multiplicadores, barra de vida) e inicia o código original do jogo |
| Runtime | `src/runtime_*.c` | Reimplementa as funções de sistema do PS4: memória, threads, arquivos, áudio, vídeo, controle, saves e serviços |
| GPU | `gpu/shadps4/` | O núcleo de vídeo do shadPS4, que traduz os comandos da GPU do PS4 para Vulkan, com as otimizações deste port (cache de shaders em segundo plano, leituras adiadas) |
| Menu do jogo | `gpu/shim/gow3_overlay.cpp` | Menu e contador de desempenho em Dear ImGui; configurações em `gpu/shim/gow3_settings.cpp` |
| Patches | `patches/God_of_War_III_Remastered.xml`, `scripts/patches.py` | Aplicados na memória ao iniciar, sem alterar os arquivos do jogo |
| Launcher | `launcher/gow3_launcher_win.py` | Configurações, patches, mods, controles e o botão de jogar |

Mais documentação: [mods](docs/MODS.md), [mapa do quadro e upscaler](docs/upscaler.md),
[trabalho paralelo da GPU](docs/parallel_gpu.md) e o [roadmap](docs/gow3/ROADMAP.md).

## Créditos

Este projeto é um fork do port para Windows de Supermedo, que parte do original de deadinside28:

- **deadinside28**: [bloodborne_pc](https://github.com/deadinside28/bloodborne_pc), o port nativo
  original.
- **Supermedo** (Mohammed Albarghouthi): [bloodborne_pc para Windows](https://github.com/Supermedo/bloodborne_pc),
  o fork de onde este projeto vem.
- **cuesta4**: [fork do shadPS4 focado em God of War III Remastered](https://github.com/cuesta4/shadPS4)
  (branch `eltutz`). Este port usa dele a regravação de blends da GNM, o estado de profundidade e
  stencil e as correções de imagens que compartilham memória.
- **Equipe do shadPS4**: o renderizador do [shadPS4](https://github.com/shadps4-emu/shadPS4).

Patches: kvicken e cuesta4 (resolução, correção de texturas, 120 FPS e pular vídeos, a partir
dos patches de PS3 de illusion0001), publicados para o shadPS4 em
[shadps4-emu/ps4_cheats#145](https://github.com/shadps4-emu/ps4_cheats/pull/145) e adaptados a
este executável. O patch de 240 FPS foi adaptado por Felipe Prado a partir do de 120 FPS.
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

GNU GPL v2 ou posterior ([LICENSE](LICENSE)). Componentes de terceiros mantêm suas licenças.
