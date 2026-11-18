# Lessons: Unity

Read before working on the Unity Editor, the exporter (`tools/unity-exporter`) or ground truth (`tools/unity-truth`). Add a line whenever something costs more than one attempt.

- Unity animator semantics: the manual's Transitions page and `AnimatorStateTransition.exitTime` reference are the authoritative sources (any-state first, ordered interruption, exit time); Write Defaults is only documented by the community (VRC School, VRCFury `technical/wd`). Johansen's thesis PDF (runevision.com/thesis) gives the 2D blend formulas; extract it with `pdftotext` (in Git Bash).
- Unity is installed outside the default paths: Editors under `G:/Unity/editor/<version>/` (they move when the owner updates: `G:/Unity6` was gone on 2026-11-06). Ask the CLI: `unity editors --format tsv` lists versions and paths, `unity editors path <version>` prints one. The Unity CLI (`C:/Users/xxsay/AppData/Local/Unity/bin/unity.exe`) is on the user PATH but not in sessions started before it was installed: call it by full path. Licence check: `unity license --json` (there is no `license status`/`list`). Don't conclude "not installed" from `C:/Program Files` alone.
- Unity batch runs take about 20 s: `unity run <project> --no-tail --log-file <log> -- -executeMethod Class.Method <own args>` (Editor from `ProjectVersion.txt`; upgrade a project with `unity run <project> --editor-version <new>`). `unity run` always adds `-quit`, so a method that needs play mode (`Constraints.Record`) quits before it runs and still exits 0: call `"$(unity editors path <v>)/Editor/Unity.exe" -batchmode -nographics -projectPath ...` without `-quit` for those. `unity run` resolves relative paths against the project: pass absolute ones; read own args with `Environment.GetCommandLineArgs()`. Licensing and `config.uca.cloud.unity3d.com` errors in the log are harmless. `Avatar` internals (`GetPreRotation`, `GetPostRotation`, `GetLimitSign`, `GetZYRoll`) are reachable by reflection; `AnimationUtility.GetAnimatableBindings` lists a humanoid's clip property names.
- Unity constraints only evaluate in play mode. Batch mode can enter it: set `EditorSettings.enterPlayModeOptions` to disable domain and scene reload, `EditorApplication.EnterPlaymode()`, tick in `EditorApplication.update`, exit with `EditorApplication.Exit(0)` (no `-quit`). Pass output dirs as absolute paths (relative ones resolve against the project), and wrap the work in try/finally around `Exit`: an exception in an update callback leaves the editor running until the timeout.
- Unity batch runs exiting 198 with "No valid Unity Editor license found" mean the licence lapsed, not a project
  problem: check `unity license status --format json` (`LICENSE_NOT_ACTIVE`) before debugging. Reactivating needs
  `--accept-eula`, which is the owner's call. C# still compiles without a licence: Unity's bundled Roslyn
  (`Editor/Data/DotNetSdkRoslyn/csc.dll` run by `Editor/Data/NetCoreRuntime/dotnet.exe`) against
  `Editor/Data/Managed/UnityEngine/*.dll` and `NetStandard/ref/2.1.0/netstandard.dll`.
- Unity YAML assets may start with a UTF-8 BOM before `%YAML` (44 of HAOLAN Quest's); classify by
  `removeprefix(BOM)`, or they look binary-serialised.
- `AssetDatabase.ImportPackage` is asynchronous: a batch method that calls it and exits (or any `unity run`, which
  adds `-quit`) imports nothing, though its own log line says it did. Exit from `importPackageCompleted`/`Failed`
  and run the Editor binary without `-quit`; check `Assets/` afterwards, not the log.
- VRChat avatars' root `Animator` has no controller: the playable-layer controllers hang off the avatar descriptor,
  a missing script without the VRChat SDK. Find controllers with `AssetDatabase.FindAssets("t:AnimatorController")`.
- Scratch editor scripts for probing (`corpus/local/unity-export/Assets/Editor/Probe.cs`) are fine: the host project
  is gitignored; they never go into `tools/unity-exporter`.
- In the VRChat SDK host project, `AssetDatabase.ImportPackage` is followed by a domain reload (the SDK changes
  settings and recompiles), which drops static callbacks: the Editor then idles until its timeout. `Batch.Import`
  polls for the package's assets and resumes from `SessionState` (ADR 0042).
- Unity batch recordings of a prefab: animators with `CullUpdateTransforms` (most VRChat avatars) evaluate nothing
  without a camera, so a sampled clip silently equals the rest pose. Set `cullingMode = AlwaysAnimate`.
- `Object.Instantiate` resets Unity constraints' at-rest values (position, rotation, scale, parent, aim; not look-at)
  to the target's local transform; `LoadPrefabContents` and `PrefabUtility.InstantiatePrefab` keep the serialized
  ones. A runtime loads avatars by instantiating, so the exporter writes the local transform (ADR 0042).
- Read VRChat enums from `SerializedProperty.enumNames[enumValueIndex]`: their underlying values are not
  sequential (menu control types are 101 to 203).
- A material whose shader is missing shows `Hidden/InternalErrorShader` but keeps its saved properties, and its
  `.mat` file still names the shader GUID (`m_Shader: {fileID, guid}`); lilToon's variant GUIDs are in its
  `.shader.meta` files on GitHub (ADR 0043). Saved properties include leftovers of earlier shaders: fingerprint by
  GUID or name before marker properties.
- Grepping Unity logs for `CIVBB` matches every path under the repo; grep `CIVBB (exported|failed|imported)`.
