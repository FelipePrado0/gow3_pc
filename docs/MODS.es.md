# Mods de archivos

[English](MODS.md) | [Português](MODS.pt-BR.md) | **Español**

Los mods cambian texturas, modelos, sonidos, textos y videos del juego sin modificar la carpeta del
juego. En cada inicio el launcher monta una vista del juego con los archivos de los mods encima, y la
elimina cuando el juego se cierra.

**Índice:** [Página Mods](#la-página-mods-del-launcher) ·
[Qué se puede cambiar](#qué-se-puede-cambiar-archivo-por-archivo) · [Ejemplos](#ejemplos) ·
[Problemas comunes](#problemas-comunes) · [Derechos de autor](#compartir-mods-y-derechos-de-autor) · [Cómo armar un mod](#cómo-armar-un-mod) ·
[Dónde se aplican los mods](#dónde-se-aplican-los-mods) ·
[Mods con parches del juego](#mods-con-parches-del-juego) ·
[Parches del juego](PATCHES.es.md)

## La página Mods del launcher

![La página Mods con tres mods de ejemplo: dos cambian R_UI.WAD, la lista de conflictos está abierta y el panel muestra el mod seleccionado](images/mods-page.es.png)

*Tres mods de ejemplo. "Kratos Gold Armor" y "Clean HUD" cambian `R_UI.WAD`: el aviso bajo la
lista muestra el archivo, y gana "Clean HUD" por estar más abajo.*

Los nombres son los del launcher en español, con el nombre en inglés entre paréntesis.

- **Cargar mods** (*Load mods*): el interruptor general. Apagado, no entra ningún mod ni parche de mod.
- **Orden de carga** (*Load order*): la lista de mods, con una casilla para activar cada uno y
  flechas para cambiar el orden. **Gana el mod que está más abajo** cuando dos mods cambian el mismo
  archivo. El contador muestra cuántos están activos ("3 de 5 activos").
- **Conflictos**: cuando más de un mod activo cambia el mismo archivo, aparece un aviso bajo la
  lista ("⚠ 2 archivos los cambian varios mods. Gana el que está más abajo.") y un ⚠ junto a cada
  mod implicado. **Detalles** (*Details*) abre la lista de archivos, con los mods en orden de carga
  (`EXEC/WAD/PS4_LE/R_UI.WAD: A > B`, gana B). Dos mods nunca se mezclan en un mismo archivo: el
  ganador entra entero.
- **Panel del mod**: al hacer clic en el nombre de un mod se abre un panel con imagen, nombre, autor,
  versión, descripción, cuántos archivos cambia, si trae parches del juego y sus conflictos.
  **Abrir carpeta del mod** y **Sitio web** abren la carpeta y la web (cuando el mod indica una).
  Otro clic cierra el panel.
- **Preajustes** (*Presets*): guardan el orden y qué mods están desactivados. Escribe un nombre y
  haz clic en **Guardar**; **Cargar** aplica un preajuste guardado y **Borrar** lo elimina. Al cargar,
  los mods que ya no existen se ignoran y los nuevos entran al final, activados. Los preajustes se
  guardan en `mods_presets.json` en la carpeta de datos.
- **Parches de terceros** y **Carpetas** (carpeta de mods y carpeta de parches) están en la misma
  página.
- **Actualizar** (*Refresh*) vuelve a leer la carpeta de mods después de copiar un mod nuevo con el
  launcher abierto.

Los cambios se aplican en el siguiente inicio del juego.

## Qué se puede cambiar, archivo por archivo

Un mod cambia **archivos enteros**. El cargador nunca edita dentro de un archivo: para cambiar una
textura de un WAD, el mod trae el WAD entero con esa textura modificada. La tabla muestra qué
contiene la carpeta del juego (cifras de CUSA01623 01.02, inglés de EE. UU.) y qué exige cambiar
cada tipo de archivo. Los formatos salen de los propios archivos (cabeceras `XVAG` y `GNF`, H.264 en
los vídeos `.BSF`); lo que contiene cada carpeta sale de los nombres de carpetas y archivos, y el
tiempo de los subtítulos en los `.TXT` de las cinemáticas sale de los números que contienen. No se
probó en el juego la sustitución de cada tipo de archivo.

| Carpeta del juego | Qué contiene | Formato | ¿Un mod puede cambiarlo? |
|---|---|---|---|
| `EXEC/WAD/PS4_LE/` | Casi todo el juego: modelos, texturas, niveles, menús, HUD y textos (410 `.WAD`) | Paquete WAD, little-endian | **Sí.** Trae el WAD entero editado, con el mismo nombre |
| `EXEC/SOUNDS/STREAMS_PS4/` | Música y efectos de sonido (945 archivos) | `.XVAG` (contenedor de audio de Sony) | **Sí**, como un `.XVAG` válido, en el mismo formato que el original |
| `<IDIOMA>/SOUNDS/STREAMS_PS4/` | Voces y diálogos, una carpeta por idioma (unos 1.200 archivos cada una) | `.XVAG` | **Sí**, por idioma: `ENGLISH/` no cambia `SPANISH/` |
| `EXEC/CINEMATICS/` | Cinemáticas pre-renderizadas (48 `.BSF`) y el tiempo de los subtítulos (35 `.TXT`) | `.BSF` = vídeo H.264 en bruto; `.TXT` = número de línea, primer fotograma, último fotograma | **Sí.** Mantén la resolución y la tasa de fotogramas del original; el `.TXT` cuenta en fotogramas del vídeo |
| `EXEC/INTRO/` | Logotipos e imágenes de la apertura (18 archivos) | `.GNF` (textura de PS4) | **Sí**, como `.GNF` |
| `EXEC/photomode/` | Imágenes del modo foto (15 archivos) | `.gnf` | **Sí**, como `.gnf` |
| `EXEC/ICONS/` | Imágenes de la lista de partidas (`ICON0.PNG`, `ICON_AUTO.PNG`, `NEWSAVE.PNG`, ...) | `.PNG` | **Sí**, como PNG |
| `EXEC/DLC/` | WADs y listas del contenido descargable | `.WAD`, `.TXT` | **Sí**, como los demás WADs |
| `eboot.bin`, `sce_module/`, `sce_sys/` | El programa, sus bibliotecas y los datos del paquete | Ejecutables de PS4 | **No.** El cargador los rechaza; los cambios de código usan [parches del juego](#mods-con-parches-del-juego) |

### Qué WAD contiene qué

Los nombres de los WADs siguen un patrón. Algunos grupos en `EXEC/WAD/PS4_LE/`:

| Archivos | Qué contienen |
|---|---|
| `R_UI.WAD` | Interfaz: menús, HUD, iconos de botones y el script de los menús |
| `R_PERM.WAD` | Por el nombre, recursos que se mantienen todo el juego; tiene otra copia del botón de QTE |
| `SHELL.WAD` | El menú principal |
| `R_LANG_EN.WAD`, `R_LANG_ES.WAD`, `R_LANG_LA.WAD`, ... (16 archivos) | Textos de menús y subtítulos, uno por idioma |
| `R_HERO00.WAD` a `R_HERO08.WAD` | Por el nombre, el héroe (Kratos) |
| `R_W_CESTUS*`, `R_W_ELECTRIC*`, `R_W_EXPLOSIVE*`, `R_W_GODMODE*`, `R_W_SOUL*` | Por el nombre, armas y sus niveles de mejora |
| `R_ZEUS*`, `R_HADE*`, `R_POSH*`, `R_CYCL*`, `R_GRUN*`, `R_SOUL*`, ... | Por el nombre, personajes y enemigos |
| `OLY*`, `HDS*`, `POS*`, `LAB*`, `CRN*`, `BAS*`, `CAV*`, `ICA*`, `GAZ*`, ... | Por el nombre, niveles y zonas, en partes (`BAS100.WAD`, `BAS125.WAD`, ...) |
| `CHALLE*` | Por el nombre, los niveles de desafío |

"Por el nombre" significa que el grupo se identificó solo por el nombre de los archivos; compruébalo
dentro del WAD antes de contar con ello.

### Dentro de un WAD

Un WAD es una lista de entradas, cada una con una cabecera de 64 bytes (etiqueta, flags, tamaño y un
nombre de hasta 56 caracteres) y sus datos, alineados a 16 bytes. Las texturas son entradas llamadas
`TX<hash>.<hash>.dds` con un prefijo corto seguido de un archivo DDS normal (DXT1 o DXT5), y los
materiales las encuentran por ese nombre. Un mod de textura que mantiene el tamaño y el formato DXT
del original es el tipo más seguro.

Los WAD de nivel (`OLY150.WAD` y similares, no `R_UI.WAD` ni `R_PERM.WAD`) empiezan con una entrada
`EntityCount` (etiqueta 0) cuyo campo de tamaño guarda una cuenta de entidades, no una longitud, y
tras su cabecera no vienen datos. Un lector que salta `size` bytes ahí cae en medio del archivo y no
encuentra nada después: hasta la versión 2.5.0 el propio lector del port lo hacía, así que podía cambiar
texturas en `R_UI.WAD` y `R_PERM.WAD` pero en ningún nivel. `scripts/glyphs.py` (`entries()`) y
`tools/wad_textures.py` ahora leen ese marcador como una cabecera sola, de modo que cualquier textura
de un WAD de nivel se puede encontrar y cambiar igual que las de la interfaz.

El port cambia texturas así: los iconos de botones (Xbox, Nintendo, teclado) se dibujan en copias de
`R_UI.WAD`, `R_PERM.WAD` y `OLY150.WAD` (la lira de las Musas en la Cámara de la Llama), manteniendo
el tamaño y el formato de cada textura, de modo que solo cambian sus píxeles (`scripts/glyphs.py`). En
una textura de nivel solo se vuelven a codificar los bloques de 4 × 4 cuyos píxeles cambiaron, en
cada nivel de mip, así que el resto de la textura queda byte a byte como el original.

### Encontrar una textura del mundo

Algo pintado sobre una superficie del mundo suele estar pintado en varias texturas a la vez, una por
capa del material, en el mismo lugar de cada una: el color (diffuse), el brillo (specular, en grises),
la luz que emite (una máscara de brillo, a menudo más pequeña), su relieve (un normal map: DXT5, X en
alfa e Y en verde) y a veces un decal dibujado encima. Si cambias una y dejas otra, el dibujo viejo se
sigue viendo debajo o encima. Las cuatro letras griegas de la lira están en cinco texturas de
`OLY150.WAD`; las de color que seguían visibles sobre las letras nuevas venían del decal.

Para encontrarlas todas, encuentra el WAD de la sala (`--grep` con palabras de la sala:
`--grep Muse Lyre`) y arma hojas solo con la parte de cada textura donde está el dibujo, a un tamaño
legible: `--crop 0.75,0,1,0.25` toma el cuarto superior derecho (izquierda, arriba, derecha, abajo,
en fracciones). Cada textura que muestra las mismas formas ahí es una capa que hay que cambiar.

`tools/wad_textures.py` encuentra una textura mirando: arma hojas de contacto numeradas con todas las
texturas de los WAD que nombres (`python tools/wad_textures.py "<juego>\EXEC\WAD\PS4_LE" OLY1* -o sheets`),
arma hojas con una parte de cada textura (`--crop`), extrae las numeradas como `.dds` (`--extract`),
busca palabras en los nombres de entrada de todos los WAD para encontrar el WAD de una sala
(`--grep Muse Lyre`) y ordena las texturas pequeñas que parecen indicaciones de botones (`--find`).
Es una herramienta de desarrollo que vive en el repositorio:
ejecútala con Python desde una copia del código fuente.

Los WADs de `PS4_LE` son little-endian. La versión de PS3 funcionaba en una consola big-endian, así
que no se espera que sus WADs funcionen aquí.

### Lo que un mod no puede hacer

- Cambiar la lógica del juego (daño, física, IA, cámara, tiempos): está en el ejecutable. Usa
  [parches del juego](#mods-con-parches-del-juego).
- Sustituir `eboot.bin`, `sce_module` o `sce_sys`: el cargador los rechaza.
- Inyectar una DLL o un script.
- Mezclar dos mods en un mismo archivo: el que está más abajo en la lista sustituye el archivo entero.
- Cambiar un archivo que no existe en tu juego: un mod hecho para otra región (por ejemplo, el
  `R_LANG_UK.WAD` europeo) añade un archivo que nadie lee, y el registro muestra `added` en lugar de
  `replaced`.

## Ejemplos

**Un mod de textura o modelo.** El mod trae el WAD editado en la ruta del juego:

```text
mods/
  Kratos Gold Armor/
    mod.json
    preview.png
    EXEC/WAD/PS4_LE/R_HERO00.WAD
```

**Una línea de voz solo en español.** El archivo va en la carpeta del idioma, con el nombre del
original:

```text
mods/
  Voces en espanol/
    SPANISH/SOUNDS/STREAMS_PS4/3004CA_ALT1.XVAG
```

**Una corrección de subtítulos.** El texto de cada idioma está en su `R_LANG_*.WAD`. Sustituye el del
idioma en el que juegas y comprueba en el juego que es el archivo que lee: hay más de un archivo de
español (`R_LANG_ES.WAD` y, por el nombre, `R_LANG_LA.WAD`).

**Una cinemática con subtítulos nuevos.** El vídeo es `EXEC/CINEMATICS/<NOMBRE>.BSF` y el tiempo de
los subtítulos es `EXEC/CINEMATICS/<NOMBRE>.TXT`; el texto de los subtítulos está en los
`R_LANG_*.WAD`.

Para encontrar la ruta exacta de un archivo, búscalo en la carpeta del juego: la ruta dentro del mod
debe ser la misma a partir de `EXEC/` o de la carpeta del idioma (mayúsculas y minúsculas dan igual).

## Problemas comunes

| Qué ves | Por qué | Qué hacer |
|---|---|---|
| El mod no aparece en la lista | Su carpeta no tiene `EXEC`, carpeta de idioma, `dvdroot_ps4` ni `patches/`, o una carpeta por fuera de ellos también tiene otras carpetas o archivos | Mueve los archivos para que `EXEC/` quede justo dentro de la carpeta del mod, o deja solo esa carpeta (y readme o imágenes) en cada carpeta por fuera |
| El registro dice `0 game files replaced` | Las rutas dentro del mod no coinciden con las del juego | Compáralas con la carpeta del juego, a partir de `EXEC/` |
| El registro dice `added` en lugar de `replaced` | El archivo no existe en tu juego (otra región o versión) | Usa un mod hecho para tu versión, o renómbralo como el archivo de tu juego |
| El cambio de un mod no se ve | Otro mod más abajo en la lista cambia el mismo archivo | Abre **Detalles** bajo la lista o cambia el orden |
| El juego se cierra o muestra modelos rotos tras un mod | El archivo es de otra versión (PS3, otra versión del juego) o se guardó en otro formato | Desactiva el mod; usa archivos hechos para esta versión, manteniendo el formato y el tamaño de las texturas del original |
| Un ZIP no se descomprimió | Ya hay una carpeta con el mismo nombre, o el zip tiene rutas peligrosas | Borra o renombra esa carpeta, o descomprime el zip tú mismo; el registro indica el motivo |

## Compartir mods y derechos de autor

Los archivos del juego pertenecen a sus titulares de derechos. Un mod hecho como describe esta guía
trae archivos enteros del juego (un WAD, un XVAG, un vídeo) con tus cambios dentro, así que
compartirlo también comparte el contenido original de esos archivos. Comparte solo lo que tienes
derecho a compartir: tus propias texturas, sonidos y textos, o archivos cuyo dueño lo permite. El
launcher no descarga ni comparte mods: solo coloca los mods de tu carpeta de mods sobre tu propia
copia del juego.

## Cómo armar un mod

La carpeta predeterminada es `mods/` en la carpeta de datos: en Windows, junto a
`SpartanPort.exe`; en Linux (AppImage), en `~/.local/share/spartanport`. **Carpetas → Carpeta de mods**
(*Folders → Mods folder*) elige otra.

Cada mod tiene su propia carpeta, que replica el árbol del juego. En SpartanPort los datos están en `EXEC/`
y en una carpeta por idioma (`ENGLISH/`, `SPANISH/`, ...):

```text
mods/
  Nombre del mod/
    EXEC/WAD/PS4_LE/R_UI.WAD
    SPANISH/SOUNDS/...
```

También se aceptan:

- `Nombre del mod/dvdroot_ps4/EXEC/...` (la convención de shadPS4): `dvdroot_ps4/` se descarta y el
  resto va a la raíz del juego;
- `Nombre del mod/app0/EXEC/...`;
- carpetas por fuera de cualquiera de estas, como la de un archivo descomprimido en una carpeta con
  su nombre: `Nombre del mod/Nombre del mod v1.2/EXEC/...`. Cada carpeta por fuera solo puede tener
  esa carpeta, más archivos que describen el mod (`.txt`, `.md`, `.json`, `.ini` e imágenes), que se
  ignoran.

Una carpeta sin `EXEC`, carpeta de idioma, `dvdroot_ps4` ni `patches/` no aparece en la lista. Un ZIP
así se descomprime, pero el registro avisa
`not a mod (no EXEC, language folder, dvdroot_ps4 or patches)`.

### Carpetas reconocidas

Carpetas de idioma que siempre se reconocen: `ENGLISH`, `FRENCH`, `GERMAN`, `ITALIAN`, `SPANISH`,
`PORTUGUESE`, `DUTCH`, `POLISH` y `RUSSIAN`.

Además cuentan las carpetas de datos que existen en la raíz del juego seleccionado, como `JAPANESE/`
en una versión asiática. Esto vale solo para el juego seleccionado: una carpeta de otra instalación
no cuenta aquí, y `eboot.bin`, `sce_module` y `sce_sys` nunca se sustituyen. La regla es la misma para
la lista del launcher y para aplicar los mods.

Un mod hecho para otra región (por ejemplo, el `R_LANG_UK.WAD` de la versión europea) no cambia nada
en un juego que no tiene ese archivo: el registro muestra `added` en lugar de `replaced`.

### Nombre, autor, imagen y sitio web

El panel del mod lee un `mod.json` opcional en la carpeta del mod, junto a `EXEC/`. Todos los campos
son opcionales y deben ser texto:

```json
{
  "name": "Kratos Gold Armor",
  "author": "Tu nombre",
  "version": "1.0",
  "description": "Armadura dorada para Kratos. Sustituye R_HERO00.WAD.",
  "url": "https://example.com/kratos-gold-armor"
}
```

| Campo | Se muestra como | Sin él |
|---|---|---|
| `name` | El nombre del mod en la lista y en el panel | El nombre de la carpeta |
| `author` | "por …" bajo el nombre | Nada |
| `version` | "v…" junto al autor | Nada |
| `description` | El texto del panel | Los primeros 400 caracteres de un `readme.txt` o `readme.md` |
| `url` | El botón **Sitio web** (solo aparece si el enlace empieza por `http://` o `https://`) | Sin botón |

**La imagen es un archivo, no un campo.** Pon `preview.png` (o `.jpg`/`.jpeg`) en la misma carpeta.
Sin ella, el launcher usa `cover.*`, luego `screenshot.*` y luego cualquier otra imagen de la carpeta.
El panel la muestra con 268 píxeles de ancho como máximo (y como mucho 4:3 de alto); una imagen 16:9
de 1280 × 720 es suficiente. Un `mod.json` que no se puede leer se ignora, como si no existiera.

La carpeta terminada:

```text
mods/
  Kratos Gold Armor/
    mod.json
    preview.png
    readme.txt
    EXEC/WAD/PS4_LE/R_HERO00.WAD
```

### Mods en ZIP

Un mod en **ZIP** se puede soltar directamente en la carpeta de mods: al abrir el launcher o iniciar
el juego se descomprime en una carpeta con el nombre del zip, y se vuelve a descomprimir cuando el
zip cambia. Una carpeta con el mismo nombre creada por ti no se sustituye. Se rechazan las rutas
peligrosas, los enlaces y los zips que pasan de 16 GiB descomprimidos. Los archivos 7z y RAR hay que
descomprimirlos antes. `tools/zip_mod.py` empaqueta una carpeta de mod.

### Nombres de archivo y registro

Las mayúsculas y minúsculas en los nombres del mod dan igual: un archivo con otra grafía sustituye
el archivo del juego con el mismo nombre. Se rechazan los enlaces simbólicos y las junctions dentro
del mod.

El registro muestra cuántos archivos tiene cada mod (`Mods: Nombre del mod: 12 files`), qué mod ganó
cada archivo en disputa (`Mods: EXEC/...: A -> B`) y el total: `Mods: 12 game files replaced, 0 added`.
Si un mod debería sustituir archivos y aparece `0 game files replaced`, revisa las rutas dentro de él.

## Dónde se aplican los mods

Los archivos del juego nunca se modifican. En cada inicio el launcher monta una vista del juego en una
carpeta `mod-game-*` y la borra cuando el juego termina. `/app0` queda en solo lectura; las partidas
guardadas siguen en su carpeta habitual.

- La vista se crea en `out/` dentro de la carpeta de datos. En Windows sin permiso de enlaces
  simbólicos (Modo de desarrollador desactivado), si la carpeta de datos y el juego están en discos
  distintos, se crea junto a la carpeta del juego, porque los hard links solo funcionan en el mismo
  disco.
- Con enlaces simbólicos disponibles, las carpetas y los archivos del juego entran como enlaces. Sin
  ellos, en Windows, las carpetas pasan a ser junctions y los archivos hard links (una copia cuando el
  archivo está en otro disco).
- En una carpeta de red (SMB), que rechaza enlaces y junctions, las carpetas se replican: carpetas
  reales con los archivos en hard link o copiados. Los archivos del juego y del mod quedan intactos.

La carpeta `mod-game-*` apunta a los archivos del juego. Si queda una después de un cierre
inesperado, no la borres con herramientas que siguen enlaces (como `rm -rf` en Git Bash o WSL): eso
puede borrar los archivos reales del juego.

Una carpeta `<carpeta del juego>-mods/EXEC/...` junto a la carpeta del juego (la convención de
shadPS4) se carga antes que los mods de la lista. El interruptor general también la desactiva.

## Orden y configuración

El orden y los mods desactivados se guardan en `mods.json` en la carpeta de datos:

```json
{"order": ["Primero", "Segundo"], "disabled": ["Primero"]}
```

Las carpetas nuevas entran activadas, al final de la lista, en orden alfabético.
`SPARTAN_MODS_CONFIG` cambia la ruta de ese archivo.

Los preajustes de la página Mods se guardan en `mods_presets.json`, una entrada por nombre, con las
mismas dos listas:

```json
{
  "Solo voces": {"order": ["Voces en espanol", "Kratos Gold Armor"], "disabled": ["Kratos Gold Armor"]}
}
```

Desde un terminal:

```sh
SPARTAN_GAME_DIR=/ruta/CUSA01623 SPARTAN_MODS_DIR=/ruta/mods bash run.sh
SPARTAN_GAME_DIR=/ruta/CUSA01623 SPARTAN_MODS_ENABLED=0 bash run.sh
```

## Mods con parches del juego

Un mod también puede traer **parches de código**: archivos XML en su carpeta `patches/`, con el
formato de los parches incluidos. Un mod puede tener solo parches, sin archivos del juego:

```text
mods/
  Nombre del mod/
    patches/
      mi-parche.xml
```

Se aplican mientras el mod está activado (**Cargar mods** apagado también los desactiva), y un parche
con el mismo nombre que uno incluido lo sustituye. Un `Resolution Patch - <ancho>x<alto>` se
convierte en una opción de **Render resolution**; el mod [Ultrawide](ultrawide.md)
(`extras/mods/Ultrawide`) es el ejemplo. El formato, las reglas y cómo probar un parche están en la
[guía de parches](PATCHES.es.md#parches-en-mods).

## Iconos de botones y el dibujo del mando

La opción de iconos en **Controles** prepara los avisos de botones y el dibujo del mando en el menú de
pausa al elegir Xbox o Nintendo (o cuando el modo automático detecta uno de esos mandos). No hace
falta instalar ningún mod. El estilo PlayStation mantiene los archivos originales; el estilo de
teclado mantiene el dibujo original del mando. Las copias se crean en la caché local, sin modificar
el dump del jugador.

Los dibujos y la herramienta de `tools/controller_diagrams/` vienen de la contribución de Kristian
Andreev en la [PR #234](https://github.com/FelipePrado0/SpartanPort/pull/234), bajo GPL-2.0-or-later.
`scripts/controller_diagrams/*.png` contiene solo esos dibujos, sin archivos del juego. Para
regenerarlos, la herramienta usa Pillow, NumPy y scikit-image, con la fuente Cinzel incluida en el
proyecto (OFL). Esas dependencias de dibujo no hacen falta para usar los PNG en el launcher.

# Parches de terceros

Los archivos de parches de shadPS4 y GoldHEN van en la carpeta `patches/` de la carpeta de datos y
aparecen en la página **Mods** del launcher, en **Parches de terceros**. Solo se aplican a un build de `eboot.bin`: mira
la [guía de parches](PATCHES.es.md#parches-de-terceros-shadps4-y-goldhen).
