# unity-exporter

CIVBB's Unity editor package (`civbb.unity-exporter`, ADRs 0037 and 0038): exports an avatar prefab to the source
package of design doc §9, glTF plus `X_*` extensions. Not a crate (excluded from the workspace). Editor-only;
links no VRChat SDK code.

- `Editor/Batch.cs`: batch entry points `Civbb.Exporter.Batch.Import` and `Civbb.Exporter.Batch.Export`.
- `Editor/UnityPackage.cs`: lists a `.unitypackage`'s asset paths so the import step can refuse packages with code.
- `Editor/Glb.cs`: the GLB writer: nodes, meshes, skins, morph targets, materials, textures, `X_avatar` (humanoid,
  muscle frames, viewpoint, face).
- `Editor/AnimatorExport.cs`: animator controllers and clips as `X_animator` (ADR 0041; curves per ADR 0039).
- `Editor/BehaviourExport.cs`: VRChat state behaviours as format behaviours (ADR 0042).
- `Editor/ConstraintExport.cs`: Unity constraint components as `X_constraints`.
- `Editor/VrcExport.cs`: VRChat components (ADR 0042): descriptor (playable layers, viewpoint, visemes, blink, eye
  look, expressions), PhysBones and colliders, contacts, VRChat constraints. Read by field name; no SDK reference.
- `Editor/Json.cs`: JSON text helpers and the X mirror for vectors, rotations and Euler angles.

Status: runs on Unity 6000.3.28f1 without the VRChat SDK and on 2022.3.22f1 with it (SDK 3.10.6). All humanoid
prefabs of the 11 local avatar packages export; meshes, triangles, shapes, joints and bind poses match the Blender
conversions in `corpus/local/glb/` (comparison on #36). With the SDK, VRChat components export too (ADR 0042); each
export logs what it skipped.

## Install

In a creator project's `Packages/manifest.json`:
`"civbb.unity-exporter": "https://github.com/yannikwirbs/civbb.git?path=tools/unity-exporter"`.

## Local runs on the Booth corpus (sessions)

Two gitignored host projects: `corpus/local/unity-export-vrc` (Unity 2022.3.22f1 with the VRChat SDK, created by
the owner with the Creator Companion; use this one) and `corpus/local/unity-export` (Unity 6, no SDK). Both list the
exporter as `"civbb.unity-exporter": "file:../../../../tools/unity-exporter"` in `Packages/manifest.json`. The SDK-less
one was created with:

```
"$(unity editors path 6000.3.28f1)/Editor/Unity.exe" -batchmode -nographics -quit -createProject D:/Documents/Repos/CIVBB/corpus/local/unity-export -logFile <log>
```

then add `"civbb.unity-exporter": "file:../../../../tools/unity-exporter"` to its `Packages/manifest.json`.
Import a package, list the humanoid prefabs, export one. `AssetDatabase.ImportPackage` is asynchronous and `unity
run` always adds `-quit`, so the import runs through the Editor binary without `-quit` and exits once every asset of
the package exists (ADR 0042; it survives the domain reload the SDK triggers):

```
"$(unity editors path 2022.3.22f1)/Editor/Unity.exe" -batchmode -nographics -projectPath D:/Documents/Repos/CIVBB/corpus/local/unity-export-vrc -logFile <log> -executeMethod Civbb.Exporter.Batch.Import -civbbPackage <abs path>.unitypackage
unity run corpus/local/unity-export-vrc --no-tail --log-file <log> -- -executeMethod Civbb.Exporter.Batch.Export
unity run corpus/local/unity-export-vrc --no-tail --log-file <log> -- -executeMethod Civbb.Exporter.Batch.Export -civbbAvatar Assets/<prefab> -civbbOut <abs path>.glb [-civbbLayers "Base=Assets/<controller>;Gesture=...;FX=..."]
```

Without `-civbbLayers`, the descriptor's playable layers fill the groups `Base`, `Additive`, `Gesture`, `Action`,
`FX` (ADR 0042). `-civbbLayers` overrides that with controllers named by asset path, in order (projects without the
SDK).

Each exits by itself (0, or 1 with `CIVBB failed:` in the log). Check the result with `civbb inspect <glb>`
next to the Blender conversion in `corpus/local/glb/`.

## Compile check without a licence

```
D="$(unity editors path 6000.3.28f1)/Editor/Data"
"$D/NetCoreRuntime/dotnet.exe" "$D/DotNetSdkRoslyn/csc.dll" -nologo -noconfig -nostdlib -target:library \
  -langversion:9 -out:<scratch>/exporter.dll $(for f in $D/Managed/UnityEngine/*.dll; do echo -r:$f; done) \
  -r:$D/NetStandard/ref/2.1.0/netstandard.dll tools/unity-exporter/Editor/*.cs
```
