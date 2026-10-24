---
status: accepted
---

# Replace GPU emulation with a Native renderer rebuilt Pass by Pass

The game's graphics currently reach the screen through GPU emulation built from shadPS4 code: PM4 command buffers are decoded and GCN shaders recompiled while the game runs, and PS4 memory is watched to find textures and buffers. We will replace it with a Native renderer that reimplements each of the engine's own Passes as Vulkan calls on PC-format resources, and remove GPU emulation entirely once Parity is reached. The goal is a port with no emulation: GPU compatibility limited only by what the game itself needs, not by what imitating a PS4 GPU needs, and no per-draw translation cost.

## Considered Options

- **Swap Vulkan for D3D12 under the existing emulation.** Rejected: keeps every emulation cost and fixes none of the emulation's compatibility problems. Vulkan is already a native PC API.
- **Implement the Gnm API natively (HLE).** Not possible here: the game imports only 11 `libSceGnmDriver` functions (shader setup, submit, markers) and no draw calls; its command buffers are built by code inside the eboot.
- **Hook the engine's command-stream functions (ICE `icecommandstream`) and emit Vulkan per call.** Rejected in favour of whole Passes: it still works with PS4 descriptors in PS4 memory, so it needs resource tracking that is close to emulation, and it gives no clean place for the port's own features (upscalers, motion vectors).
- **Reimplement the engine's Passes (chosen).** Highest reverse-engineering cost, but each Pass owns its resources, and port features such as FSR 3.1 and DLSS get the depth, camera and jitter they need directly.

## Consequences

- "Native" has a fixed boundary: converting game data once, during Preparation or when the engine first loads an asset (cached on disk), is allowed; any translation per frame or per draw is emulation. The game's own material shaders are translated from the player's dump under that rule and are never shipped.
- Until Parity, development builds draw Hybrid frames, with migration running from the end of the frame backwards: presentation, UI, post-processing, lighting, scene geometry, shadows, compute. Releases keep using GPU emulation until the Native renderer has every current graphics feature plus FSR 3.1 and DLSS.
- GPU emulation is the reference for correctness: every Reference scene is compared by screenshot and frame time on both paths until it is removed. The Native renderer reaches a release only when it is no slower than GPU emulation in any Reference scene, on the Steam Deck and on a low-end Windows machine.
- Mods keep shipping game files in their PS4 formats and go through the same conversion on load as the originals, so existing mods keep working. Mods in PC formats would be a separate, later feature.
- The Native renderer lives in its own `renderer/` directory, outside the vendored tree, so removing GPU emulation is a deletion of `gpu/shadps4` and `gpu/shim`.
- Target is Vulkan 1.3 core with no required optional extensions; D3D12 or D3D11 come later only if a GPU or driver needs them. macOS stays out of scope.
- Reverse engineering uses REA with Ghidra. Only signatures, documentation of engine structures and functions, and original code go into the repository, never decompiled game code or game shaders.
