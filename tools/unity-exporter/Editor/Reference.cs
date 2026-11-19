// Reference renders for parity (design doc §10, ADR 0048): renders an avatar prefab with the camera rig, light and
// states of a `civbb-client rig` hand-off, one PNG per state and view, in play mode so PhysBones, contacts and
// constraints run. Needs a graphics device: run the Editor without -nographics and without -quit; it exits itself.
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
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
        static int start = -1, state, late, masked, proxies;
        static bool human;
        static Transform hips;
        static (Vector3, Quaternion) hipsRest;

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
                int frames = Time.frameCount - start, due = handoff.frames_per_state * (state + 1);
                if (frames < due)
                {
                    return;
                }
                late += frames > due ? 1 : 0;
                if (hips != null)
                {
                    // Our runtime does not apply the body pose yet (#52): the hips keep their rest transform.
                    hips.localPosition = hipsRest.Item1;
                    hips.localRotation = hipsRest.Item2;
                }
                for (int v = 0; v < handoff.rig.views.Length; v++)
                {
                    Shoot(handoff.rig.views[v], Path.Combine(outDir, $"{state:D3}-{handoff.rig.views[v].name}.png"));
                }
                if (state == 0)
                {
                    LogRest();
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
                              $" ({controllers.Count} playable layers, {masked} masked, {proxies} SDK clips emptied, {(human ? "humanoid" : "generic")}, {late} states rendered late)");
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
            graph = PlayableGraph.Create("civbb reference");
            graph.SetTimeUpdateMode(DirectorUpdateMode.GameTime);
            var mixer = AnimationLayerMixerPlayable.Create(graph, layers.Count);
            for (int i = 0; i < layers.Count; i++)
            {
                var playable = AnimatorControllerPlayable.Create(graph, WithoutProxies(assets[i]));
                graph.Connect(playable, 0, mixer, i);
                mixer.SetInputWeight(i, layers[i].group == "Action" ? 0 : 1);
                mixer.SetLayerAdditive((uint)i, layers[i].group == "Additive");
                if (vrc.Masks.TryGetValue(layers[i].group, out var mask) && mask != null)
                {
                    mixer.SetLayerMaskFromAvatarMask((uint)i, mask);
                    masked++;
                }
                controllers.Add(playable);
            }
            AnimationPlayableOutput.Create(graph, "civbb", animator).SetSourcePlayable(mixer);
            foreach (var p in handoff.parameters)
            {
                SetParameter(p.name, p.value);
            }
            graph.Play();
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

        /// One line per playable layer at rest (its controller layers' weights, current clips and, mid-transition, the
        /// next state's clips after `->`), and the human
        /// pose's muscles, to tell a pose difference's source (#86).
        static void LogRest()
        {
            for (int i = 0; i < controllers.Count; i++)
            {
                var c = controllers[i];
                var parts = Enumerable.Range(0, c.GetLayerCount()).Select(l =>
                    $"{c.GetLayerName(l)}@{c.GetLayerWeight(l):0.##}=[{string.Join(",", c.GetCurrentAnimatorClipInfo(l).Select(ci => $"{ci.clip.name}{(ci.clip.humanMotion ? "(human)" : "")}"))}]" +
                    (c.IsInTransition(l) ? $"->[{string.Join(",", c.GetNextAnimatorClipInfo(l).Select(ci => ci.clip.name))}]" : ""));
                Debug.Log($"CIVBB rest layer {i}: {string.Join(" ", parts)}");
            }
            if (human)
            {
                var pose = new HumanPose();
                new HumanPoseHandler(avatar.GetComponent<Animator>().avatar, avatar.transform).GetHumanPose(ref pose);
                Debug.Log($"CIVBB rest muscles: {string.Join(" ", pose.muscles.Select(m => m.ToString("0.###")))}");
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
