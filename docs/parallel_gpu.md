# Parallel GPU command processing — design notes

Goal: remove the single-core bottleneck of the emulated GPU command processor
(`shadPS4:GpuCommandProcessor`, ~100% of one core while the rest of the CPU idles).

## Measurements (the base port's original game, test scene, ~70 FPS, `GOW3_DCB_STATS=1`)

| | |
|---|---|
| Graphics command buffers submitted | 4200–5800/s, **~65–80 per frame** |
| Draws | ~110 000/s, ~1600 per frame |
| Draws per command buffer | mostly 10–99, max ~200 |
| Nested IndirectBuffer | none |
| State before the first draw | ~118 context + ~31 SH register dwords written; only 8% start with ClearState |

GPU thread profile after the single-thread work (perf, `cpu-clock:u`):
texture binding ~25%, pipeline selection ~19% (StageSpecialization build/compare,
sharp fetches), buffer binding ~14%, render targets ~8%, PM4 decode ~3%.

## Constraints

- Command buffers are not self-contained: register state is inherited, so a worker needs
  the register state at the start of its buffer (context + SH ranges are ~6–8 KiB; one
  snapshot per buffer is ~40 MB/s at 70 FPS).
- Draw preparation reads guest memory, not only registers: extended user data (EUD) and
  shader code. Earlier packets in the stream (DMA, WriteData, constant-engine dumps) can
  change that memory, so work done ahead of the GPU thread may see stale data.
- The caches (buffer, texture, pipeline) and per-draw scratch state (flags in `Image`,
  user data inside shared `Shader::Info`) are single-threaded by design.
- Image layout and barrier tracking assume one ordered stream.

## Plan

1. **Speculative draw preparation on workers, validated on the GPU thread.**
   A cheap serial pass records the register state at the start of each command buffer.
   Workers replay their buffer's register writes and, per draw, precompute the pure
   parts: pipeline key and stage specialization inputs, sharp fetches, texture and
   render-target descriptions, dynamic state values, vertex buffer ranges. Each result
   carries the flattened user data it was computed from. The GPU thread still decodes
   the stream and refreshes the flattened user data (cheap), compares it with the
   worker's copy, and uses the prepared draw only on a match; otherwise it computes
   as today. Correctness never depends on the workers.
   Prerequisite refactor: sharp consumers read user data through a view instead of
   the shared `Shader::Info` members, and per-draw scratch moves into a context struct.
2. **Thread-safe cache lookups.** Finding existing buffers/images/views moves to the
   workers; creation, uploads, barriers stay on the GPU thread.
3. **Parallel Vulkan recording.** Each worker records its own command buffer; they are
   executed in submission order with barriers at the seams.

Every step keeps a `GOW3_TOGGLE_FILE` bit so it can be switched off at run time and
compared by screenshot and frame rate.

## Portability

Must scale down to the Steam Deck (4 cores / 8 threads): worker count follows
`hardware_concurrency()`, no busy waiting when cores are scarce, no AVX-512.

## Results

Step 1 (draw preparation, 4 workers, toggle 8192), the test scene, same view:
71.5 FPS with prepared draws vs 64.1 without (+11.5%), identical screenshots.
97–98% of direct draws use the prepared pipeline; each `bb:DrawPrep` worker ~10% of a core.
The GPU thread is still ~90% busy: texture/buffer binding is the next target (step 2).

Guest write faults (same view, toggle 65536): the game fills its per-frame buffers
sequentially and each 4 KiB page cost a protection fault — ~115k faults/s, ~20% of every
GXWorker and of the main thread spent in the kernel. Unprotecting the aligned 64 KiB window
around a fault: 16k faults/s, kernel time ~8%, **81.0 FPS vs 66.4** (+22%), identical frames.
The GPU command thread is back at ~100%: it is the limit again.

Rejected: "hot pages" (never re-protect pages written repeatedly, upload them on every
binding) — the set grew to ~14k pages, re-uploads dropped the frame rate to 33 FPS and a GPU
ring timeout followed. Left opt-in behind GOW3_HOT_PAGES=1.

Texture description cache 2-way/4096, same-target fast path, LRU touch skip, no per-texture
meta lookup: other outdoor view, 93.9 FPS; with the texture memos off (mask 1056) 65.6 FPS.
Close to the 100 Hz display cap (vblank-paced), so further gains need an uncapped test.

## Streaming stutter (GOW3_FRAME_STATS "Stall:" lines)

Running through new areas gave 60–170 ms frames (also in shadPS4). The GPU thread was busy
the whole frame, mostly in the kernel, uploading 50–400 MB of textures and buffers per frame.
Findings, in the order they were fixed:

1. Guest-to-staging copies ran on one thread (the recording thread, textures on the GPU
   thread). They now start at once on copy threads (`Gow3Copy::Async`, `gow3_copy.cpp`);
   small copies are batched per thread (one wakeup per ~512 KiB — one per copy cost 25% FPS).
   Guest-visible fences and queue submission wait for them (`Scheduler::WaitHostCopies`),
   which keeps the fix for UI flicker (the guest reused buffers before deferred copies ran).
2. Copies then ran at 0.2–0.4 GB/s per thread, almost all in the kernel: the first CPU access
   to a new staging block makes the kernel allocate and clear it (~2 ms per 16 MiB), and the
   staging pool freed blocks after 3 s idle, between streaming bursts. It now keeps 512 MiB
   (`GOW3_STAGING_KEEP_MB`), frees the rest after 30 s and populates 128 MiB at startup.
3. Write faults: a 256 KiB unprotect window (`GOW3_FAULT_WINDOW`) halves them again.
   `GOW3_UFFD=1` tracks writes with userfaultfd write-protection instead of mprotect (no
   address-space write lock, no mapping splits); read protection for readbacks still uses
   mprotect. It removes the mprotect time but did not change the stalls measurably; opt-in.
4. File reads into write-protected guest pages failed with EFAULT (kernel copies do not reach
   the fault handler); reads now touch each destination page first.

Result: stalls are mostly 40–50 ms (GPU thread ~30 ms of draw work plus ~12 ms of copies)
instead of 60–170 ms; the area load frame 350 ms instead of 430–760 ms.

## Step 2, first slice: resource sharps on the workers (2026-09-30)

A prepared stage now also carries the sharps its worker read from the flattened user data:
every T# with its texture-description hash, every S# and V# (`PrepareResources`,
`PreparedStage::image_sharps` and siblings, in the arena of the submission). The GPU thread
uses them when `PipelineCache::UsedPrepared()` reports that the draw's pipeline came from the
prepared draw — the flattened data was then compared word for word, and sharps depend on
nothing else. Resource list sizes are checked per stage. Toggle 4096 switches it off.

A/B in one run, standing still, `GOW3_FPS_LIMIT=0`, FSR 4, 20 s phases: 69–70 FPS with the
prepared sharps, 66.5–67 without (+3%), same image.

Upscaler costs removed from the GPU thread before this (perf, DWARF call graphs): per-draw
driver format queries in `SceneTargets::Eligible` (~13%), direct recording forced by
`CommandBuffer()` in object motion and the reactive mask, index-list scans for object motion
(~10%, now `Motion::IndexRangeCache`).

What remains on the GPU thread is mostly work on shared cache state: texture binding ~23%
(FindView ~5.6%, UpdateImage/Track/Touch ~5%, barriers), buffer binding ~12% (ObtainBuffer
~11%), render targets ~8%. The next step is a draw-level split: the GPU thread keeps decode,
cache mutation and barriers; a second ordered stage builds descriptor writes and vertex
input state from the resolved handles.

## Scaling to the available threads (2026-09-30)

Draw preparation used to replay the whole command stream in every worker and was switched
off below 12 hardware threads (Steam Deck: no workers). Now:

- One scanner (`bb:DrawScan`) replays the register writes in order and stores, per buffer,
  its starting checksum and a delta of the 32-word register blocks it wrote
  (`AmdGpu::RegDirty/RegDelta`, recorded by `ApplyGraphicsRegisterPacket`).
- Workers (`bb:DrawPrepN`, half the hardware threads in the affinity mask, 1..8) claim the
  nearest scanned buffer ahead of the GPU thread, reach its starting state by applying the
  deltas since their last buffer (or from the queue's tail state), and prepare its draws.
  More workers now mean more buffers prepared in parallel, not more duplicated replay.
- All helpers are SCHED_IDLE (`gow3_threads.h`): they only take idle cores. If the scanner
  starves (busy CPU), the GPU thread rebases it from its own register state once the lag
  passes 64 buffers instead of queueing without bound (`scanner rebases` in the stats).
- Copy threads use the same affinity count (critical path, normal priority).

A/B in one run (toggle 8192, standing still, `GOW3_FPS_LIMIT=0`, FSR 4), ~98% of direct draws
prepared, no rebases:

| CPU | with preparation | without |
|---|---|---|
| 16 threads, 8 workers | ~74 FPS | ~57.7 FPS |
| `taskset -c 0-3,8-11` (4 cores / 8 threads, Deck-like), 4 workers | ~68 FPS | ~53 FPS |

The previous design (4 replaying workers) gave ~69 FPS at the same spot on 16 threads, and
none on 8.

## Vertex inputs on the workers (2026-09-30)

`PreparedDraw::vertex` holds, for the dynamic vertex input path, the attribute and binding
descriptions (`GetVertexInputs`), the V# of every stream, the stream memory merged into
ranges with each stream's range index, and the XXH3 of the streams (object motion). The GPU
thread only obtains the buffers of the merged ranges and records the bindings. Used when the
prepared draw's pipeline was taken, the attribute count matches the pipeline's fetch shader
and no frame capture runs; toggle 4096 switches it off together with the sharps.

A/B (standing still, `GOW3_FPS_LIMIT=0`, FSR 4, 16 threads): ~69.3 FPS on, ~65.7 off (+5.5%;
the sharps alone gave +3%). Screenshots identical.

Render targets were looked at and left on the GPU thread: their descriptions are already
memoized per slot (key copy + compare), the size hint (`last_cb_extent`) is GPU-thread state
rather than register state, and the rest of `BeginRendering` is view lookup, barriers and the
reduced-resolution proxies — all on shared mutable state.

## Engine short paths: investigation (2026-09-30, in progress)

- The measured eboot has no symbols but links Sony's Gnmx (`sdk\target\src\gnmx\gfxcontext.cpp`,
  `lwgfxcontext.cpp`); the game's own engine sits on top, with middleware for the
  post-processing.
- `GOW3_BUFFER_STATS=1` (buffer_cache.cpp) prints buffer bindings per guest region every 5 s.
  Almost all traffic comes from the engine's frame ring, ~0x1043400000–0x1049xxxxxx inside the
  2.4 GB direct allocation at 0x1042c00000: ~400k small constant copies/s (~190 MB/s) and
  ~850 MB/s of arena re-uploads after CPU writes (~13 MB per frame).
- Candidate short path: import that ring into Vulkan (VK_EXT_external_memory_host) so the GPU
  reads it in place — no copies, no page tracking, far fewer GPU-thread operations.
  Blocker to resolve first: EOP fences are signalled when the GPU thread records the packet,
  not when the GPU executes it, so the guest may rewrite ring data still unread by the GPU.
  With ~100 MB of ring and ~13 MB per frame the wrap is ~7–8 frames; the GPU's lag is bounded
  by the presenter's frame pool (`present_frames`, swapchain image count). Next: measure the
  ring's wrap period per frame and the real GPU lag, then prototype the import behind a toggle.

### Measurements for the frame-data window (GOW3_BUFFER_STATS=1)

- Reuse distance of 64 KiB blocks in 0x104xxxxxxx: overwhelmingly 1 frame (then 2). The engine
  rewrites the same memory every frame; there is no long ring.
- GPU lag when the GPU thread starts a frame (`GPU lag at frame start`): almost always 0 —
  the GPU has finished the previous frame. But the engine writes the next frame's data while
  the GPU still executes the current one, and EOP fences are signalled at record time.
  **Reading this memory in place (VK_EXT_external_memory_host) is therefore unsafe** unless
  fences wait for real GPU completion, which costs the guest (it waits on them). Dropped.
- Per second in the hot window: ~370k small constant copies (~179 MB, ~480 B each) and ~3.6k
  arena bindings covering ~13.8 GB (≈3.8 MB per binding) of which only ~516 MB are re-uploaded.
  Copying bound ranges instead of tracking pages would multiply the traffic by ~27 (why the
  "hot pages" attempt fell to 33 FPS); page tracking is the right mechanism there.
- Remaining candidates: per-submission snapshots of the constant area (fewer, larger copies
  instead of ~6000 per frame) and trimming vertex-buffer bindings to the index range a draw
  uses (the V#s cover whole vertex pools, so each binding walks ~950 tracked pages).

### Tried and reverted: per-epoch constant chunks

Small read-only constants served from one snapshot per chunk and queue-task epoch (the epoch
changed at every task entry and resume, so a chunk only served data written before its copy).
A/B in one run, standing still: 32 KiB chunks 82/82/79 FPS vs 85/84/84 without (slower: the
extra copied bytes cost more than the saved operations); 8 KiB chunks 85/85/85 vs 85/83/85
(no difference). The per-binding constant copies are not what limits the frame; the change
was removed. `GOW3_BUFFER_STATS=1` (bindings per region, reuse distance, GPU lag) stays.

State after this work, standing still, `GOW3_FPS_LIMIT=0`, FSR 4: 82–85 FPS; GPU command thread
~90% of a core, recording thread ~92% (mostly its spin), GPU ~70% busy.

### Texture binding: repeated sets and the UpdateImage fast path

- Measured and dropped: only ~13% of draws bind exactly the textures and samplers of the
  previous draw in every stage (running through the test level), so skipping whole texture sets
  would save at most ~3% of the GPU thread.
- `TextureCache::UpdateImage` runs for every texture binding; for a clean, registered image
  already tracked and touched in this GC period it only took the texture-cache mutex (shared
  with the guest threads' fault handlers). It now returns without the lock in that case,
  reading the flags atomically (toggle 1073741824 = 1 << 30 restores the locked path).
  A/B, 8 phases of 20 s: 81.8 vs 80.2 FPS mean (+2%), 3 of 4 pairs ahead; the game window
  was partly covered by other applications during the run, so treat it as indicative.
- The A/B script now records the log line at each phase switch and prints per-phase means.

## LTO and PGO (2026-09-30)

`build.sh` builds `libgow3gpu` with LTO (`-flto=auto`, also for sirit and FSR-Vulkan linked
into it) and, when `pgo/` holds a profile, with `-fprofile-use` (`-fprofile-partial-training
-fprofile-correction`; functions changed since the profile compile without it). No `-march`:
the same build runs on the Steam Deck.

Collecting a profile: `GOW3_PGO=generate bash run.sh` builds an instrumented library
(`-fprofile-generate -fprofile-update=atomic`) that writes `pgo/` every 30 s (`bb:pgo` thread:
`__gcov_dump` + `__gcov_reset`, since the game often ends through `_exit`); play a few minutes
of ordinary gameplay. The next plain build uses it. `GOW3_PGO=off` / `GOW3_LTO=OFF` disable them.
Regenerate the profile after larger code changes.

Comparison, three runs in game (median of in-game 5 s windows, >800 draws/frame), with the new
`Frame stats` field "GPU thread us/draw" (CPU time of the GPU command thread per draw, which
tolerates small scene differences better than FPS):

| build | FPS | GPU thread µs/draw |
|---|---|---|
| no LTO, no PGO | 73.2 | 7.72 |
| LTO | 74.4 | 7.58 (−1.8%) |
| LTO + PGO | 76.3 | 7.35 (−4.8%) |

## Second pass: where the GPU thread's time goes, and a texture helper (2026-09-30)

Standing in the test scene, FSR 4, `GOW3_FPS_LIMIT=0`, ~85 FPS, ~1610 draws/frame.

**On-CPU profile** (perf, direct children): `Draw` 74% — `BindResources` 32% (textures 16%,
buffers 13%), `BeginRendering` 8%, `GetGraphicsPipeline` 5%, `BindVertexBuffers` 4.5%,
`ResetBindings` 4%, dynamic state 2%; `DispatchDirect` 6%; PM4 decode and register hashing ~9%.
No function above ~5% self time: the cost is cache misses spread over the texture, buffer and
barrier structures (`slot_images[id]`, image descriptions, backing state, set writes).

**Wall time** (new `Frame stats` fields): the GPU thread is on the CPU 91% of the time and
waits for guest submissions 0.5%. About 9% is blocked in `Scheduler::WaitHostCopies`: ~130
times per frame (EOP/EOS events, `WriteData`, submissions) it waits for the small guest
copies queued in the recording stream, i.e. until the recording thread has recorded every
command before them, plus ~4% for the copy threads (`Gow3Copy::WaitAsync`). The recording thread
itself works ~40% (the rest is its spin); it is not the limit.

### Texture binding on a helper thread (opt-in: `GOW3_TEXTURE_HELPER=1`, toggle 1 << 22)

`bb:TexBind` binds a draw's textures while the GPU thread binds its buffers and resolves its
vertex/index buffers (split into Resolve/Emit, toggle 1 << 23); the GPU thread joins before
`BeginRendering`. The helper takes only the memoized path (FindImage memo valid, image clean,
no scene-target proxy, no upscaler redirect, no storage images, no mip arrays, no storage
buffers in the pipeline); anything else is bound after the join. The GPU thread joins before
it changes image state (`Runtime::BeforeImageAccess` in `Transit`, `FlushBarriers`,
`SetBackingSamples`, and in `SynchronizeMemoryFromImage` once a texel buffer aliases an image).
68% of draws were bound in parallel, the image was unchanged — and the frame rate too:
85.9 FPS with the helper vs 85.8 without (A/B, 6 × 16 s).

Instrumented with `rdtsc`: the helper's task took ~6200 cycles per fork where the same work
cost ~4600 on the GPU thread, and the GPU thread still waited ~2500 cycles per fork in the
join. The descriptor infos, image states and binding flags the helper writes are read by the
GPU thread right after, so each draw moves them between cores; the GPU thread's own work got
slower by about what it handed over. Per-draw fork/join over ~7 µs of cache-bound work does
not pay on this CPU; the helper stays opt-in for other CPUs.

### Other attempts

| change | A/B | kept |
|---|---|---|
| FindView memo in the image description cache (1 << 21) + write prefetch in record chunks (1 << 20) | 81.5 vs 80.8 FPS | yes |
| format check by table instead of `magic_enum::enum_contains` (a linear scan per texture per draw); hot `Image` fields (flags, binding, tracking range, backing, ticks) moved in front of `ImageInfo` | within noise | yes (no downside) |
| host copy queue: small copies in a lock-free MPMC queue, run by the idle recording thread, the rest by the GPU thread at the wait | blocked 9.5% → 4.8%, but 86.5 vs 87.2 FPS: the copying moved onto the GPU thread | no |
| no barrier tracking for read-only stream buffer ranges | 85.9 vs 87.4 FPS, both phase orders (cause not found) | no |
| small host copies batched, one recorded command per 32 | 86.4 vs 86.2 FPS | no |

Fixed on the way: a draw-preparation worker could stop the process with
`SurfaceFormat: Unknown data_format=15` — it read a V# the guest was still writing and
`SurfaceFormat` asserts. Workers now use `TrySurfaceFormat` and leave such draws to the GPU
thread. It showed up with the slower PGO-instrumented build.

PGO profile regenerated for the changed code (camera rotation only, merged with the old one).
Before/after, as a user builds them (37c8eac with its profile vs this state with the new one),
three alternating restarts × 50 s: 86.9 vs 86.4 FPS, 6.47 vs 6.51 µs/draw — no change beyond
the run-to-run spread (±1.5 FPS).

### What would scale

Moving work to another core per draw costs about what it saves, because the data is shared.
Scaling needs splits where each thread owns its data for long stretches:

1. **Two-stage pipeline.** A second thread owns the texture cache and image state
   (`PrepareRenderState`, `BindTextures`, `BeginRendering`, barriers, descriptor writes, dynamic
   state, draw recording) and runs a draw behind the GPU thread, which keeps decode, pipelines
   and the buffer cache. The second thread cannot read `liverpool->regs` (the GPU thread is
   already on later packets): it keeps its own register copy from the per-buffer deltas the
   draw scanner already records (`AmdGpu::RegDelta`). Packets that touch memory or images
   outside draws (DMA, `WriteData`, EOP/EOS, dispatches, fast clears) drain the pipeline.
   Estimate: the second stage takes ~30% of today's GPU thread work, less the drains. Large,
   delicate refactor.
2. **Less work per draw.** A frame-to-frame memo of a stage's resolved textures (image ids,
   views, samplers) keyed by the prepared T#/S# hashes and validated by the registry
   generation, image cleanliness and layouts, instead of per-texture lookups.
3. **The copies before fences** (~9% blocked): keep them off the GPU thread's critical path,
   e.g. by letting the idle draw-preparation workers drain a copy queue.

## Two-stage draw pipeline (2026-09-30)

`vk_draw_pipe.h`. The GPU command thread (stage A) keeps PM4 decoding, the register file, the
constant engine and pipeline selection. A direct draw or dispatch becomes a packet in a 16 MiB
ring: the 32-word register blocks written since the previous packet (`Liverpool::pipe_dirty`,
marked by `ApplyGraphicsRegisterPacket`), the CB/DB size hints, each stage's user data, flattened
user data and program base, and the draw parameters (for a dispatch the `ComputeProgram`).
The draw recording thread `bb:DrawRec` (stage B) applies the blocks to its own register copy and
runs the rest of the draw (`DrawRecord`, `DispatchRecord`): textures, buffers, render targets,
barriers, descriptors, dynamic state, recording. While packets are in flight stage B owns the
texture/buffer caches, the runtime, the scheduler, the scene targets, the upscaler and the motion
state.

- Stage B reads registers through `Rasterizer::Regs()/CbExtent()/CsRegs()` (its copy there,
  Liverpool's elsewhere) and shader user data through `Shader::Info::UserData()/FlatUserData()/
  ProgramBase()`, which return the snapshot stage B installed for the stages of the current
  draw (`Info::ud_snapshots`, thread-local).
- Everything else stage A does on stage B's state first waits for it to run dry
  (`Rasterizer::DrainDrawPipe`): every public rasterizer entry point on the GPU thread, PM4
  packets other than register writes/draws/dispatches/fences (`PipelinedOpcode`), pending
  commands (`ProcessCommands`), compute queue packets, `DumpConstRam`.
- End-of-pipe/-shader events run in order on stage B (`Rasterizer::RunInOrder`). A
  `WaitRegMem` on a fence value handed to stage B counts as met (`Liverpool::pending_fences`):
  stage B runs everything in stream order anyway. Unmet waits drain stage B, then yield.
- The EOP fence is signalled by the Vulkan recording thread after the guest memory copies queued
  before it (`Scheduler::SignalAfterHostCopies`), so stage B does not wait for them.
- A submission's prepared draws stay alive until stage B has passed them
  (`RetireSubmission`) instead of a drain at its end.
- Faults: stage B handles its own inline, like the GPU thread (`IsGpuSideThread`); the GPU
  thread drains stage B before handling one inline. With userfaultfd, faults of the GPU thread
  take the locked path of guest threads.
- `DmaData` to 0x3022C (skipped by the handler; ~70k/s) does not drain.
- `GOW3_PIPE_VERIFY=N`: every Nth packet also carries the full register file and stage B reports
  words where its copy differs (none seen in game, menus included).
- `GOW3_DRAW_PIPE=0/1` overrides the default (on with 8+ hardware threads). The toggle mask is 64
  bits now (bits 20-29 are raw debug toggles of the motion vectors and the upscaler, which the
  first measurements below also flipped): 1 << 37 whole pipeline, 1 << 38 fences on stage B,
  1 << 39 WaitRegMem on pending fences, 1 << 40 dispatches, 1 << 41 fences signalled by the
  recording thread, 1 << 42 WriteData/DmaData/special draws/flip IRQ on stage B, 1 << 36 constant
  ring. `Frame stats` add a `Draw pipe` line: draws, drains that waited and why, stage A waiting,
  stage B busy.

Results (the test scene, standing, FSR 4, `GOW3_FPS_LIMIT=0`, A/B in one run):

| | pipeline on | off |
|---|---|---|
| 16 threads | 96.1 FPS | 80.6 FPS (+19%) |
| 4 cores / 8 threads (`taskset -c 0-3,8-11`) | 83.8 FPS | 71.0 FPS (+18%) |

Steps on the way (FPS in the same scene): first version, drains at every non-draw packet —
79.8 (drained ~80k/s, almost all no-op `DmaData`); skipping those — 89; fences on stage B and
lazy `WaitRegMem` — 92; dispatches handed over — 93; fences signalled by the recording thread —
103.5 (A/B of that step alone: 103.5 vs 94.9).

Now stage B is the limit (~85-90% busy, the draw path as profiled above), stage A waits most of
the time, and the GPU is ~80% busy with FSR 4. Remaining drains: `WriteData` (~30/frame: 192
zero bytes to a frame buffer and a 4-byte label, whose readers are unknown), non-trivial
`DmaData`, indirect draws. Next: move work from stage B to stage A — the buffer side of a draw
(ObtainBuffer, uploads) needs stage A to own the buffer cache and hand its commands to stage B.

### Second round (2026-09-30)

- `WriteData`, `DmaData`, the flip IRQ after them (the buffer label is a `WriteData`) and draws
  `FilterDraw` handles itself (fast clear elimination, resolve, depth copy) run in order on
  stage B. Drains: ~7 per frame (DumpConstRam, indirect draws/dispatches).
- **Constant ring** (`vk_constant_ring.h`): stage A copies small read-only guest buffers (the
  stream path of `ObtainBuffer`, and the flattened user data) into a 32 MiB ring of its own;
  stage B only binds them. A region is reused once the submission stage B recorded its last
  draw in has completed (stage B stamps packets with the submission tick). Buffers overlapping
  guest memory that queued work will write (storage buffers, DMA, WriteData, fences:
  `NotePendingGpuWrite`) or GPU-modified memory stay with stage B.
- `GOW3_PIPE_VERIFY` also re-walks each stage's resource tables on stage B and compares them with
  stage A's snapshot (guarded against faults: pointers may be stale by then). Pixel shaders whose
  `Info` the pipeline selection does not refresh (no user data) are skipped.

A/B in one run (16 threads, FSR 4 Ultra Performance, clean toggle bits): whole pipeline
110.9 vs 83.9 FPS (+32%); constant ring 114.2 vs 102.5 FPS (+11%).

### GPU time per frame vs upscaler preset

The GPU (RX 7800 XT) now limits more than the CPU. Presets change it little:

| mode | FPS | GPU busy | GPU ms/frame |
|---|---|---|---|
| upscaler off | 132 | 76% | 5.8 |
| FSR 3 Native AA | 116 | 76% | 6.6 |
| FSR 3 Ultra Performance (scene 640x360) | 114 | 71% | 6.3 |
| FSR 4 Quality | 115 | 88% | 7.7 |
| FSR 4 Ultra Performance | 114 | 84% | 7.4 |

The reduced scene targets are used (1160 of ~1530 scene draws per frame), but rasterizing the
scene costs little on this GPU: a ninth of the pixels saves ~0.3 ms. The rest does not depend on
the preset (shadow maps, full-resolution post-processing and UI, FSR itself — FSR 4 costs
~1.1-1.4 ms more than FSR 3 —, and emulation overhead: barriers, copies, resampling). FSR 4 at
Native AA fails to start ("no free provider frame").

### GPU profile (`GOW3_GPU_PROFILE=1`, `vk_gpu_profiler.h`)

Timestamps where each render pass, dispatch, upscaler run and submission end starts; the time to
the next one is charged to it (barriers and copies in between included; "between submissions"
is mostly the GPU waiting for the CPU). Printed every 5 s, GPU ms per frame by label. Timestamps
are written outside render passes (`radv_CmdWriteTimestamp2` crashed inside some) and only into
the rasterizer's scheduler (the presenter has its own).

the test scene, FSR 4 Ultra Performance, ~110 FPS (8.8 ms/frame):

| label | ms/frame |
|---|---|
| GPU idle between submissions | 2.1 |
| guest copy shader `fefebf9f`, 57 dispatches (HLE) | 2.1 |
| FSR 4 | 1.5 |
| guest compute `3d5ebf4e`, 8 dispatches | 0.5 |
| G-buffer passes at 640x360 | ~0.8 |
| the rest (post-processing, UI, smaller passes) | ~1.8 |

This is why presets barely change the GPU load: only the scene passes scale.

The copy shader is HLE'd (`vk_shader_hle.cpp`) as `vkCmdCopyBuffer` with ~1024 small regions
per dispatch. `buffer_multi_copy.comp` now does a batch in one dispatch (toggle 1 << 43):
the copies themselves 0.4 ms/frame; with the buffer preparation before them the label fell from
2.1 to 1.6 ms/frame. Most of the rest is `ObtainBuffer` over the merged ranges (copies of a few
KiB spread over up to 57 MiB of destination): synchronizing only the copied parts saved another
~0.75 ms of GPU time but cost more on the CPU (page protection per part, no stream path for
small sources: 108.7 vs 114.3 FPS, sources only 105.7 vs 111.9), so it was dropped. Frame rate
unchanged with the multi-copy shader (CPU-bound here); it helps where the GPU limits.

### Render state memo (toggle 1 << 44)

~90% of draws continue the render pass the previous draw opened, yet `BeginRendering` redid
the target lookups, transitions, scene-target proxies and upscaler redirects for each. Now a
draw reuses the previous render state when the scheduler still has that exact pass open (nothing
broke it: barriers, copies and dispatches end passes), the inputs match (target ids and views,
pipeline attachment key, scene/raster scaling and upscaler redirect state, image registry
generation, depth control), no clear is requested, and no target is also sampled by the draw.
States with clears are not remembered (the next draw's differs). 90% hits; 122.1 vs 111.8 FPS
(+9%); screenshots on/off differ no more than two taken in the same mode (animated scene).

### Texture set memo (toggle 1 << 45)

A stage's resolved textures (image after the depth redirect, view, backing, subresource range)
remembered by the prepared T# hashes, 8192 slots, sets of up to 16 sampled images. A hit
requires the same image registry generation, the same backings, no rebind, images up to date,
no render-target feedback and no upscaler redirect; it then only redoes the per-draw effects
(found tick, binding flags, bound list, layout transition, usage) and writes the descriptors.
84% hits; 128.6 vs 121.8 FPS (+5.6%); screenshots on/off within the scene's own noise.

### Copy shader merge distance

The HLE merged copies whose ranges fit in 64 MiB, and each merged batch synchronizes its whole
source and destination range. With 64 KiB (`GOW3_COPY_MERGE_KB`): the copy shader's GPU time
1.5 -> 0.6 ms/frame (profiler), GPU busy 85% -> ~77%, frame rate not lower (restarts vary
125-142 FPS; 16 KiB and 256 KiB similar).

## Third round (2026-09-30, afternoon)

Frame rate at the level entrance, 16 threads, FSR 4 Ultra Performance: ~134 -> ~145-150 FPS
(restarts vary by a few FPS; the A/B numbers below are from one run each).

### Where stage A waits

`Frame stats` now prints, next to the drain counts, the share of stage A's time each drain
reason and each call site (`function:line`, `DrainDrawPipe` records `__builtin_LINE`) waited.
Stage A waits ~25% of its time, almost all at one drain per frame: whichever drain comes first
after a long stretch of draws waits for stage B to finish them. It was `DrawIndirect` (21%),
then `DispatchIndirect`, then a `DumpConstRam` once those were pipelined. So stage A is faster
than stage B, and removing drains helps only where it breaks the lockstep; stage B (85% busy,
~2.8 µs per packet, flat profile) is the limit.

- Indirect draws and dispatches are pipelined like direct ones (toggle 1 << 46): the pipeline is
  selected on stage A, the argument buffer lookup and the indirect command are recorded on B.
  +1.4%.
- Pending GPU writes (the constant ring's guard) merge overlapping ranges and are pruned every
  64th check instead of on every buffer: `PendingWriteOverlaps` 7.5% -> 3.6% of stage A; together
  ~+4%.
- Texture set memo: 32768 slots; a quarter of the misses were slot collisions (37k -> 11k per
  5 s): +1.3%.
- Motion history per-frame tables: open addressing instead of `std::unordered_map` (four node
  allocations per stored draw, freed every frame). Neutral on FPS.
- Tried: a release store instead of the sequentially consistent one in `DrawPipe::Commit` with a
  timed futex sleep (+0.4%, not worth a possible missed wake-up; reverted).

Object motion vectors cost ~10% FPS (155 vs 141 FPS with `object_motion=0`): stage B spends
~0.28 µs more per packet, spread over DrawRecord (bone palette hashes, index range cache, motion
pipeline variants).

### Scene resolution and the preset/GPU load question

GPU busy time per frame (profile total minus the idle time between submissions):

| mode | GPU busy |
|---|---|
| upscaler off (native 1080p) | 3.67 ms |
| FSR 4 Quality | 5.64 -> 5.50 ms |
| FSR 4 Ultra Performance | 5.11 -> 4.98 -> 4.86 ms |

Rasterizing the scene costs little on this GPU, so a lower preset saves ~0.5 ms, while FSR 4
itself costs ~1.4 ms at 1080p output; FSR 4 Ultra Performance takes more GPU time than native
rendering without an upscaler. Found on the way:

- The reduced-size decision looked at all eight color slots, but slots past the mask width keep
  earlier passes' targets — in the lighting passes the G-buffer images they sample. The light
  accumulation passes stayed at 1920x1080 at every preset. Fixed (toggle 1 << 47 restores it).
- Every pass that sampled a reduced target made the proxy be resampled to the native size first
  (~25 resolves per frame, 0.4 ms; `GOW3_GPU_PROFILE` now labels resolves, fills, image uploads and
  downloads). The recompiler marks images read by anything but normalized sampling without
  offsets (`ImageResource::needs_native`); other sampled bindings read the proxy directly
  (toggle 1 << 48): GPU busy 5.02 -> 4.86 ms, identical screenshots. Changes the `Info` layout:
  shader meta version 7, pipeline key version 5 (caches rebuilt once).
- `GOW3_SCENE_DEBUG=<file>`: touching the file prints the next frame's scene passes with the reason
  each keeps the native size. What remains native before the upscaler: half-resolution
  (960x540) passes, which `SceneTargets::Eligible` does not handle.

Next GPU item (matters most on the Steam Deck): guest compute `3d5ebf4e` is a dword memcpy that
copies render target memory (the 1080p depth buffer, 12 MB, sampled afterwards as R32F; a
G-buffer target; two 960x540 targets): 8 dispatches, and each needs the image downloaded into
the buffer, the proxy resolved and the destination image uploaded again. Recognizing copies
whose source is an image and turning them into image copies (or a proxy-sized copy) would remove
most of that. Skipping the dispatches blacks out the scene, so the copies are needed.

### Fixed: guest heap corruption with the draw pipeline (2026-10-01)

Cause: GPU idle (`IrqC GpuIdle`, which releases `sceGnmSubmitDone`) was signalled once stage A had
decoded every submission, while end-of-pipe fences deferred to the Vulkan recording thread
(RecorderFences) were still pending; the guest freed the objects holding those labels while
another of its threads still updated them. Stage A now drains the pipe and waits for the
deferred signals before GPU idle (and before compute-queue WriteData/ReleaseMem). Found with
`GOW3_WRITE_LOG=2` (fence targets logged at decode time, off the racing path). The notes below
are the investigation.

#### Investigation notes

After ~2-15 min at the level (camera turning, nobody moving) the guest faults at guest offset
`0x263b8e7`: a free-list pop in a guest allocator reads the next pointer `0x0000005300000000`
from a freed block, so something wrote into memory the game had already freed. Soak runs of
15-20 min: with the draw pipeline 3 of 4 crashed (at 863, 844 s and one earlier), with
`GOW3_DRAW_PIPE=0` 0 of 2. With `GOW3_WRITE_LOG=1` (slower downloads) one run survived 20 min.

Ruled out / done so far:
- guest-visible writes overtaking deferred fences (now ordered: `WaitDeferredSignals`, toggle
  1 << 49) — the crash remains;
- the scene-proxy and texture-set changes: the crash also happened before them.

Candidates: late writes into guest memory that the pipeline delays further — asynchronous image
downloads (`TextureCache::DownloadImageMemory`, deferred until the GPU finishes, whole image),
buffer downloads on page faults, and fault handling when the GPU command thread (stage A), which
now reads guest memory for the constant ring, is not treated as a GPU-side thread
(`IsGpuSideThreadId` accepts only stage B). Next: a run with `GOW3_WRITE_LOG=1` that crashes
prints which logged write landed near the corrupted block; bisect the pipeline toggles
(38 tasks, 39 pending fence waits, 41 recorder fences, 42 memory writes, 36 constant ring).
