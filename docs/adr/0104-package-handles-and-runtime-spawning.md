# 0104: Packages as shared handles, islands spawned and despawned at runtime, GPU instances keyed by island

Status: accepted
Date: 2026-11-27

## Context

#134, design doc §5 ("Streaming / IO": an async pool decompresses and transcodes) and §21 (`core`: asset handles).
After #133 a runtime shows one avatar: the viewer loads one package on the main thread before the window opens,
every island shares one `Model`, and the render thread's `Gpu` holds one instance per island by index, which only
holds while nothing despawns. `Scene` bump-allocates from capacities given up front (ADR 0023: "freeing comes with
avatar streaming"). The doc does not say what a handle is, how islands keep their order when the ECS table moves
them, how the render thread learns of new packages or how its buffers are sized when packages arrive while running.

## Decision

1. **A package handle is an `Arc<Package>`** (`civbb_client::package`): the validated `Document`, its compiled
   `Model` (behind its own `Arc`, as islands hold it) and its decoded textures, plus a process-unique id. Loading
   (read, verify, decode, validate, texture decode or transcode, compile) is one task on bevy_tasks' `IoTaskPool`;
   the caller polls the task and spawns islands from the frame it is done (results enter the world between frames,
   ADR 0103 point 4). It lives in `client`, not `core`, because `Model` is a client type and an `Arc` needs no
   wrapper; refcounting is the `Arc`'s.
2. **Islands keep their spawn order.** Each island entity gets a `Spawned(u64)` sequence number and a `Shows`
   component holding its package handle. Despawning reorders the ECS table, so the extract step sorts by `Spawned`
   (snapshot and draw order stay stable). The contact pass does not sort: a receiver's value is the largest over its
   senders and its touching set is only tested for membership, so island order does not change results.
   `Runtime::spawn_package` and `Runtime::despawn` are the API; `Runtime::islands` stays in spawn order over the
   live islands.
3. **The snapshot names its islands.** Each `IslandSnapshot` carries a key (the entity's bits) and the package
   handle. The render thread keeps one upload per package (meshes, textures, shared materials, culling bounds) and
   one instance per key (skinned meshes, the island's own material records and their last packed values); an
   unknown key takes a freed instance of its package or a new one, and a key missing from a snapshot returns its
   instance to its package's free list. Freed instances need no GPU wait: they are only written by later recorded
   commands.
4. **Capacity grows by rebuilding.** The render thread sizes the scene for what a snapshot needs (`Capacity::
   for_meshes` per package and instance, materials, draws). When a snapshot needs more, it waits for the device,
   destroys the scene, every upload and instance and the forward pass, and recreates them with twice the old
   capacity or the need, whichever is larger; free lists and uploads of packages no island shows are dropped then.
   Packages keep their decoded textures in RAM for this. The first snapshot sizes the scene exactly, so parity
   allocates as before.

## Consequences

- The viewer takes several packages, loads them on the I/O pool while frames run, and spawns each package's copies
  when its load finishes; a package's arrival may stall one frame for a rebuild.
- Despawned islands' GPU memory is reused only by the same package until the next rebuild; a range allocator, a
  VRAM budget and texture eviction (design doc §6 "Textures and memory") replace rebuilding and the RAM copy of
  textures when avatar streaming needs them (#138).
- I/O threads are not yet kept to efficiency cores (ADR 0103 point 3); `affinity` has no such set.
