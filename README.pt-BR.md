# God of War III Remastered para PC

[English](README.md) | **Português (Brasil)**

Port nativo de **God of War III Remastered** (PlayStation 4) para Windows 10 e 11.

O executável original do jogo roda direto no processador do PC (o PS4 usa a mesma arquitetura
x86-64). Um runtime próprio substitui as bibliotecas do sistema do PS4, e os gráficos são
traduzidos para Vulkan. Não é um emulador genérico: o projeto atende a um jogo só.

> **Nenhum arquivo do jogo é incluído.** Você precisa de uma cópia extraída do seu próprio PS4
> (CUSA01623, versão 01.02). Este projeto não tem relação com a Sony Interactive Entertainment
> nem com a Santa Monica Studio. God of War é marca da Sony Interactive Entertainment.

**Situação: alpha em desenvolvimento.** O jogo abre, toca as cutscenes, tem som e controle, e o
gameplay roda a 60 FPS em 1080p numa AMD Radeon RX 6700 XT. Ainda não existe um pacote pronto
para baixar: por enquanto o projeto é compilado a partir do código. O andamento está em
[docs/gow3/ROADMAP.md](docs/gow3/ROADMAP.md).

## O que funciona

- Abertura, menus, cutscenes em vídeo (H.264) e gameplay.
- Áudio: música, vozes e efeitos (ATRAC9 e MP3).
- Controle (DualSense e outros, via SDL3) e teclado ao mesmo tempo.
- Salvar e carregar: a lista de saves aparece na tela, como no PS4. O autosave também funciona.
- Patches da comunidade, escolhidos no launcher antes de jogar:
  - resolução de 480p, 720p, 1440p, 1800p ou 4K;
  - correção de texturas corrompidas;
  - 120 FPS;
  - pular vídeos com o botão X.
- Launcher com a identidade do God of War III (banner e ícone do próprio jogo), em 13 idiomas
  (a página de patches do jogo está em português, inglês e russo).
- Menu de configurações dentro do jogo (Insert ou R3+L2).

## Problemas conhecidos

- **120 FPS:** o jogo limita o tempo de cada quadro a 1/FPS alvo. Abaixo de 120 FPS ele roda em
  câmera lenta (no PS4 acontece o mesmo abaixo de 60). Só vale a pena onde o PC sustenta 120.
- **Skip Intro** não está disponível: o patch da comunidade não corresponde a este executável.
- A primeira abertura de cada área pode engasgar enquanto os shaders são compilados. A abertura
  do jogo pode levar mais de um minuto quando o cache de shaders está grande.
- Sem upscaling temporal (FSR, DLSS ou TAA) por enquanto: o código está no port, mas desligado
  até ser calibrado para o God of War III.
- Testado só no Windows 11 com AMD Radeon RX 6700 XT.

## Requisitos

- Windows 10 (1903 ou mais novo) ou Windows 11, 64 bits.
- Placa de vídeo com Vulkan 1.3 e driver atualizado.
- A sua cópia do jogo: CUSA01623 com o update 01.02 aplicado (a pasta com `eboot.bin`,
  `sce_module` e `sce_sys`).
- Os patches só são aplicados ao `eboot.bin` em que foram conferidos byte a byte (sha256
  `d85c8135d330c3b601bf5dc3f1dd86bd6119fb8a9fce372bed2c9785f9a79299`). Com outro executável o
  jogo abre sem patches, e o log explica o motivo.
- Para compilar: [MSYS2](https://www.msys2.org) e [Python 3](https://www.python.org) com
  Pillow (`pip install pillow`, usado para o ícone do jogo).

## Como compilar e jogar

1. Instale o MSYS2 (`winget install MSYS2.MSYS2`) e, no shell **MSYS2 CLANG64**, os pacotes
   listados em [packaging/windows/README.md](packaging/windows/README.md).
2. Clone o repositório com os submódulos e compile. O God of War III está na branch principal
   (`main`): não é preciso trocar de branch.

   ```bash
   git clone --recursive https://github.com/FelipePrado0/gow3_pc
   cd gow3_pc
   bash build.sh
   ```

3. Abra o launcher (no PowerShell, na pasta do projeto):

   ```powershell
   python launcher/gow3_launcher_win.py
   ```

4. Em **Jogo e efeitos**, escolha a pasta do jogo. Em **Patches do jogo**, escolha a resolução
   e os patches. Clique em **JOGAR**.

Também dá para jogar sem o launcher:

```powershell
$env:GOW3_GAME_DIR = 'D:\caminho\CUSA01623'
python run.py
```

Os saves, o cache de shaders e o log ficam na pasta `user` do projeto.

### Controles no teclado

| Tecla | Botão |
|---|---|
| WASD | analógico esquerdo |
| Setas | analógico direito (câmera) |
| Espaço | Cross (X) |
| Shift esquerdo | Circle |
| E | Square |
| Q | Triangle |
| 1 e 3 | L1 e R1 |
| R e F | L2 e R2 |
| Z e C | L3 e R3 |
| I, K, J, L | direcional |
| Enter | Options |
| Tab | touchpad |
| Insert | menu de configurações do port |

Na lista de saves: setas ou direcional escolhem, Enter ou Cross confirmam, Esc ou Circle cancelam.

## Testes

```bash
python -m unittest discover -s tests
ninja -C out/gpu mp3-test videodec-test services-test
out/gpu/mp3-test.exe tests/data/sine.mp3
out/gpu/videodec-test.exe tests/data/tiny.h264
out/gpu/services-test.exe
```

Para conferir se todas as funções do sistema que o jogo usa estão implementadas, sem abrir o
jogo:

```bash
out/gow3-probe.exe out/boot-linked.bin --app0 <pasta do jogo> --check-imports
```

## Como o projeto funciona

| Parte | Onde | O que faz |
|---|---|---|
| Preparação | `scripts/` | Lê o `eboot.bin`, aplica as relocações e junta a `libc` e a `libSceFios2` do próprio jogo numa imagem única |
| Loader | `src/probe.c` | Carrega a imagem na memória e inicia o código original do jogo |
| Runtime | `src/runtime_*.c` | Reimplementa as funções do sistema do PS4: memória, threads, arquivos, áudio, vídeo, controle, saves e serviços |
| GPU | `gpu/` | Núcleo de vídeo do shadPS4, que traduz os comandos da GPU do PS4 para Vulkan |
| Patches | `patches/God_of_War_III_Remastered.xml`, `scripts/patches.py` | Aplicados na memória ao iniciar, sem alterar os arquivos do jogo |
| Launcher | `launcher/gow3_launcher_win.py` | Configurações, patches e botão de jogar |

Os upscalers temporais (FSR 3/4, DLSS, TAA) e os vetores de movimento herdados do port base
ficam no código, desligados, até serem calibrados para o God of War III (ver o roadmap).

## Créditos

Construído sobre o [bbport](https://github.com/deadinside28/bloodborne_pc), port nativo do
Bloodborne por deadinside28, sobre o [port para Windows](https://github.com/Supermedo/bloodborne_pc)
por Supermedo (Mohammed Albarghouthi), e sobre o renderizador do
[shadPS4](https://github.com/shadps4-emu/shadPS4).

Patches do God of War III Remastered por kvicken e cuesta4 (a partir dos patches do PS3 de
illusion0001), publicados para o shadPS4 em
[shadps4-emu/ps4_cheats#145](https://github.com/shadps4-emu/ps4_cheats/pull/145) e adaptados
para este executável.

Também usados: [LibAtrac9](https://github.com/Thealexbarney/LibAtrac9),
[FFmpeg](https://ffmpeg.org), [SDL3](https://github.com/libsdl-org/SDL),
[Dear ImGui](https://github.com/ocornut/imgui), [sirit](https://github.com/shadps4-emu/sirit),
[magic_enum](https://github.com/Neargye/magic_enum), [miniz](https://github.com/richgel999/miniz),
[xbyak](https://github.com/herumi/xbyak),
[Vulkan Memory Allocator](https://github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator),
[FSR-Vulkan](https://github.com/FireBurn/FSR-Vulkan) e AMD FidelityFX SDK,
[MSYS2](https://www.msys2.org), [LLVM](https://llvm.org) e [Pillow](https://python-pillow.org).

## Licença

GNU GPL v2 ou posterior ([LICENSE](LICENSE)). Os componentes de terceiros mantêm as suas
próprias licenças.
