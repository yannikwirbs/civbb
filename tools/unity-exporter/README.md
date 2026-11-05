# unity-exporter

CIVBB's Unity editor package (`civbb.unity-exporter`, ADRs 0037 and 0038): exports an avatar prefab to the source
package of design doc §9, glTF plus `X_*` extensions. Not a crate (excluded from the workspace). Editor-only;
links no VRChat SDK code.

- `Editor/Batch.cs`: batch entry points `Civbb.Exporter.Batch.Import` and `Civbb.Exporter.Batch.Export`.
- `Editor/UnityPackage.cs`: lists a `.unitypackage`'s asset paths so the import step can refuse packages with code.
- `Editor/Glb.cs`: the GLB writer: nodes, meshes, skins, morph targets, materials, textures, `X_avatar.humanoid`.

Status: written and compile-checked against Unity 6000.3.27f1's assemblies; not yet run (the Unity licence lapsed,
#72). The `.meta` files a git install needs are added after the first run.

## Install

In a creator project's `Packages/manifest.json`:
`"civbb.unity-exporter": "https://github.com/yannikwirbs/civbb.git?path=tools/unity-exporter"`.

## Local runs on the Booth corpus (sessions)

Host project, gitignored, created once (paths from CLAUDE.md, Environment):

```
Unity.exe -batchmode -nographics -quit -createProject D:/Documents/Repos/CIVBB/corpus/local/unity-export -logFile <log>
```

then add `"civbb.unity-exporter": "file:../../../../tools/unity-exporter"` to its `Packages/manifest.json`.
Import a package, list the humanoid prefabs, export one:

```
Unity.exe -batchmode -nographics -projectPath corpus/local/unity-export -executeMethod Civbb.Exporter.Batch.Import -logFile <log> -civbbPackage <abs path>.unitypackage
Unity.exe -batchmode -nographics -projectPath corpus/local/unity-export -executeMethod Civbb.Exporter.Batch.Export -logFile <log>
Unity.exe -batchmode -nographics -projectPath corpus/local/unity-export -executeMethod Civbb.Exporter.Batch.Export -logFile <log> -civbbAvatar Assets/<prefab> -civbbOut <abs path>.glb
```

Each exits by itself (0, or 1 with `CIVBB failed:` in the log). Check the result with `civbb inspect <glb>`
next to the Blender conversion in `corpus/local/glb/`.

## Compile check without a licence

```
D=G:/Unity6/Editor/Data
"$D/NetCoreRuntime/dotnet.exe" "$D/DotNetSdkRoslyn/csc.dll" -nologo -noconfig -nostdlib -target:library \
  -langversion:9 -out:<scratch>/exporter.dll $(for f in $D/Managed/UnityEngine/*.dll; do echo -r:$f; done) \
  -r:$D/NetStandard/ref/2.1.0/netstandard.dll tools/unity-exporter/Editor/*.cs
```
