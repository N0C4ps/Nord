# Dependency injection

## Goal

`nord_dependency` is a small, runtime typed registry for composing Nord Tech services and services supplied by external crates. It has no third-party dependencies and does not need to know which engine or vendor crate a registered service came from.

The crate deliberately provides a composition mechanism, not automatic construction/reflection. The application chooses concrete implementations and registers them near startup, then resolves shared instances and passes them explicitly to the modules that need them.

## Lifecycle

1. In the composition root, create a `DependencyBuilder`.
2. Register concrete service instances with `register` or pre-shared `Arc<T>` values with `register_shared`.
3. Call `build` to create an immutable `DependencyContainer`.
4. Resolve dependencies at the composition boundary and pass `Arc<T>` (or narrower references/handles) into constructors.

```rust
use nord_dependency::DependencyBuilder;
use std::sync::Arc;

struct AssetCache;
struct AudioBackend;

let mut builder = DependencyBuilder::new();
builder.register(AssetCache)?;
builder.register_shared(Arc::new(AudioBackend))?;

let services = builder.build();
let asset_cache = services.get::<AssetCache>()?;
let audio_backend = services.get::<AudioBackend>()?;
```

This example shows the container API only. In the game, keep resolution in `src/app/` and pass the resulting handles to game modules and constructors.

## Design rules

- **Explicit composition:** register and resolve services at startup or another clear composition boundary. Do not use a process-global container.
- **Constructor injection:** systems should receive the specific services they require. Avoid passing the whole container into every system, which hides dependencies and turns it into a service locator.
- **Concrete type keys:** registration and resolution are keyed by Rust `TypeId`. A concrete type can be registered only once; duplicate registration returns `DependencyError` instead of silently replacing a service.
- **Shared lifetime:** registered services are shared through `Arc`; the container can be read from multiple threads. Services themselves must be `Send + Sync + 'static`.
- **External crates:** third-party service types can be registered directly. For trait-based APIs or two instances of the same external type, define a small local newtype for each registration role, containing the desired concrete value or `Arc<dyn Trait>`.
- **No hidden construction:** this version does not run arbitrary factories, infer a dependency graph, or create transient instances. Construct services explicitly so failures and ownership are easy to follow.

The container is intentionally modest. If a concrete system later needs scoped lifetimes, async construction, fallible factories, or compile-time graph checking, add that capability based on an actual use case instead of paying for it in every build now.

## Research basis

Rust traits provide explicit interfaces, while trait objects offer runtime polymorphism with an indirect dispatch cost. This registry uses Rust's `Any` type erasure and `TypeId` for concrete service lookup; application-facing abstractions should still use narrow traits and constructor parameters where that gives better decoupling. The design keeps the dynamic lookup at composition time rather than making every gameplay/engine operation perform registry lookups.

- [Rust Reference: traits](https://doc.rust-lang.org/reference/items/traits.html)
- [Rust Reference: trait objects](https://doc.rust-lang.org/reference/types/trait-object.html)
- [Rust Book: traits and generics](https://doc.rust-lang.org/book/ch10-02-traits.html)
