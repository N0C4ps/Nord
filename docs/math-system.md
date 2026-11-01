# Nord Math

## Ownership and boundaries

`nord_math` owns game-independent numeric, geometric, and spatial primitives.
Engine crates that need the primitives may depend on it; the root game package
uses the stable `nord_app` facade (`ApplicationVec3`, `ApplicationMat4`,
`ApplicationAabb3`, and related names). The game does not import implementation
crate names.

The crate is divided by responsibility:

- `scalar`: interpolation, angle wrapping, and tolerance-aware comparisons.
- `vector`: Nord-owned scalar and integer vectors with explicit operators.
- `matrix`: Nord-owned matrices, quaternion, and affine-transform primitives.
- `geometry`: validated AABB, sphere, plane, ray, frustum, and intersections.
- `spatial`: bounded octree broad-phase indexing with AABB, sphere, and ray
  queries.

## Conventions

- Floating-point vectors use `f32` for ordinary simulation/render work; `f64`
  variants are available for high-range calculations and tooling.
- Signed and unsigned integer vectors include 32-bit and 64-bit forms; use
  64-bit integer coordinates for large world/chunk indices where exact cells
  matter more than fractional precision.
- Integer vectors provide checked arithmetic for coordinates and counters where
  overflow must be reported; plain operators follow Rust's configured overflow
  behavior.
- Angles are radians.
- Matrices use column vectors, column-major storage, and `M * v` transforms.
- Coordinate handedness is not imposed globally. A camera/projection caller
  must select a matching handedness and graphics API depth convention.
- Geometry constructors validate finite coordinates, non-negative extents, and
  ordered bounds. AABB faces are inclusive.
- `Ray3` normalizes its direction at construction, so returned hit parameters
  are world-space distances.
- Approximate equality is explicit and accepts absolute and relative tolerances;
  exact float equality is not presented as a general geometry test.

Vectors, matrices, quaternions, and transforms are implemented inside
`nord_math`; there is no external math-library dependency. The current
arithmetic is scalar and safe Rust. Keep public conventions stable while
adding and benchmarking Nord-owned optimized implementations. Any future SIMD
path must preserve scalar behavior and public layout/API contracts, and should
be selected internally by target capability rather than by game code.

## Spatial index

`Octree<T>` is a fixed-root, single-writer broad-phase structure. It stores
item bounds, retains items spanning octant boundaries at their common parent,
and returns complete AABB/sphere/ray candidates. Ray results are sorted nearest
to farthest. IDs remain unique across removals and `clear`. The depth cap bounds
recursive stack use; items outside the root are rejected rather than silently
clipped.

An octree is useful for sparse arbitrary 3D objects and spatial queries. Dense
voxel storage should normally remain chunked and grid-indexed: an octree is not
automatically the right representation for every voxel or chunk. Its exact role
in world streaming and collision broad-phase should be chosen from profiling.

## Deliberate future extensions

The current foundation does not yet include SIMD batch kernels, integer world
coordinate/chunk conversion, robust predicates for computational geometry,
frustum extraction from a renderer-specific projection matrix, OBBs, capsules,
convex hulls, or a dynamic loose octree. Add each behind this crate's API when
an engine subsystem has a concrete need, and benchmark before replacing the
scalar or broad-phase behavior.

## References

- [Rust `std::ops`](https://doc.rust-lang.org/stable/std/ops/) — operator-trait
  contracts used for vector and matrix arithmetic.
- [Rust floating-point documentation](https://doc.rust-lang.org/stable/std/primitive.f32.html) —
  floating-point operations and edge-case behavior.
- [`oktree` documentation](https://docs.rs/oktree/latest/oktree/) — reference
  octree behavior and flat/pool-oriented implementation tradeoffs.
