# unity-truth

A minimal Unity project that records Unity's behaviour as JSON test data (ADR 0013). Not a crate.

- `Assets/Editor/Muscles.cs`: humanoid muscle space (ADR 0014). Writes `muscles.json` (table and clip property
  names) and three skeleton recordings (`tpose`, `rolled`, `twist`) to the directory given by `-civbbOut`.
- `Assets/Editor/Mirror.cs`: humanoid mirroring (#89, ADR 0060) on the generated skeleton of `Muscles.cs`: one clip
  played plain, by a mirrored state, a mirrored blend tree child and both. Run
  `tools/unity-batch.sh tools/unity-truth Mirror.Record -civbbOut <abs path>/crates/anim/tests/unity/mirror.json`.
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
- `Assets/Editor/BodyPose.cs`: Unity's humanoid body pose (#52) on `Muscles.cs`'s generated skeletons (plain and
  rolled axes): `HumanPoseHandler` poses with muscles, body position and rotation set, every node's world placement and
  `GetHumanPose` of the result. Runs in this project:
  `tools/unity-batch.sh tools/unity-truth BodyPose.Record -civbbOut <abs path>/crates/anim/tests/unity/body_pose.json`;
  `crates/anim/tests/body_pose.rs` replays it.
- `Assets/Editor/VrcPhysBones.cs`: the VRChat SDK's PhysBone runtime (#75, ADR 0049): a four-bone chain on a root
  moving along a scripted path (still, sway, turn, lift) for 1.5 s, versions 1.0 and 1.1, both integration types,
  one parameter at a time and sweeps (pull, spring, momentum, stiffness, gravity, falloff, immobile, limits,
  endpoint, colliders including inside capsules thinner and fatter than the bone, scale, scaled bones against unscaled
  colliders); `bone_scales` holds B0 to B4's local scales. Cases are built in play
  mode on the first tick, so their first step already runs at the pinned frame time. Run it like `VrcConstraints.cs`, once per frame time:
  `-civbbOut <abs path>/crates/dynamics/tests/unity/vrc_physbones.json` (60 Hz, every case), then with
  `-civbbDt 0.0111111114` into `vrc_physbones_90hz.json` and `-civbbDt 0.0333333351` into `vrc_physbones_30hz.json`
  (the cases in `RateCases`); `crates/dynamics/tests/vrc_physbone.rs` replays all three.

`unity run` picks the editor pinned in `ProjectSettings/ProjectVersion.txt`. It always adds `-quit`, so
`Constraints.Record` needs the Editor binary itself:

```
unity run tools/unity-truth --no-tail --log-file <log> -- -executeMethod Muscles.Record -civbbOut <abs path>/crates/anim/tests/unity
"$(unity editors path 6000.3.28f1)/Editor/Unity.exe" -batchmode -nographics -projectPath tools/unity-truth   -executeMethod Constraints.Record -logFile <log> -civbbOut <abs path>/crates/anim/tests/unity
```

The first run imports the project (under a minute). The log ends with `CIVBB muscles recorded to ...`; Unity
licensing and network warnings in the log are harmless offline.
