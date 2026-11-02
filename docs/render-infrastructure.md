# Render Infrastructure — Parts 1 and 2

`nord_render` starts the graphics infrastructure only. It knows about a native
presentation surface and GPU resources, not Nord's voxel world, camera, game
rules, or scene. `src/render` owns the game's clear color and decides when the
application asks the renderer to present. `nord_app` is the game-facing facade;
the game does not depend on `nord_render`, `nord_platform`, or native handles.

## Current implementation

The initial Windows backend uses Direct3D 12 and DXGI:

- enables the D3D12 debug layer in debug builds when it is available;
- selects a hardware adapter that supports feature level 11.0;
- owns one direct command queue and command list;
- creates a three-buffer flip-discard waitable swapchain, caps queued presentation
  latency at two frames, and builds an RTV descriptor heap;
- has two frame-resource slots (one fewer than the swapchain buffers), each with its own command allocator, upload
  arena, and fence retirement value;
- keeps a 64 KiB persistently mapped upload heap in each frame slot. Its linear
  allocator applies alignment, resets only after that slot's fence completes,
  and is used for the frame's clear-color data;
- transitions the swapchain buffer from `PRESENT` to `RENDER_TARGET`, clears it,
  transitions it back, submits the command list, and presents with vsync by
  default;
- waits for outstanding GPU work before resize and renderer/window shutdown;
- waits for DXGI frame readiness before input sampling and simulation updates,
  then submits the freshest available frame data.

Swapchain images remain owned by DXGI. The initial transient upload path uses
committed upload buffers. There is no geometry or 3D rendering in Part 1.

## Part 2: resource-system contracts

The resource-system API now lives in isolated modules under `nord_render` and
is re-exported to the game only through `nord_app`:

- `allocator` provides alignment-aware, best-fit suballocation for device-local
  buffer, non-render-target texture, render-target texture, upload, and readback
  heap classes, with per-class preferred block sizes, block reuse/coalescing,
  and live/peak/reserved counters. The app's initial policy uses smaller
  preferred heaps for buffers, render targets, upload, and readback; a larger
  resource grows its own block to the required size. D3D12 supplies the exact
  allocation size and alignment before a resource enters a placed heap. Heap
  classes separate RT/DS resources from ordinary resources for tier-1 hardware.
  Before creating a new native heap, the backend queries the DXGI local/non-local
  process budget, trims excess empty heaps, re-queries, then rejects a reservation
  that would still exceed it. The app facade can query the current budget/usage
  snapshot and explicitly trim excess empty pools; trim waits for GPU idle and
  retains one empty heap per allocation class for reuse.
- `resources` defines validated buffer, texture, and render-target descriptions,
  format/usage enums, and opaque generation-bearing resource handles. Buffers,
  2D/array textures, and color/depth render-target attachments are allocated as
  placed resources on native D3D12 heaps. Device-local, CPU-write upload, and
  GPU-write readback buffers have distinct heap/state rules. Upload writes and
  readback reads validate ranges; readback waits for queue idle. Destruction
  likewise waits for queue idle before recycling the heap range, which is
  conservative until per-resource last-use fences are tracked by command submit.
- `descriptors` has separate fixed-capacity slot spaces for resource, sampler,
  RTV, and DSV descriptors, backed by stable native heaps. Texture SRVs, CBVs,
  raw-buffer/2D-texture/texture-array UAVs, RTVs, DSVs, and point/linear/anisotropic samplers
  can be created. Resource and sampler arrays can be created as all-or-nothing,
  contiguous descriptor ranges; generation checks reject stale slots. Explicit
  descriptor retirement removes the handle from its owning resource's cleanup list.
- `shader` invokes DXC with explicit stage/profile, include paths and defines,
  returns diagnostics and bytecode, preserves DXC's binary reflection artifact
  and assembly dump, and extracts resource names/registers/spaces/counts through
  DXC's structured reflection COM API on Windows. The fallback parser rejects
  unknown binding rows instead of silently dropping them. Shader IDs use
  domain-separated 256-bit BLAKE3. `NORD_DXC_PATH` selects a pinned compiler;
  `NORD_DXCOMPILER_PATH` can select its matching `dxcompiler.dll`.
- `pipeline` validates immutable attachment/state descriptions, builds root
  descriptor tables from the reflected vertex/pixel bindings, creates a native
  D3D12 root signature and graphics PSO, maps input layouts and fixed-function
  states, and caches the native state by stable shader content IDs plus state.
  It also builds/caches compute root signatures and PSOs from compute-stage
  reflection. Old native PSOs can be retired by frame after a conservative
  queue-idle wait.
- `RenderServices` owns allocator, descriptor and pipeline bookkeeping for the
  renderer lifetime. It stays private to the renderer implementation;
  `nord_app::ApplicationRenderer` exposes operation-specific resource methods
  and a GPU allocator statistics snapshot without handing `src` the service
  container.

The native object path is now exercised through `ApplicationRenderer`: create
resources and views, compile/reflect HLSL, create cached root signatures/PSOs,
bind reflected CBV/SRV/UAV/sampler slots, clear offscreen color/depth
attachments, issue a triangle draw and compute dispatch, and synchronously read
back pixels. The bootstrap immediate-pass APIs are backend-neutral and live in
`nord_render`; `nord_app` owns the game-facing queue facade. The render graph
infers read/write hazards, builds deterministic pass order, and plans
transition/UAV/aliasing barriers. It calculates transient texture lifetimes
and groups non-overlapping textures with compatible D3D12 heap classes. Each
group shares a GPU range sized to the maximum device-reported allocation size
and alignment; an aliasing barrier is emitted before each reuse. The runtime
smoke records and submits two compute dispatches against aliased textures; it
also reads back the persistent graphics and compute outputs.

Pass callbacks now receive a backend-neutral command encoder and never borrow
the renderer. The callbacks encode on separate workers; D3D12 then records each
pass's barrier and draw/dispatch command lists concurrently, each with its own
allocator. A graphics pass binds and clears one or more color attachments
(including per-target clear colors) and an optional depth attachment once,
then records any number of draws into the same native command list. Pipeline
descriptions match up to eight pixel-shader color outputs. Each pass can set a
viewport, depth range, and scissor rectangle; omitted values cover the full
target. Draws can
bind validated vertex streams and 16/32-bit index streams and select indexed or
non-indexed ranges. The graph checks that those streams were declared as vertex
or index reads, and the backend checks resource usage, slot layouts, strides,
offsets, draw counts, and byte ranges before recording. The resulting lists are
submitted in compiled graph order to one direct queue. Their allocators stay
alive until the final fence retires. The game uses this path from
`src/render` for its depth-tested opaque cube pass.
Reflected bounded CBV/SRV/UAV/sampler arrays are represented as descriptor
tables; native recording checks exact reflected count, heap class, contiguity,
slot liveness, view kind, and graph declarations before binding the table start.
The smoke shaders exercise two-entry CBV, SRV, sampler, and UAV tables in real
graphics and compute passes, then read back both UAV array outputs.
Barrier command lists retain their compiled-pass indices, including passes with
no barrier work, so worker completion order cannot shift transitions onto the
wrong pass. Before native recording, reflected shader bindings are checked
against live view types, owning resources, and that pass's graph declarations;
attachments must also use views belonging to the exact target.

## Render-resource verification

Run `scripts/verify_render_resources.ps1` on Windows with a D3D12-capable GPU
and DXC available on `PATH`. Add `-Release` to run the same smoke check against
the optimized build. The example exercises real device heaps, buffer upload and
byte-for-byte readback, texture and array-layer uploads, SRV/CBV/UAV/RTV/DSV and
sampler creation including contiguous CBV/SRV/UAV/sampler ranges, DXC compilation/reflection,
graphics/compute PSO creation and
cache reuse, rejection of an undeclared shader resource, a compiled seven-pass
render queue (including an empty slot before barrier-bearing work, two indexed meshes plus one
non-indexed draw in one depth-tested graphics pass, ordered compute
dispatches to two graph textures whose lifetimes alias), descriptor binding in
graphics and compute passes, real UAV and aliasing barriers, GPU pixel and
persistent-UAV readback assertions, independent pixel assertions proving both
meshes remain visible, the nearer mesh wins the overlap, the second color
output is written, and scissor clipping works, resource retirement,
allocator accounting, excess-heap trimming, and zero-size suspension followed
by same-size restoration and repeated swapchain resizing. Allocator,
descriptor-generation, resource-description, and pipeline-cache rules also
have CPU unit tests in `nord_render`.

This verifies resource creation, reflected descriptor binding, native PSO
execution, draw/dispatch output, readback, and lifetime behavior on the machine
running the script. It does not certify every adapter/driver combination or
replace long-running validation, GPU validation layers, or a multi-adapter
matrix. It also does not prove throughput at production frame scale.

## Boundaries and lifetime

The D3D12 descriptor-table contract follows Microsoft's binding model: arrays
use contiguous descriptor ranges and samplers occupy their own heap. See
[descriptor tables](https://learn.microsoft.com/en-us/windows/win32/direct3d12/using-descriptor-tables),
[root-signature descriptor ranges](https://learn.microsoft.com/en-us/windows/win32/direct3d12/creating-root-signature),
and DXC's [reflected `BindCount`](https://learn.microsoft.com/en-us/windows/win32/api/d3d12shader/ns-d3d12shader-d3d12_shader_input_bind_desc).

The initial game draw lives in `src/render`: it owns its HLSL, vertex data,
pipeline configuration, and render-graph pass. The renderer remains generic;
`present_texture` copies a completed surface-sized RGBA8 target into the active
swapchain image, applying and restoring D3D12 resource states around the copy.
This uses the documented [D3D12 `CopyResource` constraints](https://learn.microsoft.com/en-us/windows/desktop/api/d3d12/nf-d3d12-id3d12graphicscommandlist-copyresource)
and [resource barrier rules](https://learn.microsoft.com/en-us/windows/win32/direct3d12/using-resource-barriers-to-synchronize-in-direct3d-12).
The first 3D camera, indexed mesh, transforms, and depth-tested game pass are
documented in [Renderer 3D — Stage 1](render-3d-stage-1.md).
The game's basic opaque material contract lives in `src/render/material.rs`.
Cargo's build script compiles its HLSL to DXIL in `OUT_DIR`, and the executable
embeds the bytecode and reflection artifacts. Runtime loading needs no shader
directory or DXC installation. `nord_render` supplies the generic offline
shader-artifact constructor, resource, descriptor, pipeline, and
command-encoding infrastructure.

`nord_platform` exposes a hidden integration token for a live native window;
`nord_app` passes it directly to `nord_render`. The token never reaches game
modules. The runtime handles normalized resize events. During shutdown the
renderer waits for GPU completion and releases its swapchain before
`nord_platform` destroys the HWND.

The renderer is thread-affine for now: create, render, resize, and destroy it on
the application/window owner thread. GPU submission uses one direct queue;
encoder callbacks and D3D12 pass command lists are recorded on workers.

## Research basis

- [Microsoft: creating a basic Direct3D 12 component](https://learn.microsoft.com/en-us/windows/win32/direct3d12/creating-a-basic-direct3d-12-component) documents the device/queue/swapchain/allocator/list/fence lifecycle and warns that an allocator must not be reset while its GPU work is outstanding.
- [Microsoft: Direct3D 12 swap chains](https://learn.microsoft.com/en-us/windows/win32/direct3d12/swap-chains) documents flip-model swap effects, the present-state requirement, and limiting frames in flight to avoid CPU run-ahead and input latency.
- [Microsoft: DXGI flip-model performance](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/for-best-performance--use-dxgi-flip-model) covers flip-discard and frame-latency waitable objects. The runtime waits on the object before polling inputs and advancing simulation.
- [Microsoft: `SetMaximumFrameLatency`](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_3/nf-dxgi1_3-idxgiswapchain2-setmaximumframelatency) requires the waitable-object flag on swapchain creation; the implementation sets both together.
- [Khronos: Vulkan threading guide](https://docs.vulkan.org/guide/latest/threading.html) describes host-side command recording and separate per-thread command pools. Each D3D12 pass recorder owns its allocator/list pair under the same rule.
- [Khronos: Vulkan memory allocation guide](https://docs.vulkan.org/guide/latest/memory_allocation.html) motivates suballocation over frequent device-memory allocations. The first upload arena is fence-retired and linear; pooled default-heap allocation remains a later part of the renderer.
- [`raw-window-handle` 0.6.2](https://docs.rs/raw-window-handle/0.6.2/raw_window_handle/) provides the cross-platform window-handle interchange types used between PAL and the renderer; the platform-specific implementation stays behind `nord_app`.
- [Microsoft: D3D12 residency and placed resources](https://learn.microsoft.com/en-us/windows/win32/direct3d12/residency) and [`GetResourceAllocationInfo`](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-getresourceallocationinfo) require allocations to respect device-reported size/alignment, and call out distinct heap/resource behavior.
- [`CreatePlacedResource`](https://learn.microsoft.com/en-us/windows/desktop/api/d3d12/nf-d3d12-id3d12device-createplacedresource) requires the heap offset to satisfy the resource alignment and fit the full allocation.
- [Microsoft: shader-visible descriptor heaps](https://learn.microsoft.com/en-us/windows/win32/direct3d12/shader-visible-descriptor-heaps) explains the distinct CBV/SRV/UAV and sampler heaps, hardware limits, and why switching heaps can cause GPU idle waits. This informs the stable, separate descriptor spaces.
- [Microsoft: descriptor tables](https://learn.microsoft.com/en-us/windows/win32/direct3d12/using-descriptor-tables) describes how root signatures reference ranges within heaps.
- [Microsoft: recording command lists and bundles](https://learn.microsoft.com/en-us/windows/win32/direct3d12/recording-command-lists-and-bundles) documents binding and recording commands before queue submission, plus allocator lifetime rules used by per-worker pass lists.
- [Microsoft: executing and synchronizing command lists](https://learn.microsoft.com/en-us/windows/win32/direct3d12/executing-and-synchronizing-command-lists) documents queue ordering, fences, and explicit resource-state ownership.
- [Microsoft: D3D12 input layouts](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/ns-d3d12-d3d12_input_element_desc) defines the vertex stream slot, format, byte offset, and per-vertex/per-instance stepping represented by Nord's pipeline layout.
- [Microsoft: indexed instanced draws](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12graphicscommandlist-drawindexedinstanced) documents the index range and instance parameters used by the command encoder.
- [Microsoft: D3D12 resource states](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/ne-d3d12-d3d12_resource_states) defines the vertex/constant and index buffer states planned by the render graph.
- [Vulkan Guide: synchronization](https://docs.vulkan.org/guide/latest/synchronization.html) describes execution and memory dependencies; the graph models those dependencies from declared resource reads and writes.
- [Microsoft: D3D12 memory aliasing](https://learn.microsoft.com/en-us/windows/win32/direct3d12/memory-aliasing-and-data-inheritance) requires explicit aliasing barriers when placed resources reuse overlapping heap ranges.
- [AMD GPUOpen: Render Pipeline Shaders](https://gpuopen.com/rps/) describes transient resource scheduling and aliasing as graph-level memory optimizations.
- [Microsoft: texture upload and readback](https://learn.microsoft.com/en-us/windows/win32/direct3d12/upload-and-readback-of-texture-data) documents D3D12 copy footprints and CPU-visible readback layouts.
- [Microsoft: D3D12 pipeline state management](https://learn.microsoft.com/en-us/windows/win32/direct3d12/managing-graphics-pipeline-state-in-direct3d-12) documents immutable pipeline state creation and why pipeline variants should be cached.
- [DirectX Shader Compiler API](https://github.com/microsoft/DirectXShaderCompiler/wiki) and [`IDxcUtils::CreateReflection`](https://learn.microsoft.com/en-us/windows/win32/api/dxcapi/nf-dxcapi-idxcutils-createreflection) inform the DXC compiler artifact and reflection boundary.
