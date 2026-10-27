# CIVBB — Design Doc

Oct 27, 2026 · @Keynoh

## 1. Summary and goals

CIVBB (codename for "Chatting in VR but Better") is an open-source, heavily AI-assisted hobby project to build a social VR platform on a custom Rust + Vulkan engine, where every system VRChat runs on one main thread (avatar animation, dynamics, scripts, draw submission) runs as parallel per-avatar jobs. Creators bring their existing VRChat SDK projects through a one-click exporter, and a server-side content compiler turns them into an optimised native format. Compatibility comes from translation at upload time, not from emulating Unity at runtime. That is what lets old content get the new performance.

**Goals**

- **Scale with cores, not avatar count.** CPU frame time should drop roughly linearly as cores are added, and a crowded instance should degrade gracefully instead of collapsing.
- **Migrate most existing content automatically.** Avatars convert with visual parity and no creator work in the large majority of cases. Worlds convert with a clearly reported compatibility score.
- **No user content ever runs native code.** Shaders, scripts and assets are compiled and validated server-side; clients only run verified output.
- **Modding is a feature, not a ban reason.** A sandboxed, capability-based plugin API replaces client mods and kernel anti-cheat.
- **One content build, three platforms.** PC VR, flat desktop and standalone Quest from the same upload, with per-platform variants generated automatically.

**Non-goals**

- Pulling content from VRChat's servers or CDN. Migration only runs on source projects the creator owns (section 2).
- Bit-exact compatibility with arbitrary custom shaders. The target is visual parity within a measured tolerance, with fallbacks.
- A web or iOS client in v1, and blockchain or NFT ownership. Community nodes and peer-to-peer hosting are in (D10, section 12b), but come after the official node works.

**Success metrics (targets for public beta)**

| Metric | Target |
| --- | --- |
| Frame rate, 40 converted full-quality avatars, 12600K-class CPU + RTX 4070/5070-class GPU | 90 Hz stable, CPU main-thread time under 4 ms |
| Frame rate, same scene on Quest 3 standalone | 72 Hz with automatic LOD and impostors |
| Avatars from the top 500 bases that convert with visual parity (FLIP score under threshold) | 90 % or more |
| Worlds that convert and run their scripts without creator changes | 70 % or more, every failure itemised in a report |
| Instance join time on a warm cache | under 10 s to first frame with full avatars streaming in |
| Crashes caused by user content | zero known classes left unmitigated |

## 2. Principles and hard constraints

Everything below follows from eight rules. When a later section seems to conflict with one, the rule wins.

1. **Compile, don't emulate.** Legacy content is translated once, server-side, into the native format. The runtime never carries a Unity emulation layer, so it never inherits Unity's threading limits.
2. **Data-oriented by default.** Every per-frame system is a job over contiguous component data. Anything that needs the main thread must justify it in review.
3. **Untrusted content is data, never code.** Users upload sources; the platform compiles them. Clients only execute platform-compiled, signed artifacts: SPIR-V from our shader compiler, Wasm from our script compiler, meshes from our mesh pipeline.
4. **Budgets are enforced, not advertised.** Every avatar and world carries a measured cost. The runtime holds each one to its budget and degrades the farthest and most expensive content first.
5. **The server is the authority for anything that affects other people.** Ownership, permissions, moderation and instance membership live on the server, which makes a modified client mostly harmless.
6. **Memory-safe parsing at every trust boundary.** Untrusted files are parsed in Rust, in sandboxed workers, never in C++ parsers inside the client.
7. **Open where it costs nothing.** Open formats (glTF, KTX2, OSC, OpenXR), a documented protocol and a public plugin API. Lock-in comes from the community, not from opacity.
8. **Free by default.** Visiting worlds, opening instances, uploading and wearing avatars cost nothing. Money flows mainly where creators make things (bases, outfits, tools), not where people hang out, and nothing about discovery or joining can be bought.

### How legacy content may be migrated

This is a hard constraint, not a preference. Avatars and worlds on VRChat belong to their creators, and creators usually hold licences for the bases and assets inside them. So migration only works from material the creator controls:

- **Allowed:** the creator's own Unity project (VRChat Creator Companion project, `.unitypackage`, or a folder of FBX, textures and prefabs), exported with our Unity editor plugin or converted headless by our importer.
- **Not accepted:** built AssetBundles (`.vrca`, `.vrcw`) or anything pulled from VRChat's API or CDN. Ownership of a built bundle cannot be verified, and accepting them would turn the platform into a laundering service for ripped avatars. The upload pipeline fingerprints known ripper output and rejects it (section 15).
- **Creator attestation:** every upload asks the creator to confirm they hold the rights, and the base-model licences of popular Booth avatars are tracked so incompatible licences can be flagged (section 20).

"Takes any asset that was ever uploaded" therefore becomes "takes any project a creator still has". Most active creators keep their projects, because they need them to update avatars.

## 3. Architecture overview

The platform has three runtime paths and one build path. Clients hold a live QUIC connection to an instance server for everything shared in a room, call the backend for accounts, social features and join tokens, and download content only from the CDN. Creator uploads never reach a client directly: they pass through the content pipeline, which compiles, measures, scans and signs them first.

&#91;embedded content: system architecture · 7 components\]

The client is the only component that renders, and it only runs what the pipeline produced. Sections 5 to 11 cover the client, 12 and 13 the instance servers, 14 the backend, and 9, 10 and 15 the pipeline.

## 4. Engine and technology stack

Recommendation: a custom engine in Rust on Vulkan and OpenXR, assembled from proven libraries rather than written from scratch. A general-purpose engine would hand back the exact constraints this project exists to remove. Decided (D1): Rust. It is the most expensive choice, which is why Phase 0 has to prove the core idea before the engine grows.

### Engine options

| Option | Strengths | Deal-breakers for this project | Verdict |
| --- | --- | --- | --- |
| Unity 6 (URP + Entities + GPU Resident Drawer) | Fastest to ship; closest to legacy content; huge talent pool | Main-thread GameObject model still underneath; closed source; licensing risk; would rebuild VRChat's own ceiling | Fallback if team or budget is small |
| Unreal Engine 5 | Best renderer; mature tooling | Runtime loading of arbitrary user content is hard outside Epic's own UEFN; heavy on Quest; 5 % royalty | No |
| Godot 4 | Open source; light; C# and GDExtension | Renderer and multithreading less mature at this scale; we would fork it heavily anyway | Viable for a prototype only |
| Custom C++ (Vulkan, own ECS) | Total control; most engine programmers know C++ | Memory-unsafe parsing of untrusted content; slower iteration | No |
| **Custom Rust (Vulkan via `ash`, `bevy_ecs`, OpenXR)** | Memory safety at every trust boundary; fearless parallelism; strong library ecosystem | Smaller hiring pool; editor tooling must be built | **Recommended** |

### Stack by layer

| Layer | Choice | Why |
| --- | --- | --- |
| Language | Rust (engine, servers, pipeline); TypeScript (web, API gateway, creator dashboard) | Safety plus performance where it matters; TS where iteration speed matters |
| ECS and scheduling | `bevy_ecs` used standalone, custom work-stealing scheduler with fibers for long jobs | Archetype storage, system-level parallelism, no engine lock-in |
| Graphics API | Vulkan 1.3 on desktop, Vulkan 1.1 + extensions on Quest; raw bindings via `ash` | One API across Windows, Linux and Android; full access to multiview, descriptor indexing, VRS and fragment density maps |
| XR | OpenXR via the `openxr` crate | Every current headset runtime; trackers, face, eye and body tracking through extensions |
| Physics (world) | Jolt Physics through C bindings | Multithreaded, deterministic, battle-tested in shipped AAA games |
| Avatar dynamics | Custom XPBD bone-chain solver in SIMD jobs | Matches PhysBones semantics; tiny and parallel per avatar |
| Animation | Custom Mecanim-compatible runtime; ACL for clip compression | Legacy animator semantics are the compatibility core |
| Scripting | WebAssembly on `wasmtime`; Cranelift JIT, Pulley interpreter where JIT is unavailable | Sandboxing, fuel and epoch limits, memory caps, any source language |
| Audio | Steam Audio (open source, Apache-2.0) + Opus | HRTF spatialisation, occlusion and reverb; Opus for voice |
| Networking | QUIC via `quinn`: datagrams for state, streams for reliable data | Encryption, congestion control and connection migration built in |
| UI | Retained-mode UI on our renderer; `taffy` for layout; world-space canvases | Same pipeline for menus and in-world UI; legacy uGUI canvases map onto it |
| Video | FFmpeg via `ffmpeg-next`, hardware decode | Covers world video players and streams |
| Content formats | glTF 2.0 + custom extensions, KTX2 (Basis Universal), meshopt compression | Open, GPU-friendly, streamable |
| Backend | Rust (`axum`, `tokio`) for hot services; TypeScript for the API gateway and dashboards | See section 14 |
| Infrastructure | Kubernetes + Agones, PostgreSQL, Valkey, NATS, S3-compatible storage (Cloudflare R2), Cloudflare CDN | See sections 12, 14 and 15 |

## 5. Client runtime: ECS, jobs and the frame pipeline

The main thread only orchestrates; all real work is jobs over ECS data, and simulation of frame N overlaps rendering of frame N-1. Each avatar is an independent island for the most expensive part of the frame (animation through dynamics), which is what makes the work parallel.

### Threads

| Thread | Count | Job |
| --- | --- | --- |
| Sim orchestrator | 1 | Runs the frame schedule, owns no heavy work; target under 1 ms |
| Workers | cores minus 2 | Work-stealing job pool; all simulation, culling and command-buffer recording |
| Render submit | 1 | Submits recorded command buffers, presents through OpenXR |
| Audio | 1, real-time priority | Mixing, Steam Audio spatialisation, voice decode |
| Network | 1 + async tasks | QUIC I/O, packet decode into lock-free queues |
| Streaming / IO | async pool | Downloads, cache, decompression, texture transcode |

On hybrid CPUs such as Alder Lake (the 12600K), latency-critical threads are pinned to performance cores and streaming work prefers efficiency cores, using CPU sets on Windows and affinity hints on Linux.

### Frame pipeline (one simulation frame)

1. **Poses in.** Sample OpenXR poses for the predicted display time; read trackers, face and eye data.
2. **Network apply.** Move remote avatar snapshots from the network queue into interpolation buffers.
3. **Scripts.** Every world and avatar script actor runs in parallel against a read-only snapshot of last frame's state and writes commands to its own buffer (section 11).
4. **Command apply.** Commands merge in a deterministic order (actor id, then sequence), so results never depend on thread timing.
5. **Avatar island jobs**, one job per avatar, all in parallel: animator state machines and blend trees, then constraints, then full-body IK, then bone dynamics. Each step needs the previous one, but avatars do not wait for each other.
6. **Cross-avatar pass.** Contact shapes (touch interactions) from all avatars go through one spatial broadphase, split by grid cell across workers; results feed next frame's animator parameters, exactly as legacy contacts behave.
7. **World physics.** Jolt steps on the same worker pool.
8. **Visibility and LOD.** Frustum culling against the combined stereo frustum, LOD and impostor selection, and the performance governor's decisions (section 8).
9. **Extract.** Render-relevant data is copied into a separate render world; simulation can start the next frame immediately.
10. **Record and submit.** Workers record secondary command buffers per pass and batch in parallel; the submit thread re-reads head poses at the last moment (late latching) to cut motion-to-photon latency.

### Scheduling rules

- Systems declare their read and write sets; the scheduler builds a dependency graph and runs anything that doesn't conflict in parallel.
- Per-frame scratch memory comes from per-thread arena allocators that reset every frame, so there are no allocations in hot loops.
- Long jobs (a large asset decode, a script that hits its fuel limit) yield on fibers instead of blocking a worker.
- Every system reports its time to the profiler, with Tracy instrumentation in development builds and lightweight counters in release.

## 6. Rendering

On desktop the GPU decides what to draw: compute shaders cull, and indirect multi-draws submit everything that shares a pipeline in one call, so CPU cost no longer grows with material count. A 40-material avatar costs about the same CPU time as a 3-material one. Quest gets a simpler path tuned for tile-based mobile GPUs from the same content.

### Core architecture

- **Clustered forward+ shading.** It suits VR: MSAA works, transparency is natural, and toon shading (most avatar content) needs forward shading anyway. Lights are binned into a 3D cluster grid each frame by a compute pass.
- **Single-pass stereo.** Both eyes render in one pass through Vulkan multiview, so geometry is processed once.
- **Render graph.** Passes declare their resources; the graph handles barriers, aliasing of transient targets and pass culling. Mirrors and cameras are extra views in the same graph.

### GPU-driven path (desktop)

1. All meshes live in large shared vertex and index buffers; all material parameters live in one storage buffer, indexed per draw.
2. Textures are bindless through descriptor indexing, so draws never rebind textures.
3. A compute pass culls instances against the stereo frustum and a hierarchical-Z buffer from the previous frame, then writes indirect draw commands grouped by pipeline.
4. One indirect multi-draw per pipeline per pass. The number of draws scales with the number of distinct shader variants, not objects.

### Mobile path (Quest)

- Keep work in tile memory: 4x MSAA resolved on-chip, no depth prepass, no full-screen post effects by default.
- Fixed foveated rendering via fragment density maps; aggressive LOD and impostors (section 8).
- Static batching of world geometry at compile time instead of GPU culling where the GPU-driven path costs more than it saves.

### Skinning and blendshapes

Skinning runs in compute once per visible avatar per frame and writes into a buffer both eyes read. Blendshapes are stored as sparse deltas, and only shapes with non-zero weight this frame are applied. That matters, because legacy avatars often carry hundreds of face-tracking and toggle shapes while only a handful are active at once.

### Materials and shaders

| Material model | Covers | Path |
| --- | --- | --- |
| PBR | glTF metallic-roughness, Unity Standard | Native uber-shader |
| Toon | Common feature set of lilToon, Poiyomi, UTS2, Sunao, MToon | Native uber-shader with feature flags |
| Unlit / Mobile | VRChat Mobile shaders, simple unlit | Native, cheapest |
| Translated | Custom HLSL that maps to none of the above | Compiled from source by the content compiler (section 10), budgeted and sandboxed |

All pipelines a piece of content can need are known at compile time, so they are pre-built when content loads. Vulkan pipeline caches and graphics pipeline libraries keep shader-compile hitches out of the frame.

### World lighting and effects

- Baked lightmaps, light probes and reflection probes from legacy worlds are reused directly; baked lighting is expensive to recreate and converts well.
- Real-time lights go through the cluster grid; shadows use cascaded shadow maps for the main directional light and a capped number of local shadow casters.
- Post-processing covers bloom, tonemapping, colour grading LUTs and ambient occlusion, mapped from Unity's post-processing stack, and every user can switch it off.
- **Mirrors** are a separate view at reduced resolution with a LOD bias and expensive effects off. A mirror's cost counts toward the world's budget, so mirrors cost less than VRChat's full second render of the scene.

### Textures and memory

Textures ship as KTX2 with Basis Universal supercompression and are transcoded on load to BC7 (desktop) or ASTC (Quest). Mip levels stream in by screen size. A VRAM budget manager evicts the least recently visible textures first, which ends the "one avatar uses 2 GB of VRAM" problem.

## 7. Avatar runtime systems

Each avatar system reproduces the legacy behaviour creators rely on, but runs from compiled data inside the avatar's own job. The work happens at compile time: string paths, property bindings and evaluation order are resolved once on the server instead of on every client at load.

### Animation

- **A Mecanim-compatible state machine runtime:** layers (override and additive), avatar masks, 1D, 2D and direct blend trees, transitions with exit time, conditions, interruption sources and triggers.
- **Write Defaults reproduced exactly.** WD on and off behave differently in subtle ways, and mixing them is the most common source of broken legacy avatars. A reference test suite compares our evaluation against Unity frame by frame.
- **Built-in parameters keep their legacy names and meanings** (`GestureLeft`, `Viseme`, `VelocityX`, `Upright`, `IsLocal`, `TrackingType` and the rest), so existing animators work without edits.
- **State behaviours are mapped:** parameter drivers, tracking control, layer control, locomotion control and audio playback.
- **Compile-time optimisation:** toggle layers collapse into a single direct blend tree, unused layers and curves are stripped, and clips are compressed with ACL. Avatar optimiser tools already prove this cuts animator cost a lot without visible change.

### Full-body IK

- One solver handles 3-point (head and hands), 6-point and 10+ tracker setups: analytic two-bone IK for limbs, an iterative solver for spine and hips, and calibration that stores per-user offsets.
- **IK targets are networked, not bones.** Remote clients receive head, hands and tracker poses and solve locally, which saves bandwidth and matches how VRChat syncs avatars.
- Finger poses come from controller curl data (Index, Quest Touch) or OpenXR hand tracking.

### Bone dynamics (PhysBones-compatible)

An XPBD chain solver reproduces the documented PhysBone parameters: pull, spring or momentum, stiffness, gravity, immobile, angle, hinge and polar limits, sphere, capsule and plane colliders, grab and pose, and stretch and squish. Chains are solved in SIMD batches inside the avatar job. Legacy Dynamic Bones convert to the same solver, the way VRChat already auto-converts them. Simulation rate drops with distance (section 8).

### Contacts and constraints

- Contact senders and receivers keep their tags, shapes and receiver types (constant, on-enter, proximity) and feed animator parameters. A per-user setting decides whose contacts may affect your avatar: everyone, friends or nobody.
- Position, rotation, scale, parent, aim and look-at constraints are topologically sorted at compile time, so evaluation is one linear pass with no iteration.

### Face, eyes and voice

- **Lip sync:** a small on-device model turns voice audio into the 15 standard visemes. It runs on decoded audio for remote users too, so visemes never need to be sent. It replaces the Oculus Lipsync library, a third-party dependency whose licence terms would need legal review.
- **Face and eye tracking** through OpenXR extensions, normalised to the Unified Expressions parameter set the community already uses, so face-tracking avatars work unchanged. OSC input from VRCFaceTracking is also accepted.
- Fallback automatic blinking and look-at for users without eye tracking.

### Expressions and interaction

- A radial expression menu with toggles, two- and four-axis puppets and submenus, imported from legacy menus.
- Synced expression parameters: legacy avatars fit in 256 bits; native avatars get 1,024 bits by default, held in check by the network governor (section 12).
- Stations (seats), avatar pedestals, avatar scaling and eye-height calibration.

## 8. Performance governance

A runtime governor holds the frame to its time budget by degrading avatars step by step, farthest and most expensive first, instead of letting one heavy avatar drag down everyone. Performance ratings still exist, but as information for creators; the governor is what actually protects the frame rate.

### Cost model

Every avatar and world carries a cost profile produced by the content compiler (section 9). It is measured by running the content on reference hardware, not just counted:

| Dimension | Inputs |
| --- | --- |
| CPU | Animator layers and states, bones, dynamic chains, contacts, constraints, script fuel per frame |
| GPU | Skinned vertices, active blendshapes, triangles per LOD, estimated pixel-shader cost, pipelines used |
| Memory | VRAM and RAM at each LOD |
| Network | Synced parameter bits, script message rate |

Field telemetry (opt-in) feeds back to creators as "your avatar costs 0.4 ms of CPU on median hardware", which motivates optimisation far better than a letter grade.

### Degradation ladder

Each frame the governor compares measured frame time against the target (11.1 ms at 90 Hz, minus headroom). When over budget, it walks avatars in order of distance times cost, divided by a priority boost for friends and people you are talking to, and moves the worst one down one step:

1. **Update-rate decimation:** animation and dynamics run every 2nd, then every 4th frame, with interpolated poses in between. This is the single biggest CPU saving for avatars more than a few metres away.
2. **Dynamics off** beyond a distance threshold; chains settle into their rest pose.
3. **Lower mesh LOD** (generated at compile time, section 9).
4. **Impostor:** an octahedral impostor baked at compile time from the default outfit, shown at long range.
5. **Hidden:** nameplate only.

When there is headroom again, avatars climb back up one step at a time. Hysteresis keeps them from flickering between levels.

### User controls

- Caps for full-quality avatars and full-quality distance; a toggle to exempt friends; per-user "always show" and "always hide".
- Content blocks by cost class ("hide anything over 2 ms GPU"), replacing VRChat's rank-based blocking with something that tracks real cost.
- Quest ships with stricter defaults.

### Worlds

Worlds get a budget envelope for static geometry, lights, mirrors, video players and scripts. A world that goes over it lowers its own quality settings first (mirror resolution, shadow distance) before the avatar ladder kicks in, so the world, not the visitors, pays for an expensive world.

## 9. Native content format and content compiler

Creators upload an editable **source package**; the platform compiles it into a **runtime package** per platform and keeps the source. Because sources are kept, all existing content gets faster whenever the compiler improves, just by recompiling it.

### Source format

glTF 2.0 plus extensions, in the spirit of VRM:

| Extension | Holds |
| --- | --- |
| `X_avatar` | Humanoid bone mapping, viewpoint, viseme mapping, eye-look setup |
| `X_animator` | Layers, state machines, blend trees, parameters, state behaviours |
| `X_dynamics` | Bone chains, colliders, grab and pose settings |
| `X_contacts`, `X_constraints` | Senders, receivers, constraint graph |
| `X_expressions` | Menu tree and synced parameter declarations |
| `X_toon_material` | Toon material model parameters; PBR uses standard `KHR_materials_*` |
| `X_shader_source` | HLSL source for translated custom shaders (section 10) |
| `X_scripts` | Script sources or Wasm modules and their interface version |

Being glTF means creators can also author directly in Blender with our add-on; Unity becomes optional.

### Runtime format

- A signed manifest (CBOR, Ed25519 signature) listing content-addressed blobs (BLAKE3 hashes).
- GPU-ready meshes: quantised attributes, meshlets on desktop, `EXT_meshopt_compression` on the wire.
- KTX2 textures: UASTC for normal maps and high-quality slots, ETC1S for the rest; size caps per platform.
- SPIR-V for every pipeline the content needs, per platform.
- Compiled animator tables, ACL-compressed clips, sorted constraint lists, dynamics chains.
- Wasm modules; clients compile them with Cranelift and cache the result by hash.
- LOD chain, impostor atlas, thumbnails, and the measured cost profile.

### Compiler stages

1. **Ingest and validate:** schema, size limits, file-type sniffing, malware scan.
2. **Normalise:** units, axes, humanoid mapping, scale.
3. **Mesh:** weld and clean, prune bones and blendshapes nothing references (animations, menus and face tracking are checked first), merge meshes that share a material and toggle together, quantise, build meshlets, generate a skin-aware LOD chain with meshoptimizer.
4. **Materials:** identify each shader and map it to a native model, or send it to translation (section 10); atlas textures within one material model; compress to KTX2.
5. **Animation:** compile state machines, bind curves to component indices, collapse toggle layers, strip unused clips, compress with ACL.
6. **Dynamics, contacts and constraints:** convert and sort.
7. **Scripts:** compile to Wasm, validate, check imports against the API allowlist (section 11).
8. **Bake:** impostors, thumbnails, a preview turntable.
9. **Measure:** run the content headless on reference GPU nodes and record its cost profile.
10. **Parity check** (migrated content only): render the same poses in Unity and in our engine and compare with NVIDIA FLIP (section 10).
11. **Package and sign:** write blobs to storage, sign the manifest, publish.

The compiler is versioned. A manifest records which compiler version built it, and a background job recompiles older content when a new version brings a measurable gain, with the creator able to opt out.

### Creator tooling

- **Unity editor plugin:** exports legacy projects, previews conversion results and the compatibility report, uploads.
- **Blender add-on:** native authoring of avatars and simple worlds.
- **CLI:** `build`, `validate`, `upload`, for CI and power users.
- **Local test client** with hot reload, so creators iterate without uploading.

## 10. Legacy content migration

Avatars can migrate almost completely, because they are mostly standard data: meshes, bones, animators and documented components. Worlds migrate partially, because they contain arbitrary Udon logic and Unity features. Custom shaders are the hardest single problem for both. Every migrated item gets a measured compatibility report before it goes public, so nothing breaks silently.

### Export path

1. The creator installs our Unity editor plugin into their existing Creator Companion project.
2. The plugin reads the scene through Unity's own serialisation API, so it sees exactly what Unity sees: prefabs, overrides, materials, animator controllers and VRChat SDK components. It reads those components' serialised fields and does not bundle or link VRChat SDK code (section 20).
3. It writes the source package (section 9) and also **renders reference images inside Unity on the creator's machine**: a fixed camera rig, a set of poses, every toggle and expression state. This gives the parity check its ground truth without running Unity on our servers.
4. "Migrate my library" runs the same steps in batch mode over every project the Creator Companion knows about.

### Coverage matrix

| Content | Strategy | Expected coverage |
| --- | --- | --- |
| Meshes, skinning, blendshapes | Direct conversion | Full |
| Humanoid rig, viewpoint, visemes | Direct conversion | Full |
| Animator controllers, expression menus and parameters | Compiled to our Mecanim-compatible runtime (section 7) | High; Write Defaults edge cases are tested explicitly |
| PhysBones, contacts, VRChat constraints | Documented components, converted one to one | Full |
| Dynamic Bones, Unity constraints | Converted to the same solvers | High |
| Materials on known shaders | Mapped to native PBR and Toon models | High; visual drift is measured |
| Custom shaders | Translated from HLSL (below) | Partial; fallback bake when translation fails |
| AudioLink effects | Native AudioLink-compatible data texture (the protocol is MIT-licensed) | High |
| Baked lighting, probes | Reused as-is | Full |
| Particle systems (Shuriken) | Native GPU particles covering the common modules | Partial |
| Udon and UdonSharp scripts | Translated to Wasm (below) | Partial; measured per world |
| uGUI and TextMeshPro | Mapped to our UI runtime; SDF font atlases converted | High for common widgets |
| Unity Cloth, Timeline, Terrain | Cloth to XPBD, Terrain to mesh, Timeline partial | Partial |
| Occlusion data, NavMesh | Occlusion dropped (Hi-Z at runtime); NavMesh rebuilt with Recast | Full in effect |

### Shaders

1. **Fingerprint.** Shader name, GUID and source markers identify known families and their versions: Poiyomi (including locked or optimised copies), lilToon, UTS2, Sunao, Xiexe, Silent's shaders, VRChat Mobile, Unity Standard. Versioned mapping tables translate each family's material properties into native Toon or PBR parameters. The four or five biggest families cover most avatars.
2. **Translate.** Unknown shaders are compiled from source: a compatibility include library provides Unity's built-in variables and macros (`UnityCG.cginc`, light and SH globals, VRChat camera and mirror globals) backed by our lighting data. DXC compiles to SPIR-V. `ForwardBase` maps to the main light plus ambient SH, `ForwardAdd` runs once per significant light up to a cap, and `GrabPass` shares one capture per frame.
3. **Validate.** spirv-val, instruction-count limits, loop-bound instrumentation, no storage writes. Shaders that fail validation are rejected; this is also the main defence against GPU crasher shaders.
4. **Fallback bake.** If translation fails or the result is too expensive, the compiler renders the original appearance (from the reference images) into textures for a native Toon material and marks the item as approximated.

Geometry and tessellation shaders run on desktop only; Quest variants use the fallback.

### Udon worlds

- **Source first.** If the project has UdonSharp C# sources, our Roslyn-based compiler compiles them to Wasm directly, with full type information.
- **Assembly second.** Udon graphs and programs without sources are translated from Udon Assembly. Udon is a simple VM whose calls into Unity go through named "extern" signatures, so each signature maps to a host function in our API shim.
- **Coverage by corpus.** The externs to implement first are ranked by how often they appear in a corpus of public world prefabs and creator-provided worlds; coverage is reported as the share of extern calls a world uses that we implement.
- **Network semantics map one to one:** synced variables, continuous and manual sync, ownership transfer, `RequestSerialization`, custom network events (section 12).
- **Prefab substitution.** Well-known prefabs (video players, mirror toggles, pens, seat systems) are detected and swapped for native equivalents that keep their configuration. This is faster and more reliable than translating them.

### Parity scoring

The server renders the same camera rig, poses and states as the reference images and compares each pair with NVIDIA FLIP. The creator sees side-by-side images and a traffic-light score per state: green publishes automatically, yellow publishes with a note, red needs a fix or the fallback. Aggregated scores show which shader families or features to improve next.

## 11. Scripting and safe modding

All user code, in worlds, on servers and in client plugins, runs as WebAssembly in `wasmtime` with hard limits on CPU, memory and capabilities. That one sandbox replaces both Udon and client mods, and makes kernel-level anti-cheat unnecessary.

### World scripts

- **Host API defined in WIT** (the WebAssembly Component Model's interface language): entities and transforms by handle, events, players, physics queries, audio, UI, networking, persistence. The interface is versioned, and old versions stay supported.
- **Actor model for parallelism.** Each script instance is an actor with its own linear memory. During the script phase (section 5), all actors run in parallel against a read-only snapshot of the world; writes go to a command buffer, and messages to other actors arrive in the next phase. No data races are possible, and results are deterministic.
- **Limits per actor:** fuel per frame (the CPU budget), epoch interruption as a backstop, a memory cap (16 MB default, more for world-level actors), message-rate caps. An actor over budget is paused and reported, never allowed to freeze the frame.
- **No direct I/O.** Remote strings, images and JSON load through a platform proxy with rate limits and a domain allowlist, and users must agree before untrusted URLs load. The proxy also hides users' IP addresses from third-party servers.
- **Persistence:** per-world and per-player key-value storage with quotas, synced through the backend.
- **Server-side scripts:** the same module can run on the instance server for authoritative logic: game rules, scoring, anti-cheat for game worlds, persistence writes. This is a capability VRChat worlds don't have and it changes what can be built.

### Languages

| Language | Path | For |
| --- | --- | --- |
| C# (subset) | Our Roslyn-based compiler to Wasm, like UdonSharp's approach | Unity creators; migrated UdonSharp code |
| Lua (Luau) | The Luau VM compiled to Wasm; scripts run inside it | The easiest start; people coming from FiveM or Roblox |
| TypeScript (strict) | Type-checked with tsc --strict in the pipeline (type errors reject the upload), bundled, then turned into a Wasm component with the Bytecode Alliance's jco tooling (StarlingMonkey JS engine); the API types are generated from the WIT, so they always match the host | Web developers |
| Visual graphs | Node editor compiling to the same IR | Non-programmers; migrated Udon graphs |
| Rust | Official SDK crate | Performance-heavy systems |
| Anything else | Any language that emits Wasm components against the published WIT | Power users |

Decided (D2): several languages on one API, the FiveM model. Interpreted languages ship their runtime as a Wasm module (the Luau VM, the StarlingMonkey JS engine), so Lua and TypeScript scripts get exactly the same sandbox, limits and API as compiled C# or Rust. They run slower per instruction, so heavy systems are better written in C# or Rust, and the fuel budget makes that cost visible.

The local test client attaches a debugger (DWARF debug info in Wasm), supports hot reload and shows fuel use per actor per frame.

### API evolution and breaking changes

Already-published content never breaks: a compiled script is bound to the API version it was built against, and the host keeps serving that version. Breaking changes only affect creators when they rebuild from source, and even then the platform tries to migrate their code automatically.

1. **Versioned interfaces.** Every WIT package carries a semantic version (`civbb:world@1.4.0`). Minor versions only add; anything removed or changed is a new major version.
2. **Old majors live on as adapters.** The host implements an old major version on top of the new one, so a v1 script runs unchanged on a v2 engine. An adapter is dropped only when almost no live content uses it, after a long announced deprecation.
3. **Breaking changes are detected by machines, not reviewers.** CI diffs the WIT packages and the generated TypeScript declarations (with API Extractor), plus `cargo-semver-checks` for public Rust crates. Any pull request that breaks an API is labelled and needs a migration note.
4. **Preview channel.** New APIs ship first as `@civbb/sdk@next` and in an `unstable` interface that only beta nodes enable, so script developers can try them weeks before release.
5. **Ecosystem test run before release.** Because sources are kept (section 9), every public script is recompiled against the new SDK before a release, the way Rust's crater tests the compiler against all published crates. The release notes list exactly which scripts would break.
6. **Automatic migration.** Each breaking release ships codemods (for TypeScript, built on `ts-morph`) behind a `civbb migrate` command. The pipeline can also apply them itself: if the migrated script compiles and passes its own tests, the creator gets a one-click "accept migration"; otherwise they get a report of what to change.

### Client plugins: modding as a feature

Plugins are Wasm modules that run on your client, installed from a reviewed registry or side-loaded in developer mode. Each declares capabilities that the user approves at install:

| Capability | Allows |
| --- | --- |
| `ui.overlay` | Own panels and HUD elements |
| `audio.captions` | Read decoded voice audio for live captions or translation |
| `avatar.params.local` | Read and drive your own avatar's parameters |
| `osc` | Talk to local OSC tools |
| `camera` | Extra camera views and capture |
| `net.http` | Proxied web requests to the domains it lists |
| `input` | Remap and add input bindings |

The hard rule: **a plugin can change what you see, hear and send as yourself, but never gets raw access to other people's content and can't send anything a normal client couldn't.** The server validates every message anyway (section 12), so a hostile plugin gains little. Accessibility mods such as captions, which VRChat's anti-cheat rollout broke, become first-class plugins.

Game worlds that want a level playing field can rely on server-side scripts for authority. A client "unmodified" flag is offered, but on PC it can't be made tamper-proof, so it is only a soft signal.

## 12. Networking and instance servers

Every instance runs on a dedicated Rust server that relays all traffic, holds the authoritative copy of shared state and decides how often each client hears about each other client. On official nodes no traffic goes peer to peer, so IP addresses stay hidden, and server-held state means late joiners always get a correct world. Community nodes and peer-to-peer hosting are covered in 12b.

### Instance servers

- One Rust process (`tokio`) hosts many instances, each an actor pinned to one core's task queue.
- Fleets run on Kubernetes with **Agones** (the open-source game-server orchestrator): allocation, health checks, draining for zero-downtime updates, autoscaling by region.
- Regions at launch: Frankfurt, US East, US West, Tokyo (the Japanese community is large). Base load on bare-metal machines (Hetzner or OVH) for cost; burst capacity on a managed cloud cluster. Agones runs on both.

### Join flow

1. The client asks the backend to join an instance.
2. The backend checks instance type, bans, age gates and capacity, then issues a short-lived signed join token (PASETO).
3. The client connects to the instance server over QUIC with the token.
4. The server sends a join snapshot: players, avatar manifests, all networked object state.
5. The client streams content from the CDN in parallel, shows impostors first and swaps in full avatars as they arrive.

Walking up to a portal prefetches the next world's manifest and impostors.

### Transport

QUIC via `quinn`: unreliable datagrams for poses and continuous state, reliable streams for events, chat and control. QUIC brings encryption, congestion control, fast reconnects and connection migration, which matters when a Quest switches access points.

### State sync

| Kind | Authority | Sync |
| --- | --- | --- |
| Avatar pose | Owning client | IK targets + synced parameters, 30 Hz, quantised, delta-compressed against the last acknowledged state |
| World objects (legacy Udon model) | Current owner, arbitrated by the server | Continuous or manual sync, ownership transfer through the server, exactly as Udon defines them |
| Server-scripted objects | Instance server | Server snapshots; clients interpolate and predict |
| Events | Sender, validated by the server | Reliable and ordered per object; targets: all, owner, server |
| Persistence | Backend, via the server | Written on change, loaded at join |

- **Interest management:** the server sends each receiver updates from each sender at 30, 15 or 5 Hz depending on distance, visibility, friendship and who is speaking. This is the network side of the governor in section 8.
- **Timing:** clients sync to the server clock and play remote state through an adaptive jitter buffer (about 100 ms target).
- **Validation:** every message is checked for permissions, rates and sizes on the server; an invalid one is dropped, never forwarded.

### Bandwidth estimate

Rough numbers to size servers, to be confirmed by load tests: about 100 bytes per avatar update after quantisation, and about 15 updates per second per sender-receiver pair after interest management. A full 80-player instance then needs roughly 80 × 79 × 100 B × 15/s ≈ 9.5 MB/s (about 76 Mbit/s) of server egress, before voice. One modern server can host dozens of such instances.

### Instance types

Public, friends-plus, friends-only, invite-only and group instances, with group roles. Owners and moderators can mute, kick, ban, lock and set content limits for their instance, and those actions are enforced by the server.

## 12b. Distributed nodes, community servers and peer-to-peer

CIVBB uses a hub-and-nodes model: one hub owns identity, the social graph, content trust and discovery, and any number of nodes host the instances. Anyone can run a node; the official public nodes are simply the default. This is deliberately not full federation with several hubs: one trust root keeps global bans, CSAM scanning and signed content workable. Federation can be added later if the community wants it.

&#91;embedded content: hub and nodes · 3 kinds of node\]

Everything that runs on a client is still compiled and signed by the hub, whichever node the instance is on, so a community node cannot push unverified content to players.

### The node manager suite (`civbb-node`)

- **One binary and Docker image that runs many instances** with capacity limits (cores, bandwidth, player count), plus a Helm chart and Agones setup for large operators.
- **Web dashboard and CLI:** instances, players, admin roles, logs, Prometheus metrics, update channel.
- **Persistent servers, FiveM-style:** a fixed world with its own server-side scripts, mods and rules, always online under a stable name.
- **Registration:** the node generates a key pair and registers with the hub, giving operator account, region, rules, age rating and a contact address. Public nodes appear in the hub's node browser.

### Creating instances

1. When creating an instance, the user picks a node; the default is the official public node.
2. The hub asks that node's API to create it. With capacity, the node creates the instance and returns its address; when full, the client offers other nodes.
3. Instances created from inside an instance (portals, world scripts, the menu) go to the same node unless another node is named.
4. Joining needs both checks: the hub's (global bans, age status) in the join token, and the node's own rules (allowlists, node bans, paid slots).

### What nodes may and may not do

- **May:** set their own rules and moderation, monetise however they like (store perks, donations, their own memberships), and run server-side scripts and mods that execute only on the node.
- **May not:** send clients content the hub has not signed, ignore hub-level bans, or hide who operates them. Nodes that break the node terms are delisted and their keys revoked.
- A node's operator is responsible for what happens on it, legally as well; node terms make that explicit.

### Peer-to-peer

- Any client can start the embedded node and host friends-only or invite-only instances. QUIC hole punching connects peers directly, with official relays as the fallback.
- **Trade-off:** a direct connection shows the host and guests each other's IP addresses. The client warns before that happens, and relay mode, which hides IPs at some latency cost, is the default when guests aren't friends.
- A peer-hosted instance ends when the host leaves; host migration can come later.

### Privacy across nodes

Every node necessarily sees the IP addresses of its players. Before the first join to a community node, the client shows who operates it and its privacy policy. Official nodes and relays never share IP addresses.

### Node licence and tiers

The node software is AGPL-3.0 (D3): anyone can run it, and anyone who hosts a modified node must publish their changes. An open-source licence can't cap how many instances someone runs, so scale-based rules live in the hub's node terms instead: they govern connecting to the official network, not using the software.

| Tier | Who | Limits | Obligations |
| --- | --- | --- | --- |
| Personal | You and your friends | Up to `PERSONAL_NODE_MAX_INSTANCES` concurrent instances (default 3); not listed publicly | Just an account |
| Community | Anyone above the cap, or anyone listed in the public node browser | None from the hub | Verified operator contact, accepted node terms, a moderation commitment, and a small fee for hub services |
| Official | The project | None | The project's own policies |

The cap is a hub setting that can be changed at any time without a release. The default of 3 lets a group split off once and still leaves a spare instance for when everyone moves; a second split fills the node. If real use shows 4 works better, it's a config change.

The fee pays for what community nodes use from the hub (relays, listing, directory and join-token traffic, CDN bandwidth for their players), never for the software, which stays free under AGPL. It should be cost-covering and published openly; the amount is set once real hosting costs are known.

## 13. Voice and audio

Voice travels as Opus through the instance server, which forwards streams without mixing them, and every client spatialises locally with Steam Audio. Keeping voice inside the instance server means one connection, the same interest management as poses, and no separate voice service to run.

### Voice pipeline

1. **Capture:** 48 kHz, noise suppression (an RNNoise-class model), echo cancellation for desktop users on speakers (the open-source WebRTC audio processing module), automatic gain control.
2. **Encode:** Opus at 24 to 32 kbit/s, 20 ms frames, discontinuous transmission when silent.
3. **Forward:** QUIC datagrams to the instance server, which forwards each listener the nearest and loudest speakers up to a cap. The server only forwards; it never decodes or records.
4. **Play:** jitter buffer, decode, then spatialise with Steam Audio HRTF, distance falloff and occlusion against a simplified acoustic mesh baked at compile time.

### Voice features

- Per-user volume, mute and block; push-to-talk; earmuffs mode (hear only people close to you).
- Worlds can change voice falloff and set up stages or microphones that broadcast a speaker to the whole instance, as legacy Udon worlds already do.
- **Loudness protection:** every incoming voice and avatar sound passes a normaliser and hard limiter on the listener's side, so no one can blast the room.

### World and avatar audio

- Unity AudioSources map to our mixer graph with the same spatial blend, rolloff and doppler settings; reverb zones map to Steam Audio reverb.
- Video players feed their audio into the same graph, and the AudioLink analyser reads from it (section 10).
- Avatar audio is capped in volume, distance and number of sources, and every user can switch it off.

### Voice evidence for reports (D7)

Decided: no voice capture at launch, but the design keeps room for the approach the big platforms use.

| Platform | What they do |
| --- | --- |
| [Meta Horizon Worlds](https://www.meta.com/help/quest/796650364312013/) | The headset keeps a rolling buffer of the last few minutes, processed locally and overwritten; it is sent only with a report |
| [Xbox](https://news.xbox.com/en-us/2023/07/12/xbox-voice-reporting/) | Only the player can capture the last 60 seconds for a report; nothing is uploaded unless they start one |
| [Roblox](https://about.roblox.com/newsroom/2025/07/roblox-ai-moderation-massive-scale) | Goes further: a real-time ML classifier checks voice chat, and the model is [open source](https://github.com/Roblox/voice-safety-classifier) |

When CIVBB adds it, it follows the Meta and Xbox pattern: a rolling buffer of the incoming voice you heard (a few minutes), kept only in memory on your own device, attached only if you file a report, and deleted once the report is resolved. Roblox's open model could later power an opt-in "mute assist" that runs locally.

## 14. Backend services and data

Start as a modular monolith: one TypeScript API service plus three Rust services where performance or long-lived connections demand it. Split further only when load or team size forces it, because a social platform's early risk is shipping slowly, not scaling.

### Services

| Service | Language | Responsibility |
| --- | --- | --- |
| API (modular monolith) | TypeScript (Node, Fastify or Hono), OpenAPI | Accounts, social graph, groups, content metadata, favourites, search, moderation case tools, creator dashboard API |
| Realtime gateway | Rust | One persistent connection per client: presence, friend updates, invites, notifications, fan-out from NATS |
| Instance directory | Rust | Live instances, capacity, join tokens, Agones allocation |
| Content pipeline workers | Rust | The compiler in section 9, run as sandboxed jobs (section 15) |
| Identity | Off-the-shelf: Ory Kratos + Hydra, or Zitadel | Logins, passkeys, TOTP, OAuth for third parties, Steam and Meta sign-in |

### Data stores

| Store | Holds |
| --- | --- |
| PostgreSQL | Accounts, friends, blocks, groups, content metadata, moderation cases, world persistence (JSONB, partitioned) |
| Valkey | Sessions, presence, rate limits, hot caches |
| NATS JetStream | Events between services, presence fan-out, pipeline job queue |
| ClickHouse | Telemetry, performance data, product analytics |
| Meilisearch (OpenSearch later) | Search over worlds, avatars, groups, users |
| Object storage (Cloudflare R2) | All content blobs and sources (section 15) |

The primary database lives in the EU (Frankfurt) for GDPR simplicity, with read replicas near the other server regions. Social writes tolerate the extra latency; reads stay local.

### Operations

- Development and the first public test servers run on one machine with Docker Compose (PostgreSQL, Valkey, NATS, MinIO as S3 storage). Kubernetes, Argo CD and OpenTofu come in only when real load does.
- Secrets in OpenBao, synced to workloads with External Secrets Operator.
- Blue-green deploys for the API; Agones drains instance servers so updates never kick players.
- A web companion app (and later a mobile one) for friends, invites, avatar management and events, built on the same API.

### Creator economy (optional)

A marketplace for avatars and assets with creator payouts through Stripe Connect is possible but heavy on compliance: tax, consumer law and money transmission. Decided (D5): not at launch, maybe later. The architecture leaves room for it without depending on it.

## 14b. Official store

One optional, hub-run store in the style of Tebex, the way FiveM and Minecraft server owners sell perks. It exists so good creators can earn, not so everything gets a price tag: worlds are free, opening instances is free, and most avatars people wear stay free. What a user buys is an entitlement recorded at the hub and checked by clients and nodes. Nodes stay free to monetise any other way they like.

### What can be sold

| Item | Seller | What the buyer gets |
| --- | --- | --- |
| Cosmetics (accessories, outfits, effects) | Creators | The right to wear the item; it attaches to any avatar through humanoid bones |
| Avatars | Creators | Use of the compiled avatar; the source package only if the seller also sells it, like a Booth package |
| World licences | World creators | Optional: only for creators who charge commercial nodes for hosting; public worlds are free to visit and to host |
| Node perks (reserved slots, priority queue, supporter badges, node-only cosmetics) | Node operators | Benefits on that node only |

### How entitlements work

- The hub stores entitlements and issues short-lived signed entitlement tokens (PASETO). Nodes check them at join without calling back for every action.
- **World licences are bound to a node's key.** The pipeline signs a world manifest that names the licensed node, so another node can't host it and clients refuse it elsewhere. This is the same idea as FiveM's escrow for paid resources.
- All sold content goes through the normal pipeline (section 15): scanned, compiled and signed like everything else.

### Payments

A merchant-of-record provider (Paddle, Lemon Squeezy or Tebex itself) sells on the project's behalf, so VAT, chargebacks and consumer law sit with them instead of with a hobby project. The store keeps a small fee to fund official hosting (D8). Paid items are age-gated, and there are no loot boxes or random paid items, which are legally risky in several EU countries.

Like the marketplace in D5, the store comes after the core platform works.

### Keeping it free

A social platform with nobody in it sells nothing, so these rules protect the free core first:

- **Sell creation, not access.** The store leads with what creators build from: bases, outfits, accessories, shaders and tools. That is the Booth model, which already works, and the avatars people assemble from those parts stay free to wear.
- **Official nodes never charge for entry.** Paid entry exists only on community nodes that choose it, and the node browser labels those clearly.
- **Discovery is not for sale:** no ads, no paid ranking, and the store never shows up inside worlds unless the world's creator puts it there.
- **Try before buying:** every paid avatar or cosmetic can be tried on in a mirror first.
- **Supporter perks are convenience, not access:** a badge, more upload quota, early previews.
- **A small, published platform fee** that covers payments and official hosting, nothing more.

## 15. UGC upload, processing, storage and delivery

Uploads land in a quarantine bucket, get processed in throwaway microVMs, and only signed, content-addressed output ever reaches the CDN. Deduplication by content hash matters a lot here: a large share of avatars are built on the same few dozen commercial base models, so their meshes and textures are stored once.

### Upload

1. The SDK asks the API for an upload session; the API checks quota, content type and account standing.
2. The API returns presigned multipart URLs into a **quarantine bucket** (no public access, 7-day expiry). Uploads are resumable in 16 MB parts, and each part's BLAKE3 hash is checked on arrival.
3. Completion enqueues a pipeline job on NATS JetStream.

### Processing

- Each job runs in a **Firecracker microVM** with no network except the storage endpoint, hard CPU, memory and time limits, and a fresh root filesystem. Our parsers are Rust; external tools such as DXC and texture encoders run inside the same VM.
- **Defensive checks first:** file-type allowlist, archive-bomb and decompression limits, polygon, texture and bone caps, malware scan of archives.
- **Compile** (section 9), then the **GPU stage** on a separate pool of GPU nodes for cost measurement and parity renders.
- **Moderation scans:**
  - CSAM hash matching (PhotoDNA or Thorn Safer) on every texture, thumbnail and render; matches are blocked and reported as the law requires.
  - NSFW classification of thumbnails and turntable renders, used to rate content rather than to remove it.
  - Text moderation for names and descriptions.
  - **Re-upload detection:** perceptual hashes of textures and geometry fingerprints are compared with other users' content and with known ripper output. Matches go to review, which is the main practical defence against ripped avatars.
- Outcome: publish, send to human review, or reject with a reason the creator can act on.

### Storage

| Bucket | Contents | Access |
| --- | --- | --- |
| Quarantine | Raw uploads | Pipeline only; expires after 7 days |
| Sources | Creator source packages | Pipeline only; encrypted per creator with keys held in OpenBao (transit engine); never served |
| Runtime | Compiled, deduplicated blobs and signed manifests | CDN with short-lived signed URLs |
| Archive | Superseded versions | Cold storage; rollback and legal holds |

- **Versioning:** every publish creates a new manifest; recent versions stay live for rollback.
- **Quotas** per account, with room for a supporter tier.
- **Garbage collection:** blobs are reference-counted from manifests and swept after a grace period.
- **Takedowns:** revoking a manifest purges it from the CDN and adds it to a revocation list that clients check on join.

### Delivery

- **Cloudflare CDN in front of R2.** R2 charges no egress fees, and egress is usually the biggest variable cost of a UGC platform.
- The client fetches the manifest, diffs it against its local cache and downloads only missing blobs over HTTP/3, lowest detail first: impostor, then low LOD and small mips, then full quality.
- The local cache is content-addressed with a user-set size limit, and every blob is verified against its hash when read. The manifest signature is checked with the platform key, so neither a compromised CDN node nor a tampered cache can inject content.

### On ripping, honestly

Anything a client can render, a determined attacker can extract. The design raises the cost (no plugin access to others' content, encrypted transport and cache, signed URLs), but the real protection is detection at upload and a fast takedown process, not secrecy.

## 16. Safety, security and moderation

Most of VRChat's safety problems come from content and clients doing things they shouldn't be able to do. Here the architecture removes whole classes of them (crashers, IP leaks, malicious clients), so moderation can focus on people's behaviour.

### Threat model

| Threat | Mitigation |
| --- | --- |
| Crasher avatars (CPU, GPU, memory) | Server-side compile, shader validation, hard caps, runtime governor (sections 8, 9, 10) |
| Malicious scripts | Wasm sandbox with fuel, memory caps and no direct I/O (section 11) |
| Exploits in file parsers | Rust parsers, Firecracker isolation, continuous fuzzing of every parser |
| Exploits through network packets | Memory-safe client and server, strict server validation, fuzzed protocol |
| IP leaks and doxxing | All traffic through servers; remote URLs through the proxy |
| Account takeover | Passkeys, TOTP, new-device alerts; plugins never see session tokens |
| Ripped content | Re-upload detection, takedowns (section 15) |
| Spam and bots | Rate limits, trust levels, gated uploads and public-instance creation for new accounts |
| Supply-chain attacks | Signed plugins and builds, `cargo-deny` and `cargo-vet` audits, SBOMs |

### Personal safety tools

- **Block** is enforced by the server in both directions: the blocked person cannot see or hear you either.
- A **personal space bubble**: other avatars fade out when they get closer than a distance you choose.
- **Panic button:** one action hides every avatar except friends' and mutes everyone except friends.
- Per-user contact permissions (who can trigger contacts on your avatar), avatar-cloning permissions, and limits on who may join you.

### Moderation

- **Trust levels** grow from account age, activity and good standing, and gate uploads and public instance creation. They are not shown as a status badge, to avoid VRChat's rank-chasing.
- **Reports from inside the world** carry context automatically: instance, who was present, timestamps and an optional screenshot. A voice clip can be attached once the report buffer exists (section 13).
- Case management with queues, evidence, action history and **appeals**, as the EU Digital Services Act requires: a statement of reasons for every action and an internal complaint process.
- Group, instance and node moderators get the same tools at their own scope; node operators moderate their own nodes, while global bans and illegal content stay with the hub (section 12b).
- Transparency reports on the schedule the DSA sets for the platform's size.

### Age and content policy

Decided direction (D6): age verification and age-locked instances are planned, but after the core works, not at launch. Until then, keep the hooks in place so they can be added without a redesign: an age-status field on every account, an age requirement on every instance and content rating, and server-side checks at join. Third-party age verification plugs in behind that field later. The minimum age and adult-content rules still need a decision before any public servers open.

### Security engineering

- Continuous fuzzing of parsers, the network protocol and the Wasm host API; external penetration tests before beta; a bug bounty after launch.
- Content-signing keys in an HSM or OpenBao, with rotation; reproducible client builds.

## 17. User integrations and platform APIs

Every tool the VRChat community already uses over OSC works unchanged on day one, and there is an official, documented web API with OAuth, which VRChat has never offered. Integrations become a reason to switch instead of a reason to stay.

### OSC and OSCQuery (drop-in compatible)

- Same address scheme and default ports as VRChat: `/avatar/parameters/*`, `/input/*`, `/chatbox/*`, `/tracking/trackers/*`, with OSCQuery for discovery.
- Existing tools then work without changes: VRCFaceTracking, chatbox apps, OSC haptics bridges, heart-rate monitors, SlimeVR's OSC trackers.

### Tracking and hardware

- OpenXR for headsets, controllers, hand tracking, eye and face tracking, and body tracking where the runtime offers it.
- SteamVR-based trackers (Vive, Tundra, SlimeVR) through the runtime; virtual trackers through OSC or a local tracker API.
- A **tracker input API** for third-party tracking sources, so camera-based full-body tracking or custom hardware can feed poses directly into the IK solver.
- Haptics on controllers plus OSC-driven vests and bands.

### Public web API

- REST with OpenAPI docs, OAuth 2.0 scopes (profile, friends, presence, avatars, groups, events) and per-app rate limits.
- **Webhooks** for groups: member joins, event starts, instance opens.
- Official TypeScript and Python SDKs generated from the OpenAPI spec.

### Third-party platforms

- **Discord:** rich presence, account linking, group-role sync.
- **Twitch and YouTube:** a streamer camera, chat overlays in-world, live event pages.
- **Calendars:** iCal feeds for group events.
- **Steam and Meta:** sign-in, platform entitlements, store presence.
- **Asset stores:** where storefronts offer licence verification, creators can link purchases to prove base-model licences (section 20).

### Data portability

Users can download all their data and every source package they uploaded at any time. That satisfies GDPR portability, and it is also a promise to creators: their work is never locked in.

## 18. Platforms and hardware

Vulkan plus OpenXR covers Windows, Linux and Android headsets with one renderer and one XR layer. That makes native Linux and Quest standalone cheap to support compared with VRChat's per-platform builds.

| Platform | Tier | Notes |
| --- | --- | --- |
| Windows 10/11, PC VR | Launch | Any OpenXR runtime: SteamVR, Meta Link; streaming apps (Virtual Desktop, Steam Link, ALVR) work through those runtimes |
| Windows, flat desktop | Launch | Mouse, keyboard, gamepad |
| Linux, PC VR and desktop | Launch, tier 2 | SteamVR or Monado; WiVRn for Quest streaming; native, not through Proton |
| Meta Quest 3 / 3S standalone | Phase 3 | Android, Vulkan, OpenXR; Quest content variants and strict defaults |
| Meta Quest 2 | Phase 3, with Quest 3 | Supported for reach (D4): its own content variant with the strictest caps and earlier impostors |
| Other Android OpenXR headsets (Pico, Android XR devices) | Tier 2 after Quest | Same build with per-device profiles |
| Android and iOS phones, flat | Later | iOS needs Metal (through MoltenVK) and runs Wasm in the Pulley interpreter, since JIT is not allowed there |
| Apple Vision Pro, web browser | Not planned | Revisit after launch |

### PC requirements (draft)

|  | Minimum | Recommended |
| --- | --- | --- |
| GPU | Vulkan 1.3, GTX 1070 / RX 5600 class, 6 GB VRAM | RTX 3070 / RX 6800 class or better, 8 GB+ |
| CPU | 4 cores / 8 threads | 6+ performance cores (the engine scales with cores) |
| RAM | 16 GB | 32 GB |

### Comfort and accessibility

Seated and one-handed modes, snap and smooth turning, a comfort vignette, UI text scaling, colour-blind-safe UI, and live captions through the captions plugin (section 11).

## 19. Observability, testing and QA

The two promises that make or break this project, "it's faster" and "your stuff still works", are both tested automatically every night against real content on real hardware. A regression in either fails the build.

### Test suites

| Suite | What it proves | Runs |
| --- | --- | --- |
| Compatibility corpus | A growing set of real avatars and worlds (shared by creators with permission, plus open-source prefabs) still convert, with FLIP parity scores tracked over time | Nightly |
| Animator conformance | Our state machine output matches Unity frame by frame for a library of test controllers, Write Defaults combinations included | Every merge |
| Performance lab | Fixed benchmark scenes (40 avatars, a heavy world, a mirror) on reference machines, including a 12600K + RTX 5070 Ti-class PC and a Quest 3; more than 3 % frame-time regression fails | Nightly |
| Network load | Headless bot clients fill instances and regions; chaos runs with packet loss, jitter and server kills | Weekly and before releases |
| Fuzzing | Parsers, protocol, Wasm host API | Continuously |
| Unit and property tests | Everything else | Every merge |

### Telemetry

- **Client (opt-in):** frame-time histograms, time per system, observed cost per avatar (by content hash), crash minidumps with a symbol server, GPU crash dumps from vendor tools.
- **Servers:** OpenTelemetry into Prometheus, Grafana, Loki and Tempo; Agones fleet dashboards.
- **Service objectives:** join success rate, time to first frame, instance crash rate, API p99 latency, pipeline time from upload to publish.

### Release channels

Nightly, beta and stable clients, selectable in the launcher. Protocol versions are negotiated, and servers support the current and previous client version, so a channel switch never splits friends into incompatible instances.

## 20. Legal, compliance and licensing

The migration feature is the legal centre of gravity: it must work only on data creators own, and it must not reuse VRChat's code or brand. Everything in this section needs review by a lawyer before launch; it names the questions, it does not answer them.

### As a hobby project

No company does not mean no rules: whoever runs public servers is the operator under GDPR and the DSA, and an operator in Germany needs an Impressum. The low-risk path is to develop in the open without running a public service, keep test servers invite-only among people you know, and move the public service under a legal entity (an association such as an e.V., or a small UG) before strangers can join, so the liability does not sit on one person.

### Licence (D3)

Decided: open source from the first commit, MIT OR Apache-2.0 for the engine, client, SDKs and formats, which is the Rust ecosystem's default and lets anyone build on it. Decided: the node (instance server) is AGPL-3.0, so anyone hosting a modified node must publish their changes. Engine, client and SDKs stay MIT OR Apache-2.0, so worlds, plugins and tools built on them can use any licence. Contributions come in under the same licences, with a DCO sign-off instead of a CLA.

### Interoperability with VRChat content

- No VRChat trademarks, branding or UI copies; no affiliation claims.
- The exporter does not ship, link or modify the VRChat SDK. It reads the serialised data of the creator's own project through Unity's API. The VRChat SDK is distributed under VRChat's own licence terms, so how far reading SDK component data is allowed needs specific legal review.
- Only creator-owned source projects are accepted, never built bundles or CDN downloads (section 2).

### Asset and base-model licences

- Many avatars are built on commercial base models whose licences (for example the VN3 template common on Booth) can restrict which platforms the model may be used on. The platform keeps a database of popular bases and their licence terms, asks creators to attest their rights, and flags uploads whose base licence may not allow it.
- Licensors get a fast takedown path, and the platform should approach the large base-model creators early, since their permission unlocks most avatars at once.
- **Shader ports:** native versions of Poiyomi, lilToon and other families must be clean-room reimplementations or built in partnership with their authors, depending on each project's licence. A partnership is the better route: the authors then maintain the official port.

### Third-party software licences

The core stack is permissively licensed: Jolt (MIT), wasmtime (Apache-2.0 with LLVM exception), Steam Audio (Apache-2.0), meshoptimizer (MIT), Basis Universal (Apache-2.0), ACL (MIT), Opus (BSD). FFmpeg must be built and linked as LGPL only, without GPL components. A licence scan runs in CI.

### Regulation (EU first, since the company would be in Germany)

| Area | Obligations to plan for |
| --- | --- |
| GDPR | Lawful basis per data type, processor agreements, EU hosting of the primary database, opt-in telemetry, erasure that reaches sources, CDN and backups |
| Digital Services Act | Notice and action, statements of reasons, internal complaints, trusted flaggers, transparency reports; much heavier duties only past 45 million EU users |
| Child safety | CSAM detection and reporting duties in the EU and US; age-assurance rules such as the UK Online Safety Act; national under-16 social-media rules need checking for whether they cover social VR |
| Consumer and commerce law | Terms for paid features, withdrawal rights, Impressum, VAT |
| US | DMCA agent registration and takedown process, COPPA if under-13s could sign up |

### Agreements

Terms of service, privacy policy, a creator agreement (the licence creators grant so the platform can store, compile, transform and distribute their content), a plugin developer agreement and an API terms page.

## 21. Roadmap, team and cost

As an open-source, AI-assisted project (D0), the phases stay the same but each gate becomes a public milestone that shows contributors something real. Phase 0 is the whole plan for a solo start: it answers the riskiest question, whether migration works well enough, before the engine grows.

&#91;embedded content: roadmap · 5 phases, 4 gates (estimates)\]

Phase 0 is highlighted because it is where to start. A reduced version (one avatar shader family, a desktop viewer, a benchmark against Unity) fits a single developer in a few months.

### Phase 0 in detail

The question Phase 0 answers: can popular, heavy avatars be compiled into a much cheaper form that looks and behaves the same? The test corpus costs nothing, but still has to cover what makes popular paid avatars heavy: extreme shape-key counts, many outfits and lots of physics.

| Corpus slot | Source (free) | What it tests |
| --- | --- | --- |
| Popular free Booth avatars, 3 to 5 | Free releases that rank high on Booth; Yumeka, for example, was the most-liked new avatar in September 2026 and is free ([source](https://vrcfinder.net/en/collection/recent-assets/avatars/)) | The real-world setup: lilToon, many shape keys, PhysBones, toggles |
| Deliberately bloated avatars | Built for the project from a free base plus CC0 assets: stacked outfits, dozens of toggles, a nine-tail rig, 1,000+ shape keys | Worst cases on purpose |
| Control | One free avatar that is already well optimised | That the compiler never makes a good avatar worse |

**Licences:** free to download doesn't mean free to redistribute. The Booth avatars stay on your machine and out of the public repo; bloated avatars built only from CC0 or your own assets can be committed.

**Non-destructive tools:** many real avatars only become their final form at build time through Modular Avatar, VRCFury or NDMF. The exporter has to run that build step inside Unity first and export the result, or kitbashes will look broken.

**Deliverables**

1. Unity exporter, including the non-destructive build step and the reference renders.
2. Converter for meshes, rigs, animators, PhysBones, contacts and constraints, with lilToon and Poiyomi material mapping first (they cover most avatars).
3. Compiler optimisations: bone and shape-key pruning, mesh merging, toggle layers collapsed into direct blend trees, texture atlasing, PhysBone merging.
4. A small Rust desktop viewer that runs N copies of an avatar with animator and dynamics and measures CPU and GPU time.
5. A report per avatar: before and after stats, FLIP parity for every toggle state, time per frame.

**How success is measured:** the baseline is the same avatar in Unity with an Avatars 3.0 emulator, plus what existing optimisers (Anatawa12's AvatarOptimizer, d4rk's optimizer) achieve. Phase 0 passes when the compiled avatars stay visually identical (FLIP within threshold in every state) and cost clearly less CPU per avatar than those baselines.

### Running it as an open-source project

- **One monorepo** with crates for engine, client, instance server, content pipeline and SDKs; CI on GitHub Actions; short architecture decision records (ADRs) for every choice in this doc.
- **Tests are the spec, especially with AI-written code.** The animator conformance suite, the parity renders and the performance benchmark come first, and every change must pass them. AI can write most of the code, but a human reviews anything at a trust boundary: the Wasm sandbox, the network protocol, file parsers and the upload pipeline.
- **An AI-contribution policy from day one.** Open-source projects are adding these now; ALVR, for example, [added an AI usage policy](https://github.com/alvr-org/ALVR/pull/3383) to its contributor guide in September 2026.
- **Costs:** phases 0 and 1 cost almost nothing beyond your own PC and a cheap test server. Real costs start with public hosting (storage, CDN traffic, servers, moderation time), which is when funding matters (D8).
- **Funding (D8):** donations through GitHub Sponsors or Open Collective, with public accounts of what hosting costs; community-hosted servers (D10) can carry part of the load.

For scale, a funded team would need about 25 to 35 people and roughly €5 million to reach open beta; that figure is the yardstick for how much AI assistance and volunteer help has to replace.

### Code organisation: one core monorepo, a few satellites

The rule: **code that has to change in the same commit lives in the same repo.** A protocol, content-format or script-API change touches the client, the node, the pipeline and the TypeScript SDK at once. In one repo that is one pull request and one CI run; across repos it is a version matrix nobody maintains. Tools that live in other ecosystems, with their own packaging and release rhythm, get their own repos and depend only on published, versioned contracts.

| Repo | Contents | Why |
| --- | --- | --- |
| `civbb` (monorepo) | Rust workspace (engine, client, node, pipeline, hub services), TypeScript packages (API, dashboards, script SDK), WIT and protocol definitions, docs, test corpora | Everything that shares the protocol, format and API |
| `civbb-unity-exporter` | C# Unity editor package | Unity's own package tooling and release cadence |
| `civbb-blender` | Python Blender add-on | Blender's own packaging |
| `civbb-plugin-template`, `civbb-world-template` | Starter repos | What community creators fork |

Monorepo layout:

```
civbb/
  crates/
    format/        content format types + validation, no engine deps
    protocol/      network messages and version negotiation
    wit/           the script API: single source of truth for every SDK
    core/          ECS, job system, math, asset handles
    render/  anim/  dynamics/  audio/  xr/  ui/  script-host/
    client/        app binary: wires the systems together
    node/          instance server + node manager
    pipeline/      content compiler
    hub-realtime/  hub-directory/
  packages/        TypeScript (pnpm workspace)
    api/  web/  node-dashboard/  sdk/ (types generated from wit/)
  tools/xtask/     build, codegen, architecture checks
  docs/            ARCHITECTURE.md, ADRs, format and protocol specs
  tests/           conformance, parity and performance scenes
```

**Dependency rules, enforced in CI by `xtask`, not by goodwill:** `format`, `protocol` and `wit` depend on nothing internal. Engine crates depend only on `core` and `format`. `client`, `node` and `pipeline` are the only glue crates, and nothing imports from them. `node` never depends on `render`. Every crate has a README stating its one job, and public items without docs fail the build.

### Patterns teams actually live by, and the ones that stay theory

| Actually lived | Mostly theory, or lived badly |
| --- | --- |
| A short `ARCHITECTURE.md`: a map of the code that stays current because it is short | Big design documents kept in sync with code; they rot. This doc should become ADRs plus `ARCHITECTURE.md` once code exists |
| ADRs: one page per decision, never edited after acceptance | UML and diagrams of the whole system |
| Modular monolith with hard module boundaries | Microservices from day one |
| Ports and adapters only at real I/O edges (storage, network, payments) | Clean or hexagonal architecture applied to every layer |
| Bounded contexts as crates or modules (the strategic half of DDD) | DDD's tactical patterns everywhere (aggregates, repositories for everything) |
| Trunk-based development, short branches, feature flags | GitFlow with develop, release and hotfix branches |
| Boundaries enforced by tooling: crate graphs, lints, CI checks | Boundaries enforced by convention and code review alone |
| Generated code from one source (WIT to TypeScript types, protocol schema to Rust and TS) | Hand-synced types in several languages |
| Tests at boundaries: conformance, golden files, snapshots, critical paths | 100 % coverage targets, which get gamed |
| Multi-repo only behind stable public contracts | Multi-repo with internal semver between your own repos |
| One command to run everything locally | A wiki page of setup steps |

### Staying maintainable without AI

- Small crates with one job, plain names, no clever macros unless an ADR justifies them.
- Boring, widely used dependencies; every new dependency gets a one-line reason in the PR.
- `rustfmt`, `clippy` (pedantic where it helps), `eslint` and `tsc --strict` all run in CI and block merges.
- AI-written code meets exactly the same bar as human code: it must pass the suites, follow the layering, and be explainable by the person who merges it.

## 22. Risks and mitigations

The biggest risk is not technical: people go where their friends are, and a better engine does not move a community by itself. Linden Lab's Sansar was technically ahead of Second Life and still failed for that reason.

| Risk | Likelihood | Impact | Mitigation |
| --- | --- | --- | --- |
| Users don't move; the network effect wins | High | Fatal | One-click migration; win specific groups first (performance-starved PC users, Linux users, creators, event organisers); partner with popular creators and event series; invite links that work without an account |
| Custom engine scope grows past the budget | High | Severe | Phase gates with a vertical slice first (section 21); Unity 6 kept as a documented fallback (D1) |
| Shader compatibility is worse than expected | Medium | Severe | Measure on a real corpus in phase 0, before committing; partner with shader authors; fallback bake |
| Legal challenge over migration or base-model licences | Medium | Severe | Creator-owned sources only, no SDK code, early legal review, licensor relationships |
| A child-safety incident | Medium | Fatal | No open public servers until a moderation plan and age policy exist (D6) |
| CDN, server and GPU-pipeline costs outrun revenue | Medium | Severe | R2 with no egress fees, bare-metal base load, deduplication, supporter tier, per-account quotas |
| Animator semantics drift from Unity's | Medium | Medium | Frame-by-frame conformance suite against Unity |
| Quest misses its performance targets | Medium | Medium | Strict Quest defaults, impostors, decision on Quest 2 (D4) |
| AI-written code nobody fully understands (rendering, sync and sandbox bugs) | Medium | Medium | Conformance and benchmark suites as the spec; human review at trust boundaries; small, documented crates |
| A one-person project stalls (time, burnout, scope) | High | Severe | Phase 0 sized for one person; public milestones and good first issues to attract contributors |

## 23. Open decisions and questions for Keynoh

Almost everything is decided; only the final name and the hub-service fee amount remain open.

| # | Decision | Status | Where it shows |
| --- | --- | --- | --- |
| D0 | What is this? | Decided: open-source hobby project, built with heavy AI assistance, maintainable without it | Sections 1, 20, 21 |
| D1 | Engine | Decided: custom Rust + Vulkan | Section 4 |
| D2 | Scripting languages | Decided: C# subset, Lua (Luau), strict TypeScript with generated types, plus Rust and graphs | Section 11 |
| D3 | Licence | Decided: open source; the node is AGPL-3.0, everything else MIT OR Apache-2.0 | Section 20 |
| D4 | Quest 2 | Decided: supported, with the strictest content variant | Section 18 |
| D5 | Creator marketplace | Decided: later, as the official store | Section 14b |
| D6 | Age verification and age-locked instances | Decided: planned after the core works; hooks built in from the start | Section 16 |
| D7 | Voice evidence for reports | Decided: later, as a local rolling buffer sent only with a report | Section 13 |
| D8 | Funding | Decided: free by default; paid items are mostly creator assets; a small store fee and donations fund official nodes; community nodes monetise their own way | Sections 14b, 21 |
| D9 | Name | Codename CIVBB ("Chatting in VR but Better"); final name open | Title |
| D10 | Community nodes and peer-to-peer | Decided: hub-and-nodes with official, community and peer-to-peer nodes | Section 12b |

### Ideas worth considering

- **Phase 0 stands on its own.** A converter plus a desktop viewer that loads a VRChat avatar project, runs its animator and PhysBones on all cores and benchmarks it against Unity is useful by itself, fits one developer and makes a strong portfolio project.
- **A tracker input API from day one** (section 17) lets camera-based full-body tracking projects plug straight into the IK solver.
- **Ask the big base-model and shader authors early.** A handful of partnerships (base models, Poiyomi, lilToon) unlock most avatars legally and technically at once.

### Questions

- [ ] Is Quest standalone a must-have for launch, or can it follow PC?
