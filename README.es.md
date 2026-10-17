# God of War III Remastered for PC 

<div align="center">
  <a href="https://trendshift.io/repositories/299469?utm_source=trendshift-badge&amp;utm_medium=badge&amp;utm_campaign=badge-trendshift-299469" target="_blank" rel="noopener noreferrer"><img src="https://trendshift.io/api/badge/trendshift/repositories/299469/daily?language=C%2B%2B" alt="FelipePrado0%2Fgow3_pc | Trendshift" width="250" height="55"/></a>
</div>

[English](https://github.com/FelipePrado0/gow3_pc/blob/main/README.md) | [Português (Brasil)](https://github.com/FelipePrado0/gow3_pc/blob/main/README.pt-BR.md) | **Español**

Port para PC de **God of War III Remastered** (PlayStation 4) para Windows 10/11 y Linux
(AppImage experimental).

> **No se incluyen archivos del juego.** Necesitás un dump de tu propia PS4 (probados: CUSA01623 y
> CUSA01715, versiones 01.00 y 01.02).
> Este proyecto no está afiliado a Sony Interactive Entertainment ni a Santa Monica Studio.
> God of War es una marca registrada de Sony Interactive Entertainment.

## Índice

- [God of War III Remastered for PC](#god-of-war-iii-remastered-for-pc)
  - [Índice](#índice)
  - [Descargar y jugar](#descargar-y-jugar)
  - [Linux (AppImage, experimental)](#linux-appimage-experimental)
  - [Qué funciona](#qué-funciona)
  - [Disponibilidad de funciones](#disponibilidad-de-funciones)
  - [Configuración](#configuración)
  - [Controles de teclado](#controles-de-teclado)
  - [Importar partidas de shadPS4](#importar-partidas-de-shadps4)
  - [Overlays de terceros (Windows)](#overlays-de-terceros-windows)
  - [Solución de problemas](#solución-de-problemas)
  - [Rendimiento y compatibilidad](#rendimiento-y-compatibilidad)
  - [Compilar desde el código fuente](#compilar-desde-el-código-fuente)
    - [Windows](#windows)
    - [Linux (Ubuntu 24.04)](#linux-ubuntu-2404)
  - [Tests](#tests)
    - [Python](#python)
    - [Tests C++ en Windows (MSYS2 CLANG64)](#tests-c-en-windows-msys2-clang64)
    - [Tests C/C++ en Linux](#tests-cc-en-linux)
    - [Verificaciones con el juego preparado](#verificaciones-con-el-juego-preparado)
  - [Cómo funciona](#cómo-funciona)
  - [Créditos](#créditos)
  - [Licencia](#licencia)

## Descargar y jugar

- Windows 10 (1903 o posterior) o Windows 11, de 64 bits.
- Un procesador de 64 bits con AVX: Intel Sandy Bridge (2011) o posterior, AMD FX o posterior. En
  CPUs sin MOVBE, BMI1 o LZCNT (Sandy/Ivy Bridge, AMD FX) el loader reescribe las instrucciones del
  juego que las usan por stubs emulados y registra una línea `ISA:`; el rendimiento en esas CPUs
  todavía no fue medido. Las CPUs más antiguas se detienen al iniciar con una línea
  `STOP: this CPU lacks` en el log.
- Una placa de video con Vulkan 1.3 y un driver actual.
- Tu copia del juego (la carpeta con `eboot.bin`, `sce_module` y `sce_sys`). Probados: CUSA01623 y
  CUSA01715, versiones 01.00 y 01.02.

**Pasos**

1. Descargá `gow3-windows.zip` desde la
   [página de Releases](https://github.com/FelipePrado0/gow3_pc/releases/latest).
2. Extraelo en una carpeta a tu elección, por ejemplo `C:\Games\gow3-windows`. No lo ejecutes
   desde dentro del zip.
3. Abrí **`God of War III.exe`**. Si Windows muestra "Windows protegió su PC", hacé clic en
   **Más información** y después en **Ejecutar de todas formas**: el programa no está firmado
   digitalmente.
4. Hacé clic en **Choose the game folder**. Te lleva a la página **Game**. Junto a **Game folder**,
   hacé clic en **Browse…** y elegí la carpeta que contiene `eboot.bin`.
5. De vuelta en **Home**, aparecen la portada y "CUSA01623 · version 01.02". Ajustá **Quick
   settings** si querés: modo de pantalla, límite de cuadros, resolución de render y tasa de
   cuadros del motor.
6. Hacé clic en **Save** para guardar los cambios pendientes y después en **PLAY**. El primer
   inicio prepara la imagen del juego; la compilación de shaders también puede afectar el
   inicio y el rendimiento en las próximas partidas.

Los parches y correcciones vienen incluidos y se aplican automáticamente: la corrección de
texturas, saltar videos con X y 120 FPS están activados por defecto. Cambiá la resolución y la tasa
de cuadros en **Home** o **Performance**.
La corrección de pantalla negra al iniciar siempre se solicita, incluso con una selección
antigua de parches guardada; cada parche se aplica solo donde coincide su firma. Un parche de
resolución reemplaza la corrección de texturas.

**Mientras jugás**

- **Insert** en el teclado, o **R3 + L2** en el control, abre el menú del port: trucos,
  multiplicadores, imagen, rendimiento y el overlay de rendimiento.
- Los shaders guardados se cargan en segundo plano. Una pequeña nota "Loading shader cache" en la
  esquina muestra el progreso.

**La próxima vez**

- **`Play God of War III.exe`** inicia el juego directamente con tus ajustes guardados. Podés crear
  un acceso directo en el escritorio o agregarlo a Steam con **Agregar un juego que no es de Steam**.
- Tus partidas, la caché de shaders y el log están en la carpeta `user`, junto a
  `God of War III.exe`. Conservá esa carpeta cuando actualices.
- **Advanced → Check for updates** descarga e instala una versión nueva. Tus partidas y ajustes se
  conservan.

## Linux (AppImage, experimental)

La build de Linux ejecuta el mismo runtime nativo y el mismo launcher. Fue probada en Arch Linux con
una Radeon RX 7800 XT (RADV) en una sesión Wayland.

**Necesitás:** Linux x86-64 con glibc 2.39 o posterior, CPU con AVX, driver Vulkan 1.3 y tu
propio dump del juego. El inicio del launcher se verifica en Ubuntu 24.04, Debian 13,
Fedora 42, Arch y openSUSE Tumbleweed; esto no comprueba gameplay en todas las GPUs.

**Descargar y ejecutar**

1. Descargá `gow3-x86_64.AppImage` desde la
   [página de Releases](https://github.com/FelipePrado0/gow3_pc/releases/latest).
2. En la carpeta del archivo, ejecutá:
   ```bash
   chmod +x gow3-x86_64.AppImage
   ./gow3-x86_64.AppImage
   ```
3. Elegí la carpeta del juego, guardá los ajustes y presioná **PLAY**.

Si una descarga anterior muestra `GLIBC_2.44 not found`, descargá de nuevo el AppImage recompilado.
El actualizador de Windows no está disponible en Linux; descargá un AppImage nuevo para actualizar.
La corrupción gráfica en Steam Deck/RADV es un problema conocido del AppImage 2.2.0 publicado.

- **Archivos:** las partidas, los ajustes, la caché de shaders y el log están en
  `~/.local/share/gow3`; los ajustes propios del launcher en `~/.config/gow3-launcher`.
- **Carpeta de actualización:** si la actualización se volcó por separado del juego, elegila en
  **Game → Update folder**. Se superpone a la carpeta del juego sin modificar ninguna de las dos.
- **Advanced → Linux:** sistema de ventanas (automático, Wayland o X11), GameMode y MangoHud. Solo se
  pueden activar las herramientas instaladas en el sistema.
- **Steam:** **Advanced → Add to Steam** agrega "God of War III Remastered" como juego que no es de
  Steam, con el ícono, las cápsulas vertical y horizontal, el fondo de la biblioteca y el logo hechos
  a partir de las imágenes del dump. Cerrá Steam primero (reescribe sus accesos directos al salir).
  El botón también está en el launcher de Windows (aún no probado ahí).
- **Steam Deck / SteamOS (Gaming Mode):** si Steam intenta ejecutar el AppImage con Proton, abrí
  **Propiedades → Compatibilidad** del acceso directo y seleccioná **Steam Linux Runtime** en lugar de Proton.
  El AppImage es una aplicación nativa de Linux.
- **Entrada de menú:** el AppImage escribe `~/.local/share/applications/gow3.desktop` al iniciar,
  para que el launcher y la ventana del juego muestren su ícono en el dock; **Advanced → Linux** lo
  desactiva y elimina la entrada.

## Qué funciona

- Intro, menús, cinemáticas en video (H.264) y gameplay.
- Audio: música, voces y efectos (ATRAC9 y MP3).
- Control (DualSense, Xbox y otros, mediante SDL3) y teclado al mismo tiempo, ambos remapeables.
- **Íconos de botones en el juego** (opcional, desactivado por defecto): los íconos de botones de
  PlayStation dentro del juego pueden mostrar los de tu control, Xbox o Nintendo (página Game, sección Controls).
- Guardado y carga: la lista de partidas guardadas se muestra en pantalla, como en la PS4. El
  autoguardado también funciona. El launcher hace una copia de seguridad de las partidas antes de
  cada inicio.
- Parches: resolución de render de 480p, 720p, 1440p, 1800p o 4K, corrección de texturas corruptas
  y saltar videos con el botón X (comunidad); tasa de cuadros del motor de 120 FPS (kvicken), 100 FPS (para pantallas de 100 Hz) o 240
  FPS (adaptado por Felipe Prado).
- **Ultrawide (beta):** resoluciones 21:9, 32:9 y 16:10 (3440x1440 probada) con un campo de visión más ancho en
  lugar de una imagen con zoom, ya incluidas: elige una en **Resolución de render**, donde cada opción muestra la
  resolución exacta y la relación de aspecto. Las escenas de vídeo mantienen sus bordes 16:9 (ver
  [docs/ultrawide.md](docs/ultrawide.md)). Los mods también pueden incluir parches del juego; el
  [mod Ultrawide](extras/mods/Ultrawide) es lo mismo para versiones anteriores.
  **Cómo instalar un mod:** pon su `.zip` (sin descomprimir) en la carpeta `mods`, junto a `God of War III.exe`; el
  juego lo descomprime solo. El mod Ultrawide y un instalador de un clic para la 2.2.0 se explican en
  [extras/mods/Ultrawide](extras/mods/Ultrawide/README.md).
- **Compilación de shaders en segundo plano:** los pipelines guardados se cargan y compilan
  mientras jugás. Todavía puede haber tirones, efectos ausentes durante la compilación y caídas
  temporales de FPS; el inicio depende de la caché, del driver de video y de la CPU.
- **Launcher** con los colores del juego, idiomas seleccionables e inglés como alternativa:
  cinco páginas (**Home, Graphics, Performance, Game, Advanced**) y botón **Save** para cambios
  pendientes; ajustes rápidos en la página de inicio, gráficos, rendimiento, carpeta del juego
  y copias de las partidas, parches,
  mods (orden, presets y conflictos), controles con íconos de botones de PlayStation o Xbox,
  actualizaciones y diagnóstico.
- **Menú dentro del juego** (Insert en el teclado, R3+L2 en el control), con cuatro pestañas:
  - **Game:** trucos (vida infinita, magia, medidor de objeto, Furia de Esparta, máximo de orbes
    rojos); multiplicadores de 0,1x a 100x para los orbes rojos, verdes (vida), azules (magia) y
    dorados (Furia de Esparta) y para el daño causado y recibido; una barra de vida de enemigos.
  - **Image:** modo de pantalla (ventana, sin bordes, pantalla completa), VSync y nitidez RCAS.
  - **Performance:** límite de cuadros por segundo (30 a ilimitado), cuadros en cola, compilación de
    shaders en segundo plano; la resolución de render, la tasa de cuadros del motor y las opciones
    de lectura de la GPU se aplican con un solo botón "Apply and restart".
  - **Overlay:** un overlay de rendimiento con FPS, tiempo de cuadro, escalador, GPU (nombre y uso),
    CPU, RAM y VRAM, cada uno opcional, con tamaño de fuente, esquina, opacidad del fondo y diseño.
    Los números son el uso del propio juego, no el de todo el sistema.

## Disponibilidad de funciones

Disponibilidad comprobada en la [release 2.2.0](https://github.com/FelipePrado0/gow3_pc/releases/tag/windows-v2.2.0) y en el código el 14 de octubre de 2026.

| Función | Paquetes 2.2.0 publicados | Código actual |
|---|---|---|
| ZIP Windows / AppImage Linux experimental | Disponibles | Scripts de compilación disponibles |
| Corrección de pantalla negra / íconos de botones del control | Incluidos | Incluidos |
| Discord Rich Presence | No incluido | Opcional en Advanced, desactivado por defecto; nombre del juego y tiempo transcurrido |
| Corrección de la cola sparse en RADV | No incluida | Integrada en `main` |
| FSR 1, FSR 3.1, FSR 4, TAA, DLSS | Desactivados | Desactivados en el flujo de ajustes del launcher/juego |
| Nitidez RCAS | Disponible | Disponible |

El AppImage Linux se recompiló el 14 de octubre para glibc 2.39 o posterior. Los paquetes
Windows y Linux pueden usar revisiones distintas del código. Consultá las notas de la release
para los cambios en descargas posteriores.

## Configuración

Los ajustes se guardan en `gow3.ini` (el juego y el menú dentro del juego) y en
`%APPDATA%\gow3-launcher\settings.json` (el launcher). Las partidas, la caché de shaders y
`last_run.log` van a la carpeta `user` del proyecto.

En **Performance → GPU data → Stream buffer memory**, elegí **System RAM (cached, default)**,
**System RAM (uncached)** o **GPU memory (previous behavior)**. La RAM del sistema puede mejorar los
cuadros por segundo en notebooks con NVIDIA. La elección se guarda y se aplica en el próximo inicio
del juego, incluso al usar `Play God of War III.exe` o `--play`. Elegí GPU memory para restaurar el
comportamiento de asignación anterior. Para inicios directos sin el launcher, `GOW3_STREAM_MEMORY`
acepta `cached`, `host` o `device`; la elección guardada por el launcher tiene prioridad cuando se
usa.

En **Performance → GPU data → Game multisampling**, **Automatic** deja que el juego decida según la
placa de video en la que corre: crea las imágenes multimuestreadas del juego con una sola muestra en
los drivers de Windows de AMD e Intel, donde leer el buffer de profundidad de 2 muestras como textura
producía líneas negras en la iluminación (familia Radeon RX 6600, Arc), y las mantiene
multimuestreadas en el resto (NVIDIA). La etiqueta nombra la placa y la decisión, por ejemplo
"Automatic (Intel(R) Arc(TM) A750 Graphics: off)", y sigue la elección de **Graphics card**. La regla
cubre toda GPU con esos dos drivers, incluidas las integradas; la línea del log del juego `GPU: guest
multisampling off` u `on` indica lo que se aplicó. Elegí **Off** si las líneas aparecen en otra placa
y **On** para mantener el multimuestreo de todas formas. Off y On establecen `GOW3_NO_MSAA=1` o `0`;
Automatic deja pasar un `GOW3_NO_MSAA` heredado. Se aplica en el próximo inicio del juego.

## Controles de teclado

Valores por defecto; cambialos en la página **Game** del launcher, sección **Controls**.

| Tecla | Botón |
|---|---|
| WASD | stick izquierdo |
| Flechas | stick derecho (cámara) |
| Espacio | Cruz (X) |
| Shift izquierdo | Círculo |
| E | Cuadrado |
| Q | Triángulo |
| 1 y 3 | L1 y R1 |
| R y F | L2 y R2 |
| Z y C | L3 y R3 |
| I, K, J, L | cruceta |
| Enter | Options |
| Tab | clic del touchpad izquierdo (modo foto) |
| Retroceso | clic del touchpad derecho (menú de pausa) |
| Insert | el menú del port (R3+L2 en el control) |

En un control sin touchpad (Xbox y otros), View abre el menú de pausa (el clic del touchpad derecho)
y L1 + View abre el modo foto (el clic izquierdo). En un DualSense el touchpad funciona como en la
PS4, y Share/Create funciona como View.

Los **íconos de botones en el juego** (desactivados por defecto) se eligen en la página Game del launcher, sección Controls, en
*In-game button icons*: *Match my controller*, *Xbox* o *Nintendo*. Reemplazan los íconos de
PlayStation en los tutoriales, la lista de movimientos y las indicaciones de quick-time events; el
dibujo del control en el menú de pausa sigue siendo un DualShock 4. El launcher escribe una copia
parcheada de dos archivos del juego en la carpeta de datos del port y nunca modifica tu carpeta del
juego. Un control nuevo se detecta después de reiniciar. Sin el launcher, definí `GOW3_GLYPH_STYLE`
como `auto`, `xbox` o `nintendo` antes de iniciar `run.py`.

En el menú: L1/R1 o Q/E cambian de pestaña. En la lista de partidas: las flechas o la cruceta eligen,
Enter o Cruz confirman, Esc o Círculo cancelan.

## Importar partidas de shadPS4

Solo son compatibles las **partidas de shadPS4 de God of War III Remastered**. Las partidas de RPCS3
son del juego de PS3 y no funcionan. Usá el mismo ID de título (este ejemplo usa `CUSA01623`).

1. Cerrá el juego y shadPS4. Hacé una copia de seguridad de ambas carpetas de partidas.
2. Buscá el origen en la carpeta de datos de shadPS4: `user\savedata\1\CUSA01623\` en versiones
   anteriores, o `home\<user-id>\savedata\CUSA01623\` en versiones más nuevas.
3. El destino es **`user\savedata\1\CUSA01623\` junto a `God of War III.exe`**.
   Ejemplo: `C:\Games\gow3-windows\user\savedata\1\CUSA01623\`. Con una **Saves folder**
   personalizada en el launcher, usá `<carpeta personalizada>\savedata\1\CUSA01623\`.
4. Copiá cada carpeta de slot allí, conservando su nombre y los archivos del juego. No anides otra
   carpeta `CUSA01623`. Excluí la carpeta interna `sce_sys` del slot de los datos del juego.
5. Junto a cada slot, creá `<nombre del slot>.sce_sys` si no existe. Podés copiar allí el
   `sce_sys\icon0.png` de shadPS4. **No renombres `param.sfo` a `param.bin`**: los formatos son
   distintos. Conservá cualquier `param.bin` del port que ya exista. Sin uno, el slot aparece por el
   nombre de la carpeta; al guardar de nuevo en el port se escriben sus metadatos y se actualiza la
   información mostrada.
6. Iniciá el port y elegí **Load Game**. También se pueden importar partidas completadas.

El diálogo de partidas del port muestra como máximo **32 entradas**. Si el slot importado no aparece
en una lista larga, probalo con una **Saves folder** separada que contenga solo ese slot en
`savedata\1\CUSA01623\`, con su carpeta de metadatos vecina. Conservá las partidas originales y
restaurá la configuración anterior de Saves folder después de probar.

## Overlays de terceros (Windows)

El proceso del juego es **`bin\gow3-probe.exe`**, no `God of War III.exe` (las builds desde el código
fuente usan `out\gow3-probe.exe`). Agregá ese ejecutable a RTSS y activá la visualización en pantalla
de Afterburner. En el instalador de ReShade, seleccioná ese mismo ejecutable y la API **Vulkan**.

Si el overlay sigue sin aparecer, informá las versiones del port y del overlay, la última versión del
port que funcionaba, y adjuntá `user\last_run.log`. Estas verificaciones no confirman una solución
para todas las configuraciones de overlay.

## Solución de problemas

- Pantalla negra al iniciar: usá la release más reciente, que incluye el parche del deadlock al
  iniciar. Si persiste, informá GPU, driver, build del juego y la salida de **Advanced → Log**.
  Borrar la caché de shaders no es una solución general y requiere reconstruirla al volver a iniciar.
- Error de memoria 1450 ("cannot create ... guest memory"): habilitá o aumentá el archivo de
  paginación de Windows. **Win + R** → `SystemPropertiesAdvanced` → configuración de
  **Rendimiento** → **Opciones avanzadas** → **Memoria virtual**. Podés usar un tamaño
  personalizado en C: (por ejemplo, 16384 MB inicial y 24576 MB máximo); aplicá y reiniciá el PC.
- El juego no abre: revisá **Advanced → Log**, o enviá `user\last_run.log` junto con tu reporte.
  Con una **Saves folder** personalizada, `last_run.log` está en esa carpeta.
- Texturas amarillas/naranjas/blancas o iluminación incorrecta: probá desactivar **Performance →
  GPU data → Deferred GPU readbacks** y reiniciar el juego. Esta alternativa puede causar tirones.
- Los fallos gráficos por GPU, cierres, problemas de audio y bloqueos de progreso siguen bajo
  investigación. Revisá las [notas de la release](https://github.com/FelipePrado0/gow3_pc/releases/latest)
  y los [issues](https://github.com/FelipePrado0/gow3_pc/issues) antes de reportar; cuando corresponda,
  incluí una partida guardada anterior al problema.

## Rendimiento y compatibilidad

Pruebas anteriores en una AMD Radeon RX 6700 XT, Ryzen 5 5600X y 32 GB registraron 144 FPS de
promedio (55 a 240) en 1080p con el motor en 240 FPS. Son resultados de referencia, no un
benchmark de cada release ni una garantía de rendimiento en otros equipos.

**Probado y jugable de principio a fin** en 1080p nativo con el motor en 240 FPS en el equipo de
pruebas. También se reportaron texturas corruptas, cierres y bloqueos de progreso en otras
configuraciones, incluso en 1080p. Consultá los problemas conocidos en las notas de la release.

## Compilar desde el código fuente

### Windows

Instalá Python 3.10 o posterior de python.org, con soporte de Tk. Después instalá las
dependencias del launcher y de la reescritura de instrucciones en PowerShell:

```powershell
python -m pip install pillow iced-x86==1.21.0
```

Los comandos de compilación siguientes usan MSYS2 CLANG64; los comandos del launcher marcados
como PowerShell usan la instalación de Python para Windows.

El desarrollo ocurre en la rama `gow3`; `main` recibe merges ya probados.

1. Instalá MSYS2 (`winget install MSYS2.MSYS2`) y, en la consola **MSYS2 CLANG64**, los paquetes
   listados en [packaging/windows/README.md](https://github.com/FelipePrado0/gow3_pc/blob/main/packaging/windows/README.md).
2. Cloná el repositorio con sus submódulos y compilá:

   ```bash
   git clone --recursive https://github.com/FelipePrado0/gow3_pc
   cd gow3_pc
   git checkout gow3   # development branch; omit to build main
   bash build.sh
   ```

3. Iniciá el launcher (en PowerShell, en la carpeta del proyecto):

   ```powershell
   python launcher/gow3_launcher_win.py
   ```

4. En **Game**, elegí la carpeta del juego y los parches. Ajustá **Graphics** y **Performance**,
   hacé clic en **Save** para guardar cambios pendientes y después en **PLAY**.

Para jugar con los ajustes guardados sin abrir la ventana del launcher (para un acceso directo o
Steam):

```powershell
python launcher/gow3_launcher_win.py --play
```

O sin el launcher:

```powershell
$env:GOW3_GAME_DIR = 'D:\ruta\CUSA01623'
python run.py
```

### Linux (Ubuntu 24.04)

Para un clon nuevo, ejecutá los comandos siguientes en una terminal Linux. El script de
dependencias instala paquetes del sistema con sudo, selecciona GCC 14 como compilador
predeterminado y crea `out/venv` con PyInstaller, Pillow e iced-x86. Está destinado a Ubuntu
24.04; adaptá las dependencias en otras distribuciones. Si ya clonaste, empezá en la carpeta
del repositorio y omití los dos primeros comandos.

```bash
git clone --recursive https://github.com/FelipePrado0/gow3_pc
cd gow3_pc
bash packaging/linux/ci-deps.sh
bash build.sh
out/venv/bin/python launcher/gow3_launcher_win.py
```

Para generar un AppImage después de compilar:

```bash
PATH="$PWD/out/venv/bin:$PATH" bash packaging/linux/build.sh
./dist/gow3-x86_64.AppImage
```

El entorno de compilación se crea antes del comando de empaquetado. Compilar en una distribución
más nueva puede requerir una glibc más nueva; las builds públicas usan Ubuntu 24.04. En la carpeta
del repositorio, usá `out/venv/bin/python launcher/gow3_launcher_win.py --play` para los ajustes
guardados del launcher, o `GOW3_GAME_DIR=/ruta/a/CUSA01623 out/venv/bin/python run.py` para
iniciar directamente.

## Tests

Ejecutá desde la raíz del repositorio después de compilar. Los primeros tests C++ siguientes no necesitan un dump del juego.

### Python

En Windows, usá PowerShell. En Linux, reemplazá `python` por `out/venv/bin/python`.

```powershell
python -m unittest discover -s tests -p "test_*.py"
```

### Tests C++ en Windows (MSYS2 CLANG64)

```bash
source ./msys2-env.sh
ninja -C out/gpu perf-overlay-test warmup-inbox-test graphics-settings-test
out/gpu/perf-overlay-test.exe
out/gpu/warmup-inbox-test.exe
out/gpu/graphics-settings-test.exe
```

### Tests C/C++ en Linux

```bash
bash build.sh --test
ninja -C out/gpu perf-overlay-test warmup-inbox-test graphics-settings-test
out/gpu/perf-overlay-test
out/gpu/warmup-inbox-test
out/gpu/graphics-settings-test
```

`bash build.sh --test` ejecuta los tests adicionales del runtime C solo en Linux. En Windows, la compilación termina antes de ese bloque; usá los targets Ninja anteriores.

### Verificaciones con el juego preparado

Después de preparar tu propio dump, se necesitan `out/eboot.elf` y `out/boot-linked.bin` para
las verificaciones siguientes. En MSYS2 CLANG64 de Windows, reemplazá la ruta de ejemplo por
la carpeta de tu juego:

```bash
ninja -C out/gpu stat-multipliers-test
out/gpu/stat-multipliers-test.exe out/eboot.elf
out/gow3-probe.exe out/boot-linked.bin --app0 "D:/Games/CUSA01623" --check-imports
```

En Linux, usá `out/gpu/stat-multipliers-test` y `out/gow3-probe`, sin `.exe`, y una ruta Linux
para el juego. `--check-imports` verifica los imports del sistema y termina sin iniciar el
gameplay. Hay más targets en [gpu/CMakeLists.txt](https://github.com/FelipePrado0/gow3_pc/blob/main/gpu/CMakeLists.txt).

## Cómo funciona

La parte de la CPU es nativa: el ejecutable original del juego corre directamente en el procesador
de la PC (la PS4 usa la misma arquitectura x86-64), y un runtime escrito para este juego reemplaza
las bibliotecas del sistema de la PS4, sin emulación de CPU. Los gráficos se traducen: corren
sobre el video core de [shadPS4](https://github.com/shadps4-emu/shadPS4), que convierte los
comandos y shaders de la GPU de la PS4 a Vulkan, con las correcciones de renderizado de God of War
III de la [rama `eltutz` de cuesta4](https://github.com/cuesta4/shadPS4/tree/eltutz). Ejecutar el
código del juego directamente en la CPU es también como funciona shadPS4; lo que este proyecto
cambia es la capa del sistema y el trabajo específico del juego (parches, caché de shaders en
segundo plano, launcher y menú dentro del juego).
Es un fork del [port para Windows de Supermedo](https://github.com/Supermedo/bloodborne_pc) de
[bloodborne_pc de deadinside28](https://github.com/deadinside28/bloodborne_pc) (ver
[Créditos](#créditos)).

- Los parches y los hooks dentro del juego (trucos, multiplicadores, barra de vida) encuentran sus
  sitios por firma: los bytes originales alrededor de cada uno, donde sea que una versión haya
  colocado ese código (`tools/make_signatures.py`). Si no se encuentra un sitio, solo ese parche o
  hook queda desactivado; el juego inicia sin él, y el log y el menú explican por qué.

| Parte | Dónde | Qué hace |
|---|---|---|
| Preparación | `scripts/` | Lee `eboot.bin`, aplica sus relocaciones y enlaza las `libc` y `libSceFios2` propias del juego en una sola imagen |
| Loader | `src/probe.c` | Carga la imagen, instala los hooks validados (trucos, multiplicadores, barra de vida) e inicia el código original del juego |
| Runtime | `src/runtime_*.c` | Reimplementa las funciones del sistema de la PS4: memoria, hilos, archivos, audio, video, control, partidas y servicios |
| GPU | `gpu/shadps4/` | El video core de shadPS4, que traduce los comandos y shaders de la GPU de la PS4 a Vulkan, con las correcciones de God of War III de cuesta4 (rama `eltutz`) y el trabajo de velocidad de este port (caché de shaders en segundo plano, lecturas diferidas) |
| Menú dentro del juego | `gpu/shim/gow3_overlay.cpp` | El menú Dear ImGui y el overlay de rendimiento; ajustes en `gpu/shim/gow3_settings.cpp` |
| Parches | `patches/God_of_War_III_Remastered.xml`, `scripts/patches.py` | Se aplican en memoria al iniciar, sin modificar los archivos del juego |
| Launcher | `launcher/gow3_launcher_win.py` | Ajustes, parches, mods, controles y el botón de jugar |

Más documentación: [mods](https://github.com/FelipePrado0/gow3_pc/blob/main/docs/MODS.md), [mapa de cuadros e historial de investigación de escaladores](https://github.com/FelipePrado0/gow3_pc/blob/main/docs/upscaler.md)
y [trabajo paralelo de GPU](https://github.com/FelipePrado0/gow3_pc/blob/main/docs/parallel_gpu.md).

## Créditos

Este proyecto es un fork del port para Windows de Supermedo, que se basa en el original de
deadinside28:

- **deadinside28**: [bloodborne_pc](https://github.com/deadinside28/bloodborne_pc), el port nativo
  original.
- **Supermedo** (Mohammed Albarghouthi): [bloodborne_pc para Windows](https://github.com/Supermedo/bloodborne_pc),
  el fork del que proviene este proyecto.
- **cuesta4**: [fork de shadPS4 enfocado en God of War III Remastered](https://github.com/cuesta4/shadPS4/tree/eltutz)
  (rama `eltutz`). El renderizado del juego en este port depende de sus correcciones: la reescritura
  del blend GNM, el estado de depth/stencil y el aliasing de imágenes. cuesta4 también es coautor de
  los parches de resolución, corrección de texturas, 120 FPS y saltar videos (abajo). Si jugás en
  shadPS4, esta es la build a usar para God of War III.
- **Equipo de shadPS4**: el video core de [shadPS4](https://github.com/shadps4-emu/shadPS4), que
  hace toda la traducción de GPU y shaders a Vulkan en este port.

Parches: kvicken y cuesta4 (resolución, corrección de texturas, 120 FPS y saltar videos, sobre los
parches de PS3 de illusion0001), publicados para shadPS4 en
[shadps4-emu/ps4_cheats#145](https://github.com/shadps4-emu/ps4_cheats/pull/145) y adaptados a este
ejecutable. El parche de 240 FPS fue adaptado por Felipe Prado a partir del de 120 FPS; el de 100 FPS es el mismo
parche con 100.
Trucos: adaptados de los [GoldHEN cheats](https://github.com/GoldHEN/GoldHEN_Cheat_Repository) de
Celogamez.

También se usan: [LibAtrac9](https://github.com/Thealexbarney/LibAtrac9), [FFmpeg](https://ffmpeg.org),
[SDL3](https://github.com/libsdl-org/SDL), [Dear ImGui](https://github.com/ocornut/imgui),
[sirit](https://github.com/shadps4-emu/sirit), [magic_enum](https://github.com/Neargye/magic_enum),
[miniz](https://github.com/richgel999/miniz), [xbyak](https://github.com/herumi/xbyak),
[Vulkan Memory Allocator](https://github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator),
[FSR-Vulkan](https://github.com/FireBurn/FSR-Vulkan) y el AMD FidelityFX SDK,
[MSYS2](https://www.msys2.org), [LLVM](https://llvm.org) y [Pillow](https://python-pillow.org).

## Licencia

GNU GPL v2 o posterior ([LICENSE](https://github.com/FelipePrado0/gow3_pc/blob/main/LICENSE)). Los componentes de terceros conservan sus propias
licencias.
