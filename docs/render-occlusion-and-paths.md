# Occlusion culling and render path comparison

## Current decision

The game uses the forward PBR path with GPU frustum culling by default. Same-frame
Hi-Z is opt-in: set `NORD_OCCLUSION_CULLING=1` (or `true`) to enable it. An unset
variable, `0`, or `false` keeps the frustum-only path.

No deferred renderer is added at this stage. Occlusion culling is a visibility
decision that can feed either forward or deferred shading. These measurements
do not show a shading or bandwidth limit that would justify changing the
shading path.

## Same-frame conservative Hi-Z

When enabled, the Hi-Z path records these render-graph dependencies in the same
frame:

1. Run the current GPU frustum pass and compact candidates into the existing
   instance buffer.
2. Render depth for those candidates into an R32 target at quarter output
   resolution, with D32 depth testing and depth writes enabled. The indirect
   draw avoids rasterizing instances already outside the camera frustum.
3. Build a depth pyramid. Each compute pass reduces a 4x4 source footprint
   with `max`, including odd-sized image boundaries and out-of-range guards.
   This coarser hierarchy cuts pyramid dispatches while conservatively retaining
   some occluded instances.
4. Reset indirect arguments and counters, then project each source instance's
   conservative world-space bounds. Objects crossing
   the camera plane, with invalid clip coordinates, or with bounds spanning too
   many texels are kept visible.
5. Sample the selected Hi-Z level. Cull only when the object's nearest depth is
   farther than the maximum depth in every covered texel, with a small depth
   bias. The normal GPU frustum test runs first.
6. Compact survivors to the indirect instance buffer and submit the regular
   shadow and forward PBR passes.

The D3D depth convention in this renderer is near=0 and far=1. Therefore the
maximum reduction preserves the farthest depth in a region; uncovered pixels
remain at 1 and prevent unsafe culling. Building from the current frame avoids
using stale visibility after camera movement. When the projection or coverage
test is uncertain, the shader fails open and keeps the object.

The color depth target lets the current generic render graph sample the
prepass output without adding game-specific logic to `nord_render`. The
algorithm and policy are game-owned in `src/render`; Nord's renderer only
provides the resource, descriptor, command encoder, and graph infrastructure.

## Measured A/B run

The comparison script adds eight same-mesh instances behind a central opaque
instance on the camera's view axis. Both paths render the identical probe scene.
Runs used the Windows D3D12 backend at 1280x720; each warmed up for 16 frames
and measured 80 frames.

| Path | Frustum-visible | Hi-Z rejected | Final visible | p50 completed frame | p95 completed frame | Image checksum |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Frustum only | 590 | 0 | 590 | 7.79 ms | 9.06 ms | `5f2c2790964e06d4` |
| Same-frame Hi-Z | 590 | 6 | 584 | 15.64 ms | 16.96 ms | `5f2c2790964e06d4` |

The exact checksum match proves that the six rejected probes did not alter the
captured output in this test. The diagnostic benchmark waits for GPU completion
after each frame, so it compares completed frame cost consistently but
serializes CPU/GPU work; it is not a throughput benchmark for a fully pipelined
production loop. The probe confirms that Hi-Z culls fully hidden instances, but
this small scene costs about 2x as much at p50 as frustum-only while rejecting
only six instances. The result does not establish performance for a
representative voxel world, so Hi-Z remains opt-in until it is measured against
representative scenes, including large opaque occluders and camera motion.

## Reproduce

`scripts/compare_occlusion_paths.ps1` builds the Release executable if
requested and runs both paths with GPU counter validation and the benchmark
enabled. It compares the checksums and prints timings and reject counts.

## References

- Greene, *Occlusion Culling with Optimized Hierarchical Buffering*, SIGGRAPH
  1999: conservative culling may miss hidden objects but must not remove
  visible ones. [SIGGRAPH archive](https://history.siggraph.org/wp-content/uploads/2023/03/SIGGRAPH-1999-Conference-Abstracts-and-Applications.pdf)
- *Advances in Real-Time Rendering*, SIGGRAPH 2008 course notes: max-depth
  reduction, conservative Hi-Z tests, integer address arithmetic, and careful
  odd-sized mip boundaries.
  [Course notes PDF](https://www.realtimerendering.com/advances/s2008/Siggraph2008-AdvancesInRealTimeRendering-CourseNotes.pdf)
- Microsoft, [D3D12 predication](https://learn.microsoft.com/en-us/windows/win32/direct3d12/predication):
  GPU predication can suppress draw/dispatch work based on GPU-produced data.
  This can be evaluated later if indirect-list compaction proves insufficient.
