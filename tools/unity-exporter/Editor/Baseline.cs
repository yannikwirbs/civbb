// Unity CPU baselines (design doc §21, #38, ADR 0081): N copies of an avatar prefab stepped in play mode as our bench
// steps them (same grid, sway, parameter flips and frame time), the playable layers set up as `Reference.Render`
// sets them, PhysBones, contacts and constraints running in the SDK's runtime. Job workers off and the process
// confined to one logical CPU, so a frame's wall time is all the CPU work Unity did for it on one core, as our
// one-worker `measure`. Runs without -quit; exits itself.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Reflection;
using Unity.Profiling;
using Unity.Profiling.LowLevel.Unsafe;
using UnityEditor;
using UnityEditor.Animations;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Animations;
using UnityEngine.Playables;
using Debug = UnityEngine.Debug;
using Object = UnityEngine.Object;

namespace Civbb.Exporter
{
    public static class Baseline
    {
        /// Our bench's frame time (`civbb_client::bench::DT`) and parameter flip period (`FLIP_EVERY`).
        const float Dt = 1f / 90f;
        const int FlipEvery = 30;
        /// Markers recorded per frame. `PlayerLoop` is the frame's whole player-loop work; the animator is the three
        /// director markers (jobs run inline, no workers); the SDK's PhysBones, contacts and VRChat constraints run inside
        /// the three `VRCAvatarDynamics` markers, Unity constraints in `ConstraintManagerUpdate`.
        static readonly string[] Markers =
        {
            "PlayerLoop", "Update.DirectorUpdate", "PreLateUpdate.DirectorUpdateAnimationBegin",
            "PreLateUpdate.DirectorUpdateAnimationEnd", "VRCAvatarDynamicsPreSchedule", "VRCAvatarDynamicsPostSchedule",
            "VRCAvatarDynamicsComplete", "PreLateUpdate.ConstraintManagerUpdate",
        };

        /// One copy: its root, home position and playable layers.
        sealed class Copy
        {
            public GameObject root;
            public Vector3 home;
            public PlayableGraph graph;
            public readonly List<AnimatorControllerPlayable> controllers = new List<AnimatorControllerPlayable>();
            public readonly List<string> groups = new List<string>();
        }

        static GameObject prefab;
        static string outPath;
        static int[] counts;
        static int frames, warmup, phase = -1, frame;
        static readonly List<Copy> copies = new List<Copy>();
        static readonly Dictionary<Animator, Copy> byAnimator = new Dictionary<Animator, Copy>();
        static List<string> parameters = new List<string>();
        static readonly List<(string name, ProfilerRecorder r)> recorders = new List<(string, ProfilerRecorder)>();
        static readonly List<double> wall = new List<double>();
        static readonly Stopwatch clock = new Stopwatch();
        static readonly List<string> results = new List<string>();
        static string hooked = "", burst = "", optimized = "";
        /// After optimising: the groups that had the creator's own controller before (null: every group).
        static HashSet<string> customGroups;
        static GameObject emulator;

        /// `-civbbAvatar <prefab asset path> -civbbOut <absolute JSON path> -civbbCpu <logical CPU>
        /// [-civbbCopies 0,1,40] [-civbbFrames 300] [-civbbWarmup 60] [-civbbOptimize] [-civbbTop N]`: writes per copy count the mean milliseconds per
        /// frame of each marker and of the editor's whole frame.
        public static void Measure()
        {
            try
            {
                prefab = AssetDatabase.LoadAssetAtPath<GameObject>(Arg("-civbbAvatar") ?? throw new ArgumentException("missing -civbbAvatar"))
                    ?? throw new ArgumentException("no prefab at -civbbAvatar");
                outPath = Arg("-civbbOut") ?? throw new ArgumentException("missing -civbbOut");
                int cpu = int.Parse(Arg("-civbbCpu") ?? throw new ArgumentException("missing -civbbCpu"));
                counts = (Arg("-civbbCopies") ?? "0,1,40").Split(',').Select(int.Parse).ToArray();
                frames = int.Parse(Arg("-civbbFrames") ?? "300");
                warmup = int.Parse(Arg("-civbbWarmup") ?? "60");
                burst = BurstSynchronous();
                // Every job runs on the main thread, and every thread of the process on one core.
                if (cpu >= 0)
                {
                    Unity.Jobs.LowLevel.Unsafe.JobsUtility.JobWorkerCount = 0;
                    Process.GetCurrentProcess().ProcessorAffinity = new IntPtr(1L << cpu);
                }
                hooked = string.Join(", ", new[]
                {
                    ("VRC_AvatarParameterDriver", "OnApplySettings", nameof(Drive)),
                    ("VRC_PlayableLayerControl", "Initialize", nameof(InitLayerControl)),
                    ("VRC_AnimatorLayerControl", "Initialize", nameof(InitLayerControl)),
                }.Select(h => $"{h.Item1} {(Hook(h.Item1, h.Item2, h.Item3) ? "hooked" : "missing")}"));
                EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
                if (Environment.GetCommandLineArgs().Contains("-civbbOptimize"))
                {
                    prefab = Optimize(prefab);
                }
                EditorSettings.enterPlayModeOptionsEnabled = true;
                EditorSettings.enterPlayModeOptions = EnterPlayModeOptions.DisableDomainReload | EnterPlayModeOptions.DisableSceneReload;
                EditorApplication.update += Tick;
                EditorApplication.EnterPlaymode();
            }
            catch (Exception e)
            {
                Debug.LogError($"CIVBB failed: {e}");
                EditorApplication.Exit(1);
            }
        }

        static void Tick()
        {
            if (!EditorApplication.isPlaying)
            {
                return;
            }
            bool done = false;
            try
            {
                Time.captureDeltaTime = Dt;
                if (phase < 0 || frame == warmup + frames)
                {
                    if (phase >= 0)
                    {
                        Finish();
                    }
                    phase++;
                    done = phase == counts.Length;
                    if (!done)
                    {
                        Spawn(counts[phase]);
                        frame = 0;
                    }
                }
                else if (frame == warmup)
                {
                    var names = Markers.ToList();
                    if (Arg("-civbbTop") != null)
                    {
                        // Diagnostic: every time marker, to find where a frame's time goes.
                        var all = new List<ProfilerRecorderHandle>();
                        ProfilerRecorderHandle.GetAvailable(all);
                        names = all.Select(ProfilerRecorderHandle.GetDescription)
                            .Where(d => d.UnitType == ProfilerMarkerDataUnit.TimeNanoseconds).Select(d => d.Name).Distinct().ToList();
                    }
                    foreach (var m in names)
                    {
                        var r = new ProfilerRecorder(m, frames, ProfilerRecorderOptions.Default);
                        r.Start();
                        recorders.Add((m, r));
                    }
                    wall.Clear();
                    clock.Restart();
                }
                else if (frame > warmup)
                {
                    wall.Add(clock.Elapsed.TotalMilliseconds);
                    clock.Restart();
                }
                Step(frame);
                frame++;
            }
            catch (Exception e)
            {
                Debug.LogError($"CIVBB failed: {e}");
                done = true;
            }
            if (done)
            {
                EditorApplication.update -= Tick;
                Clear();
                bool ok = phase == counts.Length;
                if (ok)
                {
                    var handles = new List<ProfilerRecorderHandle>();
                    ProfilerRecorderHandle.GetAvailable(handles);
                    Debug.Log("CIVBB markers: " + string.Join(", ", handles.Select(h => ProfilerRecorderHandle.GetDescription(h).Name)
                        .Where(n => System.Text.RegularExpressions.Regex.IsMatch(n, "PhysBone|Contact|VRC|Constraint|Animat|Director", System.Text.RegularExpressions.RegexOptions.IgnoreCase))
                        .Distinct().OrderBy(n => n)));
                    File.WriteAllText(outPath, $"{{\"avatar\": {Json.Str(prefab.name)}, \"frames\": {frames}, \"warmup\": {warmup}, " +
                        $"\"dt\": {Dt.ToString("R", System.Globalization.CultureInfo.InvariantCulture)}, \"parameters\": {parameters.Count}, " +
                        $"\"hooks\": {Json.Str(hooked)}, \"burst\": {Json.Str(burst)}, \"optimized\": {Json.Str(optimized)}, \"runs\": [{string.Join(", ", results)}]}}\n");
                    Debug.Log($"CIVBB measured {prefab.name} at {string.Join(", ", counts)} copies to {outPath}");
                }
                EditorApplication.Exit(ok ? 0 : 1);
            }
        }

        /// The bench's per-frame inputs: each copy sways sideways, and every `FlipEvery` frames one parameter flips.
        static void Step(int f)
        {
            float t = f * Dt;
            for (int k = 0; k < copies.Count; k++)
            {
                var c = copies[k];
                c.root.transform.position = c.home + new Vector3(0.1f * Mathf.Sin(t * 3 + k), 0, 0);
                if (parameters.Count > 0 && f % FlipEvery == 0)
                {
                    var name = parameters[f / FlipEvery % parameters.Count];
                    SetParameter(c, name, 1 - GetParameter(c, name));
                }
            }
        }

        static void Finish()
        {
            var parts = new List<string> { $"\"copies\": {counts[phase]}" };
            var ms = new List<string>();
            var top = new List<(string name, double ms)>();
            foreach (var (name, r) in recorders)
            {
                // Nanoseconds per frame, summed over threads; the oldest samples drop out of the buffer first.
                double sum = 0;
                for (int i = 0; i < r.Count; i++)
                {
                    sum += r.GetSample(i).Value;
                }
                if (Markers.Contains(name))
                {
                    ms.Add($"{Json.Str(name)}: {Num(r.Count > 0 ? sum / r.Count / 1e6 : -1)}");
                }
                top.Add((name, r.Count > 0 ? sum / r.Count / 1e6 : 0));
                r.Dispose();
            }
            if (Arg("-civbbTop") != null)
            {
                Debug.Log($"CIVBB top at {counts[phase]} copies: " + string.Join(", ", top.OrderByDescending(t => t.ms)
                    .Take(int.Parse(Arg("-civbbTop"))).Select(t => $"{t.name} {Num(t.ms)}")));
            }
            recorders.Clear();
            parts.Add($"\"marker_ms\": {{{string.Join(", ", ms)}}}");
            parts.Add($"\"frame_ms\": {Num(wall.Count > 0 ? wall.Average() : -1)}");
            results.Add($"{{{string.Join(", ", parts)}}}");
            Debug.Log($"CIVBB {counts[phase]} copies: {string.Join(", ", ms)}, frame {Num(wall.Average())} ms");
        }

        static string Num(double v) => v.ToString("0.######", System.Globalization.CultureInfo.InvariantCulture);

        static void Clear()
        {
            foreach (var c in copies)
            {
                if (c.graph.IsValid())
                {
                    c.graph.Destroy();
                }
                Object.Destroy(c.root);
            }
            copies.Clear();
            byAnimator.Clear();
            if (emulator != null)
            {
                Object.Destroy(emulator);
            }
        }

        /// `n` copies on a square grid 1 m apart, as the bench's `scene`, each with its playable layers.
        static void Spawn(int n)
        {
            Clear();
            int side = Mathf.Max(1, Mathf.CeilToInt(Mathf.Sqrt(n)));
            for (int id = 0; id < n; id++)
            {
                var root = Object.Instantiate(prefab);
                root.SetActive(true);
                var c = new Copy { root = root, home = new Vector3(id % side, 0, id / side) };
                root.transform.SetPositionAndRotation(c.home, Quaternion.identity);
                root.transform.localScale = Vector3.one;
                if (!Environment.GetCommandLineArgs().Contains("-civbbEmulator"))
                {
                    Layers(c);
                }
                copies.Add(c);
            }
            if (Environment.GetCommandLineArgs().Contains("-civbbEmulator"))
            {
                Emulate();
            }
            if (copies.Count > 0)
            {
                parameters = copies[0].controllers
                    .SelectMany(p => Enumerable.Range(0, p.GetParameterCount()).Select(i => p.GetParameter(i).name))
                    .Distinct().ToList();
            }
        }

        /// The descriptor's playable layers as `Reference.Begin` sets them up: VRChat's group order, Additive
        /// additive, Action at weight 0, descriptor masks applied, the SDK's proxy clips played empty.
        static void Layers(Copy c)
        {
            var animator = c.root.GetComponent<Animator>();
            var vrc = new VrcExport(c.root.transform, t => 0);
            var layers = vrc.Layers().Where(l => l.path != null && (customGroups == null || customGroups.Contains(l.group))).ToList();
            if (animator == null || layers.Count == 0)
            {
                return;
            }
            var assets = layers.Select(l => (AnimatorController)vrc.Controllers[l.group]).ToList();
            bool human = assets.Any(a => a.animationClips.Any(clip => clip != null && clip.humanMotion && !AnimatorExport.SdkAsset(clip) && !clip.name.StartsWith("proxy_")));
            animator.runtimeAnimatorController = null;
            if (!human)
            {
                animator.avatar = null;
            }
            animator.applyRootMotion = false;
            animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
            byAnimator[animator] = c;
            c.graph = PlayableGraph.Create("civbb baseline");
            c.graph.SetTimeUpdateMode(DirectorUpdateMode.GameTime);
            var mixer = AnimationLayerMixerPlayable.Create(c.graph, layers.Count);
            for (int i = 0; i < layers.Count; i++)
            {
                var playable = AnimatorControllerPlayable.Create(c.graph, WithoutProxies(assets[i]));
                c.graph.Connect(playable, 0, mixer, i);
                mixer.SetInputWeight(i, layers[i].group == "Action" ? 0 : 1);
                mixer.SetLayerAdditive((uint)i, layers[i].group == "Additive");
                if (vrc.Masks.TryGetValue(layers[i].group, out var mask) && mask != null)
                {
                    mixer.SetLayerMaskFromAvatarMask((uint)i, mask);
                }
                c.controllers.Add(playable);
                c.groups.Add(layers[i].group);
            }
            AnimationPlayableOutput.Create(c.graph, "civbb", animator).SetSourcePlayable(mixer);
            c.graph.Play();
        }

        static readonly Dictionary<AnimatorController, RuntimeAnimatorController> overrides =
            new Dictionary<AnimatorController, RuntimeAnimatorController>();

        static RuntimeAnimatorController WithoutProxies(AnimatorController c)
        {
            if (!overrides.TryGetValue(c, out var r))
            {
                // By name too: an optimiser's copies of the SDK's proxy clips have no package path.
                var sdk = c.animationClips.Where(a => a != null && (AnimatorExport.SdkAsset(a) || a.name.StartsWith("proxy_"))).Distinct().ToList();
                var empty = new AnimationClip { name = "civbb empty" };
                var o = new AnimatorOverrideController(c);
                o.ApplyOverrides(sdk.Select(a => new KeyValuePair<AnimationClip, AnimationClip>(a, empty)).ToList());
                r = overrides[c] = sdk.Count == 0 ? (RuntimeAnimatorController)c : o;
            }
            return r;
        }

        /// As `Reference.Hook`: points the SDK's static client delegate at one of ours.
        static bool Hook(string type, string field, string method)
        {
            var f = AppDomain.CurrentDomain.GetAssemblies()
                .Select(a => a.GetType("VRC.SDKBase." + type)).FirstOrDefault(t => t != null)
                ?.GetField(field, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Static);
            f?.SetValue(null, Delegate.CreateDelegate(f.FieldType, typeof(Baseline).GetMethod(method, BindingFlags.NonPublic | BindingFlags.Static)));
            return f != null;
        }

        static object Field(object o, string name) => o.GetType().GetField(name).GetValue(o);

        static void InitLayerControl(object control)
        {
            var f = control.GetType().GetField("ApplySettings", BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
            var method = control.GetType().Name.Contains("Playable") ? nameof(PlayableLayer) : nameof(AnimatorLayer);
            f?.SetValue(control, Delegate.CreateDelegate(f.FieldType, typeof(Baseline).GetMethod(method, BindingFlags.NonPublic | BindingFlags.Static)));
        }

        /// Layer controls set their goal weight at once (the cost is the same; `Reference` blends for the image).
        static void PlayableLayer(object control, Animator animator)
        {
            if (!byAnimator.TryGetValue(animator, out var c))
            {
                return;
            }
            var group = Field(control, "layer").ToString();
            var mixer = c.graph.GetOutput(0).GetSourcePlayable();
            for (int i = 0; i < c.groups.Count; i++)
            {
                if (c.groups[i] == group)
                {
                    mixer.SetInputWeight(i, (float)Field(control, "goalWeight"));
                }
            }
        }

        static void AnimatorLayer(object control, Animator animator)
        {
            if (!byAnimator.TryGetValue(animator, out var c))
            {
                return;
            }
            var group = Field(control, "playable").ToString();
            int layer = (int)Field(control, "layer");
            for (int i = 0; i < c.groups.Count; i++)
            {
                if (c.groups[i] == group && layer < c.controllers[i].GetLayerCount())
                {
                    c.controllers[i].SetLayerWeight(layer, (float)Field(control, "goalWeight"));
                }
            }
        }

        /// As `Reference.Drive`, per copy: Set, Add, Random and Copy (with range conversion).
        static void Drive(object driver, Animator animator)
        {
            if (!byAnimator.TryGetValue(animator, out var c))
            {
                return;
            }
            foreach (var p in (System.Collections.IEnumerable)Field(driver, "parameters"))
            {
                var name = (string)Field(p, "name");
                float value = (float)Field(p, "value");
                switch (Field(p, "type").ToString())
                {
                    case "Set": SetParameter(c, name, value); break;
                    case "Add": SetParameter(c, name, GetParameter(c, name) + value); break;
                    case "Random":
                        SetParameter(c, name, UnityEngine.Random.Range((float)Field(p, "valueMin"), (float)Field(p, "valueMax")));
                        break;
                    case "Copy":
                        float v = GetParameter(c, (string)Field(p, "source"));
                        if ((bool)Field(p, "convertRange"))
                        {
                            float s0 = (float)Field(p, "sourceMin"), s1 = (float)Field(p, "sourceMax");
                            float d0 = (float)Field(p, "destMin"), d1 = (float)Field(p, "destMax");
                            v = s1 != s0 ? d0 + (v - s0) / (s1 - s0) * (d1 - d0) : d0;
                        }
                        SetParameter(c, name, v);
                        break;
                }
            }
        }

        static float GetParameter(Copy c, string name)
        {
            foreach (var p in c.controllers)
            {
                for (int i = 0; i < p.GetParameterCount(); i++)
                {
                    var param = p.GetParameter(i);
                    if (param.name == name)
                    {
                        return param.type == AnimatorControllerParameterType.Float ? p.GetFloat(name)
                            : param.type == AnimatorControllerParameterType.Int ? p.GetInteger(name)
                            : p.GetBool(name) ? 1 : 0;
                    }
                }
            }
            return 0;
        }

        /// Sets a parameter on every layer's controller that has it, as VRChat shares parameters across layers.
        static void SetParameter(Copy c, string name, float value)
        {
            foreach (var p in c.controllers)
            {
                for (int i = 0; i < p.GetParameterCount(); i++)
                {
                    var param = p.GetParameter(i);
                    if (param.name != name)
                    {
                        continue;
                    }
                    switch (param.type)
                    {
                        case AnimatorControllerParameterType.Float: p.SetFloat(name, value); break;
                        case AnimatorControllerParameterType.Int: p.SetInteger(name, Mathf.RoundToInt(value)); break;
                        case AnimatorControllerParameterType.Bool: p.SetBool(name, value != 0); break;
                        default:
                            if (value != 0) p.SetTrigger(name);
                            else p.ResetTrigger(name);
                            break;
                    }
                }
            }
        }

        /// AvatarOptimizer's Trace and Optimize on an instance of the prefab, run through NDMF in edit mode (by
        /// reflection: the exporter references neither). The processed instance, inactive, is the template every copy
        /// is instantiated from; NDMF's own apply-on-play is switched off so copies are not processed again.
        static GameObject Optimize(GameObject source)
        {
            var trace = Find("Anatawa12.AvatarOptimizer.TraceAndOptimize") ?? throw new InvalidOperationException("AvatarOptimizer is not installed");
            var processor = Find("nadena.dev.ndmf.AvatarProcessor") ?? throw new InvalidOperationException("NDMF is not installed");
            Find("nadena.dev.ndmf.config.Config")?.GetProperty("ApplyOnPlay")?.SetValue(null, false);
            var o = Object.Instantiate(source);
            o.name = source.name;
            string before = Stats(o);
            // NDMF fills the default layers with copies of the SDK's controllers; they stay unplayed, as before (ADR 0042).
            customGroups = new HashSet<string>(new VrcExport(o.transform, t => 0).Layers().Where(l => l.path != null).Select(l => l.group));
            o.AddComponent(trace);
            processor.GetMethod("ProcessAvatar", new[] { typeof(GameObject) }).Invoke(null, new object[] { o });
            optimized = $"{before} -> {Stats(o)}";
            o.SetActive(false);
            return o;
        }

        /// #38's check of the setup (`-civbbEmulator`): Lyuma's Av3Emulator runs the copies' playable layers instead
        /// of `Layers`, without its mirror, shadow and remote clones or preprocess hooks (which would add avatars or
        /// run NDMF). Parameters then follow the emulator, not the bench's flips.
        static void Emulate()
        {
            var type = Find("Lyuma.Av3Emulator.Runtime.LyumaAv3Emulator") ?? throw new InvalidOperationException("Av3Emulator is not installed");
            Find("nadena.dev.ndmf.config.Config")?.GetProperty("ApplyOnPlay")?.SetValue(null, false);
            emulator = new GameObject("civbb av3emulator");
            emulator.SetActive(false);
            var e = emulator.AddComponent(type);
            foreach (var (field, value) in new[] { ("RunPreprocessAvatarHook", false), ("CreateNonLocalClone", false), ("DisableMirrorClone", true), ("DisableShadowClone", true), ("EnableHeadScaling", false) })
            {
                type.GetField(field).SetValue(e, value);
            }
            emulator.SetActive(true);
        }

        static Type Find(string name) =>
            AppDomain.CurrentDomain.GetAssemblies().Select(a => a.GetType(name)).FirstOrDefault(t => t != null);

        /// What an optimiser changes that costs CPU: PhysBone components and their transforms' share of the
        /// hierarchy, skinned meshes and their shapes, and animator layers.
        static string Stats(GameObject o)
        {
            var vrc = new VrcExport(o.transform, t => 0);
            vrc.Layers();
            var skinned = o.GetComponentsInChildren<SkinnedMeshRenderer>(true);
            return $"physbones {o.GetComponentsInChildren<Component>(true).Count(c => c != null && c.GetType().Name == "VRCPhysBone")}, " +
                   $"transforms {o.GetComponentsInChildren<Transform>(true).Length}, skinned meshes {skinned.Length}, " +
                   $"shapes {skinned.Sum(s => s.sharedMesh != null ? s.sharedMesh.blendShapeCount : 0)}, " +
                   $"animator layers {vrc.Controllers.Where(kv => customGroups == null || customGroups.Contains(kv.Key)).Select(kv => kv.Value).OfType<AnimatorController>().Sum(c => c.layers.Length)}";
        }

        /// Burst-compiled jobs (the SDK's PhysBones and contacts) compile in the background in the Editor and run as
        /// managed code until then, about 25x slower; the VRChat client runs them compiled. Makes the first schedule
        /// compile synchronously. By reflection, so projects without Burst still load the exporter.
        static string BurstSynchronous()
        {
            var compiler = AppDomain.CurrentDomain.GetAssemblies().Select(a => a.GetType("Unity.Burst.BurstCompiler")).FirstOrDefault(t => t != null);
            if (compiler == null)
            {
                return "no Burst";
            }
            bool enabled = (bool)compiler.GetProperty("IsEnabled").GetValue(null);
            var options = compiler.GetField("Options").GetValue(null);
            options.GetType().GetProperty("EnableBurstCompileSynchronously").SetValue(options, true);
            bool jobs = Unity.Jobs.LowLevel.Unsafe.JobsUtility.JobCompilerEnabled;
            Unity.Jobs.LowLevel.Unsafe.JobsUtility.JobCompilerEnabled = true;
            return (enabled ? "enabled, synchronous" : "disabled") + $", job compiler was {jobs}";
        }

        static string Arg(string name)
        {
            var args = Environment.GetCommandLineArgs();
            int i = Array.IndexOf(args, name);
            return i >= 0 && i + 1 < args.Length ? args[i + 1] : null;
        }
    }
}
