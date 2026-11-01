# Occlusion culling and render path comparison

## Current decision

The game keeps the forward PBR path as its default. Same-frame Hi-Z is available
as an opt-in diagnostic path using `NORD_OCCLUSION_CULLING=1`; it is not
enabled by default because the current scene did not produce any additional
occlusion rejects and the extra passes increased measured frame cost.

No deferred renderer is added at this stage. Occlusion culling is a visibility
decision that can feed either forward or deferred shading. These measurements
do not show a shading or bandwidth limit that would justify changing the
shading path.

## Same-frame conservative Hi-Z

The opt-in path records these render-graph dependencies in the same frame:

1. Run the current GPU frustum pass and compact candidates into the existing
   instance buffer.
2. Render depth for those candidates into an R32 depth-color target, with D32
   depth testing and depth writes enabled. The indirect draw avoids rasterizing
   instances already outside the camera frustum.
3. Build a 15-entry depth pyramid. Each compute pass reduces a 2x2 region with
   `max`, including odd-sized image boundaries and out-of-range guards.
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

Runs used the Windows D3D12 backend at 1280x720. Each path warmed up for 16
frames, then measured 80 frames. The diagnostic benchmark waits for GPU
completion after each frame, so it compares completed frame cost consistently
but serializes CPU/GPU work; it is not a throughput benchmark for a fully
pipelined production loop.

### Current 1,681-instance grid

| Path | Frustum-visible | Hi-Z rejected | Final visible | p50 completed frame | p95 completed frame | Image checksum |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Frustum only | 582 | 0 | 582 | 7.02 ms | 7.95 ms | `0ab48b0be99ac217` |
| Same-frame Hi-Z | 582 | 0 | 582 | 18.92 ms | 25.22 ms | `0ab48b0be99ac217` |

### Synthetic occlusion probe

The comparison script adds eight same-mesh instances behind a central opaque
instance on the camera's view axis. Both paths render the identical probe scene.

| Path | Frustum-visible | Hi-Z rejected | Final visible | p50 completed frame | p95 completed frame | Image checksum |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Frustum only | 590 | 0 | 590 | 6.93 ms | 8.26 ms | `5f2c2790964e06d4` |
| Same-frame Hi-Z | 590 | 6 | 584 | 20.82 ms | 25.60 ms | `5f2c2790964e06d4` |

The exact checksum match proves that the six rejected probes did not alter the
captured output in this test. The regular grid contributes no Hi-Z rejects;
the synthetic probe confirms the occlusion path does reject fully hidden
instances. For the tested workload, the extra depth prepass and pyramid cost
about 3x the frustum-only p50. Keep Hi-Z opt-in until it is measured against a
dense voxel scene with large opaque occluders and through camera motion.

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
