# Parches del juego

[English](PATCHES.md) | [Português](PATCHES.pt-BR.md) | **Español**

Los parches cambian el código y los datos del juego **en memoria**, al iniciar el juego. Corrigen
errores, cambian la resolución de render, suben la tasa de fotogramas y saltan vídeos. La carpeta del
juego nunca se modifica: los bytes del parche se escriben en la copia del programa que carga el port.
Para cambiar texturas, sonidos o textos, mira la [guía de mods](MODS.es.md).

Los nombres del launcher aparecen en español, con el nombre en inglés entre paréntesis. El menú
dentro del juego (Insert o R3 + L2) está en inglés.

**Índice:** [Parches incluidos](#parches-incluidos) · [Dónde elegirlos](#dónde-elegirlos) ·
[Qué pasa al iniciar el juego](#qué-pasa-al-iniciar-el-juego) · [Leer el registro](#leer-el-registro) ·
[Cuando un parche no se aplica](#cuando-un-parche-no-se-aplica) ·
[Escribir un parche](#escribir-un-parche) · [Parches en mods](#parches-en-mods) ·
[Parches de terceros](#parches-de-terceros-shadps4-y-goldhen) · [Créditos](#créditos)

## Parches incluidos

Los parches incluidos están en `patches/CUSA01623.xml`. Son para CUSA01623 y
CUSA01715, versión 01.02, y encuentran cada punto por **firma** (los bytes originales alrededor del
punto), así que también funcionan en otros builds cuyo código coincide.

| Parche | Qué hace | Quién lo activa | Autor |
|---|---|---|---|
| **Bug Fix - Startup Black Screen** | Corrige la pantalla negra al iniciar con el contador de FPS en marcha: el índice de ritmo de fotogramas empieza en 2 en lugar de 0 | Siempre aplicado; no aparece en el launcher | SpartanPort |
| **Bug Fix - Texture Corruption Fix** | Corrige texturas dañadas dando al juego una video arena más grande (más memoria de vídeo) | Aplicado siempre que la resolución de render es **Native**; un parche de resolución lo sustituye | cuesta4 |
| **Skip Any Video With X Button** | Permite saltar cualquier cinemática con X | Página **Juego**, activado por defecto | cuesta4 |
| **Skip Intro** | Salta los logotipos al iniciar | Página **Juego**, desactivado por defecto | cuesta4 |
| **Resolution Patch - &lt;tamaño&gt;** (21 tamaños, de 480p a 5120 × 2160) | Fija la resolución de render y la video arena; los tamaños ultrawide también amplían el campo de visión (Hor+) | **Resolución de render** (página **Rendimiento** del launcher, o el menú del juego); uno a la vez | kvicken + cuesta4 (480p, 720p, 1440p, 1800p, 4K); SpartanPort (900p y los tamaños ultrawide, 16:10 y 4:3) |
| **Frame Rate Patch - 240 FPS** | Sube el límite de fotogramas del motor y corrige la swing energy para que coincida con la simulación original de 60 Hz | Aplicado en cada inicio; la **Velocidad de fotogramas del motor** elegida (60, 100, 120, 144, 180 o 240) se escribe en vivo | kvicken, SpartanPort |
| Frame Rate Patch - 120 FPS / 144 FPS / 100 FPS | El mismo parche con un límite de 120, 144 o 100 | El launcher ya no los usa; siguen en el archivo para otros usos | kvicken / kvicken, SpartanPort |

Notas:

- **Resolución y corrección de texturas.** Los dos cambian el tamaño de la video arena, así que el
  juego usa uno u otro, nunca los dos. En **Native**, la corrección de texturas siempre se añade,
  aunque su casilla de la página **Juego** esté desmarcada.
- **Memoria.** Una nota de parche como "Requires setting DMEM to 3072 MB" hace que el port reserve
  esa memoria directa de más, además de los 5056 MB del juego original. No hay que ajustar nada a mano.
- **Tasa de fotogramas.** La tasa del motor cambia con el juego en marcha (menú del juego,
  **R3 + L1 + R1** o **F9**): cada inicio usa el parche de 240 FPS y el port escribe la tasa elegida
  en el juego. 144 FPS está pensado para pantallas de 144 Hz; con VSync, mantiene la presentación
  alineada con esa frecuencia. Si el parche de 240 FPS no se encuentra en tu `eboot.bin`, se usa el parche de tasa de
  fotogramas más alto que se encuentre; si no se encuentra ninguno, el motor se queda en los 60 FPS
  originales.

## Dónde elegirlos

Página **Juego**, **Parches del juego** (*Game patches*): los parches que se pueden activar y
desactivar, con su autor y su nota. Los parches de resolución y de tasa de fotogramas no aparecen
aquí: siguen a **Rendimiento**.

![La sección Parches del juego de la página Juego del launcher](images/game-patches.es.png)

- **Resolución de render** (*Render resolution*): página **Rendimiento** del launcher, o el menú del
  juego (Insert o R3 + L2 → **Performance**). Una resolución nueva se aplica en el siguiente inicio
  (**Apply and restart** en el juego).
- **Velocidad de fotogramas del motor** (*Engine frame rate*): página **Rendimiento** del launcher, o
  el menú del juego, donde cambia en vivo.
- **Parches en mods**: llegan con el mod mientras está activado ([Parches en mods](#parches-en-mods)).
- **Parches de terceros**: están en la página **Mods**
  ([Parches de terceros](#parches-de-terceros-shadps4-y-goldhen)).

## Qué pasa al iniciar el juego

1. El launcher arma la lista: lo elegido en la página **Juego**, las correcciones siempre activas, los
   parches de resolución y de tasa de fotogramas de **Rendimiento** y los parches de los mods
   activados que vienen activados por defecto.
2. `scripts/patches.py` busca cada punto de cada parche en tu `eboot.bin`, por su firma. Un parche solo
   se aplica cuando se encuentran **todos** sus puntos; si no, queda fuera entero y el registro dice
   por qué.
3. Las escrituras van a `out/patches.bin`, y el cargador las aplica al código del juego en memoria
   antes de que el juego arranque. Los parches de terceros se aplican después de los incluidos.
4. La memoria y el VBlank que piden las notas de los parches aplicados se ajustan para ese inicio.

## Leer el registro

`user/last_run.log` (o **Avanzado → Registro** en el launcher) muestra qué se aplicó:

```text
Patches: CUSA01623 01.02, eboot.bin sha256 d85c8135d330c3b601bf5dc3f1dd86bd6119fb8a9fce372bed2c9785f9a79299
Patches: CUSA01623; 10 writes from ['Bug Fix - Texture Corruption Fix', 'Skip Intro', 'Skip Any Video With X Button', 'Bug Fix - Startup Black Screen', 'Frame Rate Patch - 240 FPS']
Engine frame rate: 120 FPS, live up to 240
```

| Línea | Significado |
|---|---|
| `Patches: <título> <versión>, eboot.bin sha256 …` | El juego y el build exacto de tu `eboot.bin` |
| `Patches: <nombre> skipped (signature not found in this eboot.bin)` | El código de ese parche no está en tu build; quedó fuera |
| `Patches: <nombre> is not available for this game build: skipped` | El archivo de parches no tiene un parche con ese nombre para esta versión |
| `Patches: using Frame Rate Patch - <n> FPS instead` | Falta el parche de 240 FPS; se usa uno menor |
| `Patches: <nombre> from the mod file <mod>/<archivo>.xml` | Se añadió el parche de un mod, o sustituyó a uno incluido con el mismo nombre |
| `Patches: external <archivo>.xml/<nombre> (<n> writes)` | Se aplicó un parche de terceros |
| `Patches: skipped <archivo>.xml/<nombre>: <motivo>` | Un parche de terceros quedó fuera (línea no admitida o dirección fuera del eboot) |
| `Patches: <título>; <n> writes from [...]` | Los parches que se aplicaron |

## Cuando un parche no se aplica

| Qué ves | Por qué | Qué hacer |
|---|---|---|
| `skipped (signature not found …)` | Tu `eboot.bin` es un build con código distinto en ese punto | Nada que ajustar: el juego arranca sin ese parche. Indica el sha256 del registro en una issue |
| `signature found more than once` | El patrón no es único en tu build | El autor del parche tiene que alargar el patrón |
| El parche de 240 FPS queda fuera y el motor se queda en 60 FPS | Ningún parche de tasa de fotogramas coincide con tu build | Indica el sha256 del registro |
| Un parche de terceros no hace nada | Los parches de terceros solo se aplican al build de referencia (abajo) | Revisa la línea del sha256 en el registro |
| El juego no arranca y el registro dice `choose one resolution patch, not [...]` | `SPARTAN_PATCHES` (terminal) nombra dos parches de resolución | Nombra solo uno; el launcher siempre pasa el elegido en **Resolución de render** |
| El juego se cierra al iniciar tras añadir un parche | El parche escribe bytes incorrectos para tu build | Desactívalo; los parches con firma son más seguros que las direcciones fijas |

## Escribir un parche

Los parches usan el formato XML de shadPS4/GoldHEN. Un archivo tiene los title IDs a los que se
aplica y un `Metadata` por parche:

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

**Atributos de `Metadata`**

| Atributo | Significado |
|---|---|
| `Name` | El nombre del parche, como lo muestran el launcher y el registro. Un nombre que empieza por `Resolution Patch - <ancho>x<alto>` se convierte en una opción de **Resolución de render** |
| `Note` | Aparece bajo el parche en el launcher. El port lee "Requires setting DMEM to N MB" y "Requires setting VBlank to N FPS" |
| `Author`, `PatchVer` | Se muestran en el launcher (autor) y quedan como referencia |
| `AppVer` | La versión del juego a la que se aplica el parche: `01.02` |
| `AppElf` | `eboot.bin` (los parches para otros archivos se ignoran) |
| `isEnabled` | `true`: activado por defecto |

**Atributos de `Line`**

| Atributo | Significado |
|---|---|
| `Type` | `bytes` (hex, se admiten espacios), `bytes16`, `bytes32`, `bytes64` (un número, escrito en little-endian), `float32`, `float64`, `utf8` y `utf16` (texto, se añade un terminador cero) |
| `Value` | Lo que se escribe. En `bytes`, `{rel:NOMBRE:TAIL}` escribe el desplazamiento relativo de 4 bytes hasta el destino `NOMBRE` (ver abajo) |
| `Pattern` | La **firma**: los bytes originales alrededor del punto, en hex, con `??` en los bytes que pueden cambiar entre builds (desplazamientos relativos, destinos de salto). Debe coincidir exactamente una vez en el `eboot.bin` |
| `At` | Dónde empieza la escritura, contando en bytes desde el inicio de `Pattern` |
| `Address` | El punto en el build de referencia (base `0x400000`). Solo se usa cuando la línea no tiene `Pattern` |
| `Target` | Escribe en un destino con nombre en lugar del punto de la propia línea: datos a los que apunta el código, como una variable que no tiene bytes propios para buscar |

**Destinos.** Un `Target` da nombre a una dirección que el parche necesita, encontrada por su propio
`Pattern`/`At` (o `Line="N"`: el punto de la línea N, contando desde 0). Con `Ref="1"`, los 4 bytes
encontrados ahí son un desplazamiento relativo, y el destino es la dirección a la que apunta (`Tail`:
cuántos bytes de la instrucción vienen después de ese desplazamiento). El parche de 240 FPS lo usa
para hacer que una instrucción apunte a la constante 60,0 del propio juego:

```xml
<Line Type="bytes" Address="0x0048ff1a" Value="488d05{rel:sixty:0}" Pattern="c5 fa 11 85 98 fb ff ff 48 8d 05 ?? ?? ?? ?? c5 fa 10 00" At="8"/>
<Target Name="sixty" Address="0x006fc0d8" Pattern="c5 fa 59 85 9c fd ff ff c5 fa 59 05 ?? ?? ?? ?? e8 ?? ?? ?? ?? c5 f9 70 c0 00" At="12" Ref="1" Tail="0"/>
```

**Firmas.** `tools/make_signatures.py` las genera a partir del build de referencia. Un buen patrón es
lo bastante largo para ser único y cambia por `??` cada byte que varía entre builds, como los 4 bytes
después de un `e8` (call) o un desplazamiento relativo a rip. Las líneas `mask` (`Type="mask"`, la
búsqueda por patrón de algunos archivos de shadPS4) no se admiten.

**Probar.** Pon el archivo en la carpeta `patches/` de un mod ([Parches en mods](#parches-en-mods)),
inicia el juego y lee el registro: el parche debe aparecer en la línea `writes from [...]` y no en una
línea `skipped`.

## Parches en mods

Un mod puede traer parches: archivos XML en su carpeta `patches/`, con el formato de arriba. Un mod
puede tener solo parches, sin archivos del juego.

```text
mods/
  Mi parche/
    mod.json
    patches/
      mi-parche.xml
```

- Los parches del mod se aplican mientras el mod está activado; **Cargar mods** apagado también los
  desactiva.
- Se suman a los incluidos, en orden de carga. Un parche con el mismo nombre que uno incluido (o que
  uno de un mod más arriba en la lista) lo **sustituye**.
- Un archivo cuya lista `TitleID` no tiene ninguno de los IDs del juego (`CUSA01623`, `CUSA01715`) se
  ignora. Un archivo sin `TitleID` se acepta.
- `isEnabled="true"` activa el parche junto con el mod. Los parches de resolución y de tasa de
  fotogramas son la excepción: siempre siguen a **Rendimiento**. Un
  `Resolution Patch - <ancho>x<alto>` se convierte en una opción de **Resolución de render** mientras
  el mod está activado, y se aplica cuando lo eliges; mira el [mod Ultrawide](ultrawide.md).
- El panel del mod muestra "parches del juego" en los mods que los traen.

## Parches de terceros (shadPS4 y GoldHEN)

Los archivos de parches de terceros van en la carpeta `patches/` de la carpeta de datos (en la página
**Mods** del launcher, **Parches de terceros** (*Third-party patches*) los lista y
**Carpetas → Carpeta de parches** elige otra carpeta).

![La sección Parches de terceros de la página Mods del launcher, con dos parches de ejemplo](images/third-party-patches.es.png)

- Cada parche aparece como `<archivo>.xml/<nombre del parche>`. Si viene activado por defecto lo decide
  el `isEnabled` del archivo; tus elecciones se guardan en `patches.json`:

  ```json
  {"enabled": ["archivo.xml/Nombre del parche"], "disabled": ["archivo.xml/Otro parche"]}
  ```

- Solo se usan parches de la versión `01.02` para `eboot.bin`; los archivos con el `TitleID` de otros
  juegos se ignoran.
- **Solo se aplican al build de referencia**, el `eboot.bin` con sha256
  `d85c8135d330c3b601bf5dc3f1dd86bd6119fb8a9fce372bed2c9785f9a79299`, porque usan direcciones fijas.
  Con otro build no se aplican; la primera línea `Patches:` del registro muestra tu sha256.
- Tipos de línea admitidos: `bytes`, `bytes16/32/64`, `float32/64`, `utf8`, `utf16`. Un parche con una
  línea `mask` o con una dirección fuera del `eboot.bin` queda fuera entero, con un aviso en el
  registro.
- Se aplican después de los incluidos: donde ambos escriben, gana el de terceros.
- Desde un terminal: `SPARTAN_PATCHES_DIR=/ruta SPARTAN_PATCHES_CONFIG=/ruta/patches.json bash run.sh`.

Para usar un parche de terceros en cualquier build, conviértelo al formato incluido, con firmas, y
distribúyelo en un mod ([Parches en mods](#parches-en-mods)).

## Créditos

- **kvicken** y **cuesta4**: los parches de resolución, corrección de texturas, 120 FPS y saltar
  vídeos, publicados para shadPS4, a partir de los parches de PS3 de **illusion0001**.
- **SpartanPort** (Felipe Prado): la adaptación de esos parches a este build y a firmas, las versiones de
  100, 144 y 240 FPS, la corrección de la pantalla negra al iniciar y los tamaños ultrawide, 16:10 y 4:3
  (`tools/make_ultrawide_patch.py`).
