# 0048: Unity reference renders reproduce our rig, light and runtime from a hand-off

Status: accepted
Date: 2026-11-19

## Context

Design doc §10 (export path step 3, parity scoring): the exporter renders reference images inside Unity with a
fixed camera rig and every state, and FLIP scores our renders against them per state. ADR 0033 scores the compiled
avatar against its own import in our renderer and planned Unity as a second reference "isolating shader and runtime
translation"; ADR 0036 frames the rig from the rest pose's skinned world-space vertices. #79 asks for the Unity
side: the avatars' shader packages in the SDK host project, a batch render method with fixed lighting, and scores of
the 13 local exports per shader family to rank ADR 0043's material mappings.

The host project had none of the shader packages its avatars use. The exports' `X_toon_material` GUIDs (ADR 0043)
give the families: lilToon (Cokoro, HAOLAN PC, Tessakuchan, Yumeka; Ostgaroa's lilToon Multi), UTS2 (Inuinu),
Poiyomi Toon 7.3 (KohzieBoi: its GUID is 7.3's in Poiyomi's own legacy GUID table), and shaders that ship with the
avatars or the SDK (Taro's locked Poiyomi copy, Blackbird's TSF, VRChat Mobile).

## Decision

**Shader packages.** Installed into the gitignored host project only, nothing committed: lilToon 2.4.1 (MIT; its
`currentVersionValue` 45 is the exports' `_lilToonVersion`, HAOLAN's 33 is migrated by lilToon) as the embedded
package `Packages/jp.lilxyzw.liltoon`; Poiyomi Toon 7.3.050 (MIT) and UTS2 2.0.9 shader-only (Unity-Chan License
2.0, which permits use; we distribute nothing of it) unpacked from their `.unitypackage` files with GUIDs kept, so
the saved materials find their shaders. They carry editor code (lilToon generates its shaders through a scripted
importer), so they do not go through `Batch.Import`, which refuses code.

**Hand-off, not shared maths.** Our side decides everything and Unity reproduces it: `civbb-client rig
<package>` prints `parity::Handoff` as JSON (`rig.json`): the ADR 0033/0036 rig (its bounds need our skinning, so
Unity does not recompute them), light, `dt`, frames per state, every animator parameter's start value and the
states (rest, then one parameter per state, as `parity::states` steps them). Avatar space as ADR 0006; Unity
mirrors X as the exporter does. `civbb-client parity <dir> <test>` takes a directory of Unity renders as the
reference (`"reference": "unity"`), checks each state's name against the hand-off, and scores as ADR 0033 does.

**Reference avatar.** The import without any compiler stage (`civbb build --unstaged`), so Unity against it
measures shader and runtime translation only; compiled against source stays ADR 0033's job.

**Render method.** `Civbb.Exporter.Reference.Render` (`tools/unity-exporter/Editor/Reference.cs`) in batch mode with
a graphics device (`tools/unity-batch.sh --graphics`), in play mode so PhysBones, contacts and constraints run, with
`Time.captureDeltaTime = dt`; a state renders after its frames, then the next state's parameter is set.

**Lighting (fixed, our viewer's).** One directional light, white, intensity 1, towards `light_towards`, no shadows;
ambient 0.3 as a flat ambient probe set directly (linear, `SphericalHarmonicsL2.AddAmbientLight`); no skybox, no
fog, reflection intensity 0; no post-processing. Camera: solid background (our linear colour, gamma-encoded as
Unity's colour properties take it in a linear project; the PNGs' background is our background texel exactly),
vertical field of view, near plane 0.05, no HDR, no MSAA, an sRGB ARGB32 target read back to PNG. Four skin weights,
skinned meshes updated off screen. The host project is in linear colour space, as VRChat projects are.

**The avatar as our runtime runs it.** Instantiated (as runtimes load avatars, ADR 0042) with the root reset to
identity (the exporter writes no root transform: the root is avatar space). The descriptor's playable layers that
the exporter exports (custom, non-SDK controllers) run in a `PlayableGraph`: groups in VRChat's order, Additive
additive, Action at weight 0, every parameter set on every controller that has it (VRChat shares parameters). Where
our runtime knowingly differs from VRChat, Unity follows ours, so poses match and the score measures shading:

- SDK clips (`proxy_*`, which VRChat replaces with built-in animations neither side has) play as empty clips
  through an `AnimatorOverrideController`, as the exporter references them without copying.
- The humanoid avatar stays only when a non-SDK clip animates muscles (our rule, ADR 0014); otherwise the animator
  binds generically and the bones keep the rest pose.
- The hips get their rest transform back before each render: our runtime does not apply the body pose yet (#52).
- State behaviours (parameter drivers, layer control) do not run: the SDK's are hooks into the VRChat client.

The descriptor's layer masks are applied, as VRChat applies them; our runtime ignores them, which is a difference
the scores show (#80).

## Consequences

- The 13 exports render in 12 to 20 s each in Unity. All are red (table on #79): the scores measure translation,
  not compiler stages, and ADR 0033's thresholds were set for our renderer against itself. Revisit them once
  shading converges.
- Shading findings, by family: our toon model brightens lit surfaces to albedo times (light + ambient) = 1.3, where
  lilToon, UTS2 and Poiyomi clamp the light to 1, so every toon avatar renders washed out (#81); lilToon emission
  and its Multi variant, VRChat Mobile MatCap and Poiyomi 7.3 are the largest mapping gaps (#82).
- Pose noise remains where runtimes differ (finger proxies, PhysBone sag, FX states): FLIP counts every misplaced
  edge, so a view's p99 stays near 1 until geometry matches. The per-view images in `--images` show which.
- Unity is not in `civbb build --report` yet: the report's `unity_parity` key (ADR 0033) needs the exporter to
  render during export on the creator's machine (design doc §10 step 3); this ADR provides the method and the
  scoring.
