# unity-exporter

CIVBB's Unity editor package (`civbb.unity-exporter`, ADRs 0037 and 0038): exports an avatar prefab to the source
package of design doc §9, glTF plus `X_*` extensions. Not a crate (excluded from the workspace). Editor-only;
links no VRChat SDK code.

- `Editor/Batch.cs`: batch entry points `Civbb.Exporter.Batch.Import` and `Civbb.Exporter.Batch.Export`.
- `Editor/UnityPackage.cs`: lists a `.unitypackage`'s asset paths so the import step can refuse packages with code.
- `Editor/Glb.cs`: the GLB writer: nodes, meshes, skins, morph targets, materials, textures, `X_avatar.humanoid`.
- `Editor/AnimatorExport.cs`: animator controllers and clips as `X_animator` (ADR 0041; curves per ADR 0039).
- `Editor/ConstraintExport.cs`: Unity constraint components as `X_constraints`.
- `Editor/Json.cs`: JSON text helpers and the X mirror for vectors, rotations and Euler angles.

Status: runs on Unity 6000.3.28f1. All 13 humanoid prefabs of the five Booth packages export; meshes, triangles,
shapes, joints and bind poses match the Blender conversions in `corpus/local/glb/` (comparison on #36). Animators
and Unity constraints export and import; VRChat components (descriptor, PhysBones, contacts, VRChat constraints,
state behaviours) need the VRChat SDK in the host project (#37).

## Install

In a creator project's `Packages/manifest.json`:
`"civbb.unity-exporter": "https://github.com/yannikwirbs/civbb.git?path=tools/unity-exporter"`.

## Local runs on the Booth corpus (sessions)

Host project, gitignored, created once (`unity editors path <version>` gives the Editor folder):

```
"$(unity editors path 6000.3.28f1)/Editor/Unity.exe" -batchmode -nographics -quit -createProject D:/Documents/Repos/CIVBB/corpus/local/unity-export -logFile <log>
```

then add `"civbb.unity-exporter": "file:../../../../tools/unity-exporter"` to its `Packages/manifest.json`.
Import a package, list the humanoid prefabs, export one. `AssetDatabase.ImportPackage` is asynchronous and `unity
run` always adds `-quit`, so the import runs through the Editor binary without `-quit` and exits from its completion
callback:

```
"$(unity editors path 6000.3.28f1)/Editor/Unity.exe" -batchmode -nographics -projectPath D:/Documents/Repos/CIVBB/corpus/local/unity-export -logFile <log> -executeMethod Civbb.Exporter.Batch.Import -civbbPackage <abs path>.unitypackage
unity run corpus/local/unity-export --no-tail --log-file <log> -- -executeMethod Civbb.Exporter.Batch.Export
unity run corpus/local/unity-export --no-tail --log-file <log> -- -executeMethod Civbb.Exporter.Batch.Export -civbbAvatar Assets/<prefab> -civbbOut <abs path>.glb [-civbbLayers "Base=Assets/<controller>;Gesture=...;FX=..."]
```

`-civbbLayers` names the controllers that fill the animator's layer groups, in order. Until the exporter reads the
VRChat avatar descriptor (#37), sessions pick them by file name.

Each exits by itself (0, or 1 with `CIVBB failed:` in the log). Check the result with `civbb inspect <glb>`
next to the Blender conversion in `corpus/local/glb/`.

## Compile check without a licence

```
D="$(unity editors path 6000.3.28f1)/Editor/Data"
"$D/NetCoreRuntime/dotnet.exe" "$D/DotNetSdkRoslyn/csc.dll" -nologo -noconfig -nostdlib -target:library \
  -langversion:9 -out:<scratch>/exporter.dll $(for f in $D/Managed/UnityEngine/*.dll; do echo -r:$f; done) \
  -r:$D/NetStandard/ref/2.1.0/netstandard.dll tools/unity-exporter/Editor/*.cs
```
