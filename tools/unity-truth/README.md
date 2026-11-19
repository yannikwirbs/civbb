# unity-truth

A minimal Unity project that records Unity's behaviour as JSON test data (ADR 0013). Not a crate.

- `Assets/Editor/Muscles.cs`: humanoid muscle space (ADR 0014). Writes `muscles.json` (table and clip property
  names) and three skeleton recordings (`tpose`, `rolled`, `twist`) to the directory given by `-civbbOut`.
- `Assets/Editor/Constraints.cs`: transform constraints (ADR 0018). Writes `constraints.json`: per case the input
  hierarchy, the constraints in `civbb-format`'s serde shape and Unity's result. Constraints only run in play mode,
  so run it **without** `-quit` (it exits by itself) and give `-civbbOut` as an absolute path (Unity resolves
  relative ones against the project).
- `Assets/Editor/Exported.cs`: an exported avatar's ground truth (#36, ADR 0042): named clips sampled on the prefab
  and its Unity constraints' results under random source poses. The avatars are third-party, so copy the file into
  an exporter host project's `Assets/Editor` and write to `corpus/local/unity-truth/<name>.json`, next to
  `corpus/local/unity-glb/<name>.glb`; `crates/pipeline/tests/unity_export.rs` compares them. Run like
  `Constraints.Record` (play mode, no `-quit`): `-executeMethod Exported.Record -civbbAvatar Assets/<prefab>
  -civbbClips "<clip>;<clip>" -civbbOut <abs path>.json`.
- `Assets/Editor/VrcConstraints.cs`: the VRChat SDK's constraint runtime (#75, ADR 0046): generated cases with
  Solve In Local Space, Freeze To World and Rebake Offsets When Unfrozen, ten frames each. It needs the SDK, so
  copy it into `corpus/local/unity-export-vrc/Assets/Editor` and run it there; it reaches the SDK only by reflection
  and the cases hold no third-party data, so the output is committed:
  `tools/unity-batch.sh corpus/local/unity-export-vrc VrcConstraints.Record -civbbOut <abs path>/crates/anim/tests/unity/vrc_constraints.json`.
- `Assets/Editor/VrcContacts.cs`: the VRChat SDK's contact runtime (#75, ADR 0047): a sender moving through a
  receiver, one step a frame, for sphere, capsule and box shapes, the receiver types and face proximity. Run it like
  `VrcConstraints.cs`, with `-civbbOut <abs path>/crates/dynamics/tests/unity/vrc_contacts.json`.
- `Assets/Editor/VrcPhysBones.cs`: the VRChat SDK's PhysBone runtime (#75): a four-bone chain on a root moving
  along a scripted path (still, sway, turn, lift), 90 frames, versions 1.0 and 1.1, both integration types, then one
  parameter at a time (pull, gravity, falloff, immobile, limits, endpoint, colliders). Run it like
  `VrcConstraints.cs`, with `-civbbOut <abs path>/crates/dynamics/tests/unity/vrc_physbones.json`;
  `crates/dynamics/tests/vrc_physbone.rs` replays it.

`unity run` picks the editor pinned in `ProjectSettings/ProjectVersion.txt`. It always adds `-quit`, so
`Constraints.Record` needs the Editor binary itself:

```
unity run tools/unity-truth --no-tail --log-file <log> -- -executeMethod Muscles.Record -civbbOut <abs path>/crates/anim/tests/unity
"$(unity editors path 6000.3.28f1)/Editor/Unity.exe" -batchmode -nographics -projectPath tools/unity-truth   -executeMethod Constraints.Record -logFile <log> -civbbOut <abs path>/crates/anim/tests/unity
```

The first run imports the project (under a minute). The log ends with `CIVBB muscles recorded to ...`; Unity
licensing and network warnings in the log are harmless offline.
