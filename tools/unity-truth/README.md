# unity-truth

A minimal Unity project that records Unity's behaviour as JSON test data (ADR 0013). Not a crate.

- `Assets/Editor/Muscles.cs`: humanoid muscle space (ADR 0014). Writes `muscles.json` (table and clip property
  names) and three skeleton recordings (`tpose`, `rolled`, `twist`) to the directory given by `-civbbOut`.
- `Assets/Editor/Constraints.cs`: transform constraints (ADR 0018). Writes `constraints.json`: per case the input
  hierarchy, the constraints in `civbb-format`'s serde shape and Unity's result. Constraints only run in play mode,
  so run it **without** `-quit` (it exits by itself) and give `-civbbOut` as an absolute path (Unity resolves
  relative ones against the project).

`unity run` picks the editor pinned in `ProjectSettings/ProjectVersion.txt`. It always adds `-quit`, so
`Constraints.Record` needs the Editor binary itself:

```
unity run tools/unity-truth --no-tail --log-file <log> -- -executeMethod Muscles.Record -civbbOut <abs path>/crates/anim/tests/unity
"$(unity editors path 6000.3.28f1)/Editor/Unity.exe" -batchmode -nographics -projectPath tools/unity-truth   -executeMethod Constraints.Record -logFile <log> -civbbOut <abs path>/crates/anim/tests/unity
```

The first run imports the project (under a minute). The log ends with `CIVBB muscles recorded to ...`; Unity
licensing and network warnings in the log are harmless offline.
