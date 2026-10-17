# Trophies

The game calls the PlayStation trophy library (`sceNpTrophy*`). This port answers those calls, keeps
the unlocked trophies on disk and shows them to the player: a notice when a trophy is unlocked, a
list inside the game and a **Trophies** page in the launcher.

Everything is read from the player's own game folder (`sce_sys/trophy/trophy00.trp`); no game data
is part of the port.

## What the player sees

| Where | What |
|-------|------|
| In the game, when a trophy unlocks | A card at the top-right (below the FPS counter when it is on) with the trophy's icon, name and description. It slides in, stays 5 seconds and fades. |
| In the game, **F8** or **R3+R2** | A list of all trophies: icon, name, description and state (unlocked in colour, locked dimmed), the count, and a filter (all / unlocked / locked). |
| Launcher, **Trophies** page | The same list, with the category (platinum, gold, silver, bronze) and a filter. |

Keys of the in-game list: Up/Down, W/S, PageUp/PageDown, Home/End, mouse wheel or D-pad scroll;
Left/Right, A/D or L1/R1 change the filter; F8, Esc, Backspace, Enter, Circle/Cross or R3+R2 close it.
While it is open the game gets no input (like the Insert menu).

The platinum (id 0) is unlocked with the last other trophy, as the PlayStation does.

## Language

The launcher writes `user/trophy_texts.txt` every time it starts the game, in the launcher's
language, and the game reads it at start. Change the language of the launcher and start the game
again to change the language of the notice and of the list.

Languages: English, Russian, Arabic, Spanish, Portuguese, French, German, Italian, Polish, Turkish,
Chinese, Japanese and Korean (`launcher/gow3_trophies.py`). English is the text of the PlayStation
list; Portuguese is the text of the Brazilian PlayStation list. **The others are translations, not
the official localized texts** (those are encrypted inside the `.trp` and are not read). A language
without a list uses English.

Without the launcher, put the same file by hand in the saves folder (`user/` or `GOW3_USER_DIR`):
`trophy_texts.txt`, or `trophy_names.txt` (same format, used when there is no `trophy_texts.txt`).

### File format

```
# comment
@heading=Trophies
@notice=TROPHY UNLOCKED
@unlocked=UNLOCKED
@locked=LOCKED
@all=All trophies
@only_unlocked=Unlocked
@only_locked=Locked
@counter={} of {} unlocked
@hint=Up/Down: scroll   Left/Right: filter   F8 / R3+R2 / Esc: close
0=King of the Hill | Unlock all trophies
1=Releasing the Floodgates | Kill Poseidon
```

`ID=Name | Description` (the description is optional), UTF-8, one trophy per line. A name takes up to
63 bytes and a description up to 159. A trophy without a line is shown as `Trophy ID N`. Missing
`@` lines use the English labels.

## Saved state

`user/trophies.dat`: 16 bytes, four little-endian 32-bit words. Trophy `id` is bit `id % 32` of
word `id / 32`. Delete the file to reset the trophies. It is separate from the game's own saves.

## Trophy ids

The ids are the numbers in the icon names of the package (`TROP000.PNG` ... `TROP034.PNG`): 35
trophies, id 0 the platinum, 1-18 bronze, 19-29 silver, 30-34 gold (by the colour of the icons).
The texts are listed in the order of the PlayStation list, which matches this order.

## How it works

- `src/runtime_services.c`: the `sceNpTrophy*` functions. On `RegisterContext` it reads
  `trophy00.trp` (header big endian: magic `DC A2 4D 00`, file count at `0x10`, entry size at
  `0x14`, entries from `0x60`: name, offset, size) and the texts file. `UnlockTrophy` sets the bit,
  saves the state and shows the notice; the platinum is granted when every other trophy is set.
  `GetTrophyUnlockState` returns the saved bits and the trophy count.
- `gpu/gow3gpu.h`: `gow3gpu_toast_trophy` (the notice) and `gow3gpu_trophy_setup` (the list: texts,
  icons and a pointer to the live state), handed over once by the runtime.
- `gpu/shim/gow3_overlay.cpp`: draws the notice and the list with Dear ImGui, decodes the icons with
  `Gow3Png` (on a worker thread) and handles the keys and buttons of the list.
- `launcher/gow3_trophies.py`: the texts, the reader of the `.trp` and of `trophies.dat`.
- `launcher/gow3_launcher_win.py`: the Trophies page and `write_trophy_texts`.
- `tests/test_trophies.py`: unit tests (they build a `.trp` and need no game files).

## Limits

- Only trophies unlocked through this port are known. Trophies earned on a PlayStation are not
  imported, and with an already advanced save the game usually does not unlock again what it did before.
- The names and descriptions of the package are encrypted (`TROP*.ESFM`) and are not read: the
  texts above are built into the launcher.
- Trophy synchronisation with PSN is not implemented, and there is no online profile.
- R3+R2 can be pressed by accident in combat; the Insert menu uses R3+L2 for the same reason.
