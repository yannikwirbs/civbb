// Reference renders for parity (design doc §10, ADR 0048): renders an avatar prefab with the camera rig, light and
// states of a `civbb-client rig` hand-off, one PNG per state and view, in play mode so PhysBones, contacts and
// constraints run. Needs a graphics device: run the Editor without -nographics and without -quit; it exits itself.
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;
using UnityEditor;
using UnityEditor.Animations;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Animations;
using UnityEngine.Playables;
using UnityEngine.Rendering;
using Object = UnityEngine.Object;

namespace Civbb.Exporter
{
    public static class Reference
    {
#pragma warning disable 0649 // Filled by JsonUtility.
        [Serializable]
        class Handoff
        {
            public Rig rig;
            public float[] light_towards, light_color, ambient;
            public float dt;
            public int frames_per_state;
            public Param[] parameters;
            public State[] states;
        }

        [Serializable]
        class Rig
        {
            public int width, height;
            public float fov_y;
            public float[] background;
            public View[] views;
        }

        [Serializable]
        class View
        {
            public string name;
            public float[] eye, target;
        }

        [Serializable]
        class Param
        {
            public string name;
            public float value;
        }

        [Serializable]
        class State
        {
            public string name, parameter;
            public float value;
        }
#pragma warning restore 0649

        static Handoff handoff;
        static string outDir;
        static GameObject avatar;
        static Camera camera;
        static RenderTexture target;
        static Texture2D readback;
        static PlayableGraph graph;
        static readonly List<AnimatorControllerPlayable> controllers = new List<AnimatorControllerPlayable>();
        /// Each playable layer's descriptor group (Base, Additive, Gesture, Action, FX), in `controllers` order.
        static readonly List<string> groups = new List<string>();
        static int start = -1, state, late, masked, proxies;
        static bool human;
        static string hooked = "no animator";
        static Transform hips;
        static (Vector3, Quaternion) hipsRest;
        /// Per playable layer, whether its descriptor mask passes the body pose, and its controller layers' masks.
        static readonly List<(bool root, AvatarMask[] layers)> rootMasks = new List<(bool, AvatarMask[])>();
        static readonly Dictionary<AnimationClip, bool> rootCurves = new Dictionary<AnimationClip, bool>();

        /// `-civbbAvatar <prefab asset path> -civbbOut <absolute directory holding rig.json>`: writes
        /// `<state>-<view>.png` there for every state and view of the hand-off.
        public static void Render()
        {
            try
            {
                var prefab = AssetDatabase.LoadAssetAtPath<GameObject>(Arg("-civbbAvatar") ?? throw new ArgumentException("missing -civbbAvatar"))
                    ?? throw new ArgumentException("no prefab at -civbbAvatar");
                outDir = Arg("-civbbOut") ?? throw new ArgumentException("missing -civbbOut");
                handoff = JsonUtility.FromJson<Handoff>(File.ReadAllText(Path.Combine(outDir, "rig.json")));
                if (SystemInfo.graphicsDeviceType == GraphicsDeviceType.Null)
                {
                    throw new InvalidOperationException("no graphics device: run without -nographics");
                }
                EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
                // As a runtime loads an avatar (ADR 0042): instantiated, which resets Unity constraints' at-rest values.
                avatar = Object.Instantiate(prefab);
                // The root is avatar space: the exporter writes none of its transform.
                avatar.transform.localPosition = Vector3.zero;
                avatar.transform.localRotation = Quaternion.identity;
                avatar.transform.localScale = Vector3.one;
                camera =new GameObject("civbb camera").AddComponent<Camera>();
                var light = new GameObject("civbb light").AddComponent<Light>();
                light.type = LightType.Directional;
                light.color = Gamma(handoff.light_color);
                light.intensity = 1;
                light.shadows = LightShadows.None;
                light.transform.rotation = Quaternion.LookRotation(-Mirror(handoff.light_towards));
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
                Time.captureDeltaTime = handoff.dt;
                if (start < 0)
                {
                    Begin();
                    start = Time.frameCount;
                    return;
                }
                Blend();
                int frames = Time.frameCount - start, due = handoff.frames_per_state * (state + 1);
                if (frames < due)
                {
                    return;
                }
                late += frames > due ? 1 : 0;
                if (hips != null && !PlaysBodyPose())
                {
                    // No clip sets the body pose: our runtime keeps the hips at rest (ADR 0063), where Unity would sink
                    // them to body position 0.
                    hips.localPosition = hipsRest.Item1;
                    hips.localRotation = hipsRest.Item2;
                }
                for (int v = 0; v < handoff.rig.views.Length; v++)
                {
                    Shoot(handoff.rig.views[v], Path.Combine(outDir, $"{state:D3}-{handoff.rig.views[v].name}.png"));
                }
                LogState();
                if (state == 0 && Arg("-civbbBones") is string bones)
                {
                    WriteBones(bones);
                }
                state++;
                done = state == handoff.states.Length;
                if (!done)
                {
                    SetParameter(handoff.states[state].parameter, handoff.states[state].value);
                }
                else
                {
                    Debug.Log($"CIVBB rendered {state} states x {handoff.rig.views.Length} views to {outDir}" +
                              $" ({controllers.Count} playable layers, {masked} masked, {proxies} SDK clips emptied, {(human ? "humanoid" : "generic")}, {hooked}, {late} states rendered late)");
                }
            }
            catch (Exception e)
            {
                Debug.LogError($"CIVBB failed: {e}");
                done = true;
            }
            if (done)
            {
                EditorApplication.update -= Tick;
                if (graph.IsValid())
                {
                    graph.Destroy();
                }
                EditorApplication.Exit(state == handoff.states.Length ? 0 : 1);
            }
        }

        /// The fixed light, camera and target (ADR 0048), and the descriptor's playable layers as our runtime runs
        /// them: groups in VRChat's order, Additive additive, Action at weight 0, layer masks of the descriptor
        /// ignored. Like ours, the humanoid pose applies only when some clip animates muscles; otherwise the
        /// animator binds generically and the bones keep the rest pose.
        static void Begin()
        {
            RenderSettings.skybox = null;
            RenderSettings.fog = false;
            RenderSettings.ambientMode = AmbientMode.Flat;
            RenderSettings.ambientLight = Gamma(handoff.ambient);
            var sh = new SphericalHarmonicsL2();
            sh.AddAmbientLight(new Color(handoff.ambient[0], handoff.ambient[1], handoff.ambient[2]));
            RenderSettings.ambientProbe = sh;
            RenderSettings.defaultReflectionMode = DefaultReflectionMode.Custom;
            RenderSettings.reflectionIntensity = 0;
            QualitySettings.shadows = ShadowQuality.Disable;
            QualitySettings.antiAliasing = 0;
            QualitySettings.skinWeights = SkinWeights.FourBones;

            var rig = handoff.rig;
            target = new RenderTexture(new RenderTextureDescriptor(rig.width, rig.height, RenderTextureFormat.ARGB32, 24) { sRGB = true, msaaSamples = 1 });
            readback = new Texture2D(rig.width, rig.height, TextureFormat.RGB24, false, false);
            camera.enabled = false;
            camera.clearFlags = CameraClearFlags.SolidColor;
            camera.backgroundColor = Gamma(rig.background);
            camera.fieldOfView = rig.fov_y * Mathf.Rad2Deg;
            camera.nearClipPlane = 0.05f;
            camera.farClipPlane = 1000;
            camera.allowHDR = false;
            camera.allowMSAA = false;
            camera.targetTexture = target;
            // Shader time pinned to 0, as our runtime samples time-driven material effects at their start (ADR 0062).
            // Unity sets the time globals per camera, so only the camera's own command buffers can override them.
            foreach (var at in new[] { CameraEvent.BeforeForwardOpaque, CameraEvent.BeforeForwardAlpha })
            {
                var pin = new CommandBuffer { name = "civbb time 0" };
                pin.SetGlobalVector("_Time", Vector4.zero);
                pin.SetGlobalVector("_SinTime", Vector4.zero);
                pin.SetGlobalVector("_CosTime", Vector4.one);
                camera.AddCommandBuffer(at, pin);
            }
            foreach (var r in avatar.GetComponentsInChildren<SkinnedMeshRenderer>(true))
            {
                r.updateWhenOffscreen = true;
            }

            var animator = avatar.GetComponent<Animator>();
            var vrc = new VrcExport(avatar.transform, t => 0);
            var layers = vrc.Layers().Where(l => l.path != null).ToList();
            if (animator == null || layers.Count == 0)
            {
                return;
            }
            var assets = layers.Select(l => AssetDatabase.LoadAssetAtPath<AnimatorController>(l.path)).ToList();
            human = assets.Any(c => c.animationClips.Any(clip => clip != null && clip.humanMotion && !AnimatorExport.SdkAsset(clip)));
            animator.runtimeAnimatorController = null;
            if (!human)
            {
                animator.avatar = null;
            }
            else
            {
                hips = animator.GetBoneTransform(HumanBodyBones.Hips);
                hipsRest = (hips.localPosition, hips.localRotation);
            }
            animator.applyRootMotion = false;
            animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
            // Before the controllers exist: their behaviours' Awake calls the layer controls' Initialize.
            hooked = string.Join(", ", new[]
            {
                ("VRC_AvatarParameterDriver", "OnApplySettings", nameof(Drive)),
                ("VRC_PlayableLayerControl", "Initialize", nameof(InitLayerControl)),
                ("VRC_AnimatorLayerControl", "Initialize", nameof(InitLayerControl)),
            }.Select(h => $"{h.Item1} {(Hook(h.Item1, h.Item2, h.Item3) ? "hooked" : "missing")}"));
            graph = PlayableGraph.Create("civbb reference");
            graph.SetTimeUpdateMode(DirectorUpdateMode.GameTime);
            var mixer = AnimationLayerMixerPlayable.Create(graph, layers.Count);
            for (int i = 0; i < layers.Count; i++)
            {
                var playable = AnimatorControllerPlayable.Create(graph, WithoutProxies(assets[i]));
                graph.Connect(playable, 0, mixer, i);
                mixer.SetInputWeight(i, layers[i].group == "Action" ? 0 : 1);
                mixer.SetLayerAdditive((uint)i, layers[i].group == "Additive");
                bool root = true;
                if (vrc.Masks.TryGetValue(layers[i].group, out var mask) && mask != null)
                {
                    mixer.SetLayerMaskFromAvatarMask((uint)i, mask);
                    masked++;
                    root = mask.GetHumanoidBodyPartActive(AvatarMaskBodyPart.Root);
                }
                rootMasks.Add((root, assets[i].layers.Select(l => l.avatarMask).ToArray()));
                controllers.Add(playable);
                groups.Add(layers[i].group);
            }
            AnimationPlayableOutput.Create(graph, "civbb", animator).SetSourcePlayable(mixer);
            foreach (var p in handoff.parameters)
            {
                SetParameter(p.name, p.value);
            }
            graph.Play();
        }

        /// Whether clips with body pose curves (`RootT`, `RootQ`) carry the full weight of some weighted layer whose
        /// masks keep the Root part. Our runtime then places the hips from them (ADR 0063); where they blend with an
        /// emptied SDK clip Unity takes body position 0 from it and sinks the avatar, while our runtime blends towards
        /// the rest body pose, so the reference resets the hips to rest there as when no clip sets the body pose.
        static bool PlaysBodyPose()
        {
            var mixer = graph.GetOutput(0).GetSourcePlayable();
            for (int i = 0; i < controllers.Count; i++)
            {
                if (mixer.GetInputWeight(i) <= 0 || !rootMasks[i].root)
                {
                    continue;
                }
                var c = controllers[i];
                for (int l = 0; l < c.GetLayerCount(); l++)
                {
                    var mask = l < rootMasks[i].layers.Length ? rootMasks[i].layers[l] : null;
                    if ((l > 0 && c.GetLayerWeight(l) <= 0) || (mask != null && !mask.GetHumanoidBodyPartActive(AvatarMaskBodyPart.Root)))
                    {
                        continue;
                    }
                    var infos = c.GetCurrentAnimatorClipInfo(l).Concat(c.GetNextAnimatorClipInfo(l)).ToList();
                    if (infos.Count > 0 && infos.Where(ci => RootCurves(ci.clip)).Sum(ci => ci.weight) >= 0.999f * infos.Sum(ci => ci.weight))
                    {
                        return true;
                    }
                }
            }
            return false;
        }

        static bool RootCurves(AnimationClip clip)
        {
            if (!rootCurves.TryGetValue(clip, out bool has))
            {
                has = rootCurves[clip] = clip.humanMotion && AnimationUtility.GetCurveBindings(clip)
                    .Any(b => b.propertyName.StartsWith("RootT.") || b.propertyName.StartsWith("RootQ."));
            }
            return has;
        }

        /// The controller with the SDK's clips (`proxy_*`, which VRChat replaces with its own animations) played as
        /// empty clips, as our runtime plays them: the exporter references them without copying (ADR 0042).
        static RuntimeAnimatorController WithoutProxies(AnimatorController c)
        {
            var sdk = c.animationClips.Where(a => a != null && AnimatorExport.SdkAsset(a)).Distinct().ToList();
            if (sdk.Count == 0)
            {
                return c;
            }
            var empty = new AnimationClip { name = "civbb empty" };
            var o = new AnimatorOverrideController(c);
            o.ApplyOverrides(sdk.Select(a => new KeyValuePair<AnimationClip, AnimationClip>(a, empty)).ToList());
            proxies += sdk.Count;
            return o;
        }

        /// The SDK's state behaviours (parameter drivers, playable and animator layer control) only call delegates the
        /// VRChat client sets; without them they do nothing. Sets the static `VRC.SDKBase.<type>.<field>` to one of
        /// ours, by reflection (no SDK reference, ADR 0042); false when the SDK lacks it.
        static bool Hook(string type, string field, string method)
        {
            var f = AppDomain.CurrentDomain.GetAssemblies()
                .Select(a => a.GetType("VRC.SDKBase." + type)).FirstOrDefault(t => t != null)
                ?.GetField(field, BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Static);
            f?.SetValue(null, Delegate.CreateDelegate(f.FieldType, typeof(Reference).GetMethod(method, BindingFlags.NonPublic | BindingFlags.Static)));
            return f != null;
        }

        /// Weights moving linearly to a goal (layer control behaviours), stepped each frame by `Blend`.
        static readonly List<(Action<float> set, float from, float to, float start, float duration)> blends =
            new List<(Action<float>, float, float, float, float)>();

        static void StartBlend(Action<float> set, float from, float to, float duration)
        {
            blends.Add((set, from, to, Time.time, duration));
            Blend();
        }

        static void Blend()
        {
            for (int i = blends.Count - 1; i >= 0; i--)
            {
                var b = blends[i];
                float t = b.duration > 0 ? Mathf.Clamp01((Time.time - b.start) / b.duration) : 1;
                b.set(Mathf.Lerp(b.from, b.to, t));
                if (t >= 1)
                {
                    blends.RemoveAt(i);
                }
            }
        }

        static object Field(object o, string name) => o.GetType().GetField(name).GetValue(o);

        /// The layer controls' static `Initialize` (from each behaviour's Awake) is where the client gives the instance
        /// its `ApplySettings`; ours points it at `PlayableLayer` or `AnimatorLayer`.
        static void InitLayerControl(object control)
        {
            var f = control.GetType().GetField("ApplySettings", BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance);
            var method = control.GetType().Name.Contains("Playable") ? nameof(PlayableLayer) : nameof(AnimatorLayer);
            f?.SetValue(control, Delegate.CreateDelegate(f.FieldType, typeof(Reference).GetMethod(method, BindingFlags.NonPublic | BindingFlags.Static)));
        }

        /// `VRC_PlayableLayerControl`: blends a group's playable layer (Action, FX, Gesture, Additive) to a weight.
        static void PlayableLayer(object control, Animator animator)
        {
            var group = Field(control, "layer").ToString();
            var mixer = graph.GetOutput(0).GetSourcePlayable();
            for (int i = 0; i < groups.Count; i++)
            {
                if (groups[i] == group)
                {
                    int input = i;
                    StartBlend(w => mixer.SetInputWeight(input, w), mixer.GetInputWeight(input), (float)Field(control, "goalWeight"), (float)Field(control, "blendDuration"));
                }
            }
        }

        /// `VRC_AnimatorLayerControl`: blends one layer of a group's controller to a weight.
        static void AnimatorLayer(object control, Animator animator)
        {
            var group = Field(control, "playable").ToString();
            int layer = (int)Field(control, "layer");
            for (int i = 0; i < groups.Count; i++)
            {
                var c = controllers[i];
                if (groups[i] == group && layer < c.GetLayerCount())
                {
                    StartBlend(w => c.SetLayerWeight(layer, w), c.GetLayerWeight(layer), (float)Field(control, "goalWeight"), (float)Field(control, "blendDuration"));
                }
            }
        }

        /// Applies a driver's operations to every layer's controller, as the client does for the local avatar
        /// (Set, Add, Random, Copy with an optional range conversion; Random uses Unity's generator, not ours).
        static void Drive(object driver, Animator animator)
        {
            object F(object o, string name) => Field(o, name);
            foreach (var p in (System.Collections.IEnumerable)F(driver, "parameters"))
            {
                var name = (string)F(p, "name");
                float value = (float)F(p, "value");
                switch (F(p, "type").ToString())
                {
                    case "Set": SetParameter(name, value); break;
                    case "Add": SetParameter(name, GetParameter(name) + value); break;
                    case "Random":
                        float min = (float)F(p, "valueMin"), max = (float)F(p, "valueMax");
                        switch (ParameterType(name))
                        {
                            case AnimatorControllerParameterType.Bool:
                            case AnimatorControllerParameterType.Trigger:
                                SetParameter(name, UnityEngine.Random.value < (float)F(p, "chance") ? 1 : 0);
                                break;
                            case AnimatorControllerParameterType.Int:
                                SetParameter(name, UnityEngine.Random.Range((int)min, (int)max + 1));
                                break;
                            default: SetParameter(name, UnityEngine.Random.Range(min, max)); break;
                        }
                        break;
                    case "Copy":
                        float v = GetParameter((string)F(p, "source"));
                        if ((bool)F(p, "convertRange"))
                        {
                            float s0 = (float)F(p, "sourceMin"), s1 = (float)F(p, "sourceMax");
                            float d0 = (float)F(p, "destMin"), d1 = (float)F(p, "destMax");
                            // Unclamped, as our runtime converts (civbb_anim::behaviour).
                            v = s1 != s0 ? d0 + (v - s0) / (s1 - s0) * (d1 - d0) : d0;
                        }
                        SetParameter(name, v);
                        break;
                }
            }
        }

        static AnimatorControllerParameterType? ParameterType(string name)
        {
            foreach (var c in controllers)
            {
                for (int i = 0; i < c.GetParameterCount(); i++)
                {
                    if (c.GetParameter(i).name == name)
                    {
                        return c.GetParameter(i).type;
                    }
                }
            }
            return null;
        }

        /// A parameter's value from the first layer's controller that has it.
        static float GetParameter(string name)
        {
            foreach (var c in controllers)
            {
                for (int i = 0; i < c.GetParameterCount(); i++)
                {
                    var p = c.GetParameter(i);
                    if (p.name != name)
                    {
                        continue;
                    }
                    switch (p.type)
                    {
                        case AnimatorControllerParameterType.Float: return c.GetFloat(name);
                        case AnimatorControllerParameterType.Int: return c.GetInteger(name);
                        default: return c.GetBool(name) ? 1 : 0;
                    }
                }
            }
            return 0;
        }

        /// Sets a parameter on every layer's controller that has it, as VRChat shares parameters across layers.
        static void SetParameter(string name, float value)
        {
            foreach (var c in controllers)
            {
                for (int i = 0; i < c.GetParameterCount(); i++)
                {
                    var p = c.GetParameter(i);
                    if (p.name != name)
                    {
                        continue;
                    }
                    switch (p.type)
                    {
                        case AnimatorControllerParameterType.Float: c.SetFloat(name, value); break;
                        case AnimatorControllerParameterType.Int: c.SetInteger(name, Mathf.RoundToInt(value)); break;
                        case AnimatorControllerParameterType.Bool: c.SetBool(name, value != 0); break;
                        default:
                            if (value != 0) c.SetTrigger(name);
                            else c.ResetTrigger(name);
                            break;
                    }
                }
            }
        }

        /// Per rendered state, one line per playable layer (its mixer weight, its controller layers' weights, current
        /// clips and, mid-transition, the next state's clips after `->`) and the human pose's muscles, to tell a pose
        /// difference's source (#86). Lines start `CIVBB <state name>`.
        /// `-civbbBones <absolute file>`: every transform's path under the avatar and its world position at rest, as
        /// `path|x|y|z` in our handedness (x mirrored), to compare PhysBone rest with ours.
        static void WriteBones(string file)
        {
            var lines = avatar.GetComponentsInChildren<Transform>(true).Where(t => t != avatar.transform).Select(t =>
            {
                var path = t.name;
                for (var p = t.parent; p != avatar.transform; p = p.parent)
                {
                    path = $"{p.name}/{path}";
                }
                var w = t.position;
                return FormattableString.Invariant($"{path}|{-w.x:0.00000}|{w.y:0.00000}|{w.z:0.00000}");
            });
            File.WriteAllLines(file, lines);
        }

        static void LogState()
        {
            if (!graph.IsValid())
            {
                return;
            }
            var mixer = graph.GetOutput(0).GetSourcePlayable();
            var name = handoff.states[state].name;
            for (int i = 0; i < controllers.Count; i++)
            {
                var c = controllers[i];
                var parts = Enumerable.Range(0, c.GetLayerCount()).Select(l =>
                    $"{c.GetLayerName(l)}@{c.GetLayerWeight(l):0.##}=[{string.Join(",", c.GetCurrentAnimatorClipInfo(l).Select(ci => $"{ci.clip.name}{(ci.clip.humanMotion ? "(human)" : "")}{(ci.weight < 0.999f ? $"*{ci.weight:0.##}" : "")}"))}]" +
                    (c.IsInTransition(l) ? $"->[{string.Join(",", c.GetNextAnimatorClipInfo(l).Select(ci => ci.clip.name))}]" : ""));
                Debug.Log($"CIVBB {name} layer {i}@{mixer.GetInputWeight(i):0.##}: {string.Join(" ", parts)}");
            }
            if (human)
            {
                var pose = new HumanPose();
                new HumanPoseHandler(avatar.GetComponent<Animator>().avatar, avatar.transform).GetHumanPose(ref pose);
                Debug.Log($"CIVBB {name} muscles: {string.Join(" ", pose.muscles.Select(m => m.ToString("0.###")))}");
            }
        }

        static void Shoot(View v, string path)
        {
            camera.transform.position = Mirror(v.eye);
            camera.transform.LookAt(Mirror(v.target), Vector3.up);
            camera.Render();
            var active = RenderTexture.active;
            RenderTexture.active = target;
            readback.ReadPixels(new Rect(0, 0, target.width, target.height), 0, 0, false);
            RenderTexture.active = active;
            File.WriteAllBytes(path, readback.EncodeToPNG());
        }

        /// Avatar space (glTF, ADR 0006) to Unity's: X mirrored, as the exporter writes it (`Json.Vec`).
        static Vector3 Mirror(float[] v) => new Vector3(-v[0], v[1], v[2]);

        /// A linear colour as Unity's colour properties take it in a linear project: gamma-encoded.
        static Color Gamma(float[] c) => new Color(c[0], c[1], c[2]).gamma;

        static string Arg(string name)
        {
            var args = Environment.GetCommandLineArgs();
            int i = Array.IndexOf(args, name);
            return i >= 0 && i + 1 < args.Length ? args[i + 1] : null;
        }
    }
}
