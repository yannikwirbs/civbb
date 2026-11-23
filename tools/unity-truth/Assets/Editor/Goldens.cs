//! Plays the animator golden cases (`crates/anim/tests/golden/*.json`, #54) in Unity: builds each case's controller
//! with the AnimatorController API, steps it frame by frame as `golden.rs` does (parameters set, then one update of
//! `dt`) and records every expected value and current state, so the cases' expectations come from Unity.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using UnityEngine.Animations;
using UnityEngine.Playables;

public static class Goldens
{
    /// Entry point: `-executeMethod Goldens.Record -civbbCases <abs dir> -civbbOut <abs path>/goldens.json`.
    public static void Record()
    {
        try
        {
            var args = Environment.GetCommandLineArgs();
            string Arg(string n) => args[Array.IndexOf(args, n) + 1];
            var s = new StringBuilder("{\n  \"unity\": \"" + Application.unityVersion + "\",\n  \"cases\": {");
            bool first = true;
            foreach (var path in Directory.GetFiles(Arg("-civbbCases"), "*.json").OrderBy(p => p, StringComparer.Ordinal))
            {
                var c = (Dictionary<string, object>)Json.Parse(File.ReadAllText(path));
                s.Append(first ? "\n    " : ",\n    ").Append(Str(Path.GetFileNameWithoutExtension(path))).Append(": ").Append(Play(c));
                first = false;
            }
            File.WriteAllText(Arg("-civbbOut"), s.Append("\n  }\n}\n").ToString());
            Debug.Log("CIVBB goldens recorded");
        }
        catch (Exception e)
        {
            Debug.LogError($"CIVBB failed: {e}");
        }
        finally
        {
            EditorApplication.Exit(0);
        }
    }

    static List<object> L(object o) => o as List<object> ?? new List<object>();
    static Dictionary<string, object> D(object o) => o as Dictionary<string, object> ?? new Dictionary<string, object>();
    static float N(object o) => Convert.ToSingle(o, CultureInfo.InvariantCulture);
    static object Get(Dictionary<string, object> d, string k) => d.TryGetValue(k, out var v) ? v : null;

    /// The case's frames as Unity plays them: per frame, every expected key's value.
    static string Play(Dictionary<string, object> c)
    {
        var root = new GameObject("Root");
        var nodes = new Dictionary<string, GameObject>();
        foreach (Dictionary<string, object> n in L(c["nodes"]))
        {
            var go = new GameObject((string)n["name"]);
            go.transform.SetParent(root.transform, false);
            go.SetActive(!(Get(n, "active") is bool b) || b);
            nodes[go.name] = go;
        }
        var ctrl = new AnimatorController();
        var kinds = new Dictionary<string, string>();
        foreach (Dictionary<string, object> p in L(c["params"]))
        {
            var name = (string)p["name"];
            var kind = (string)p["kind"];
            kinds[name] = kind;
            var type = kind == "Bool" ? AnimatorControllerParameterType.Bool : kind == "Int" ? AnimatorControllerParameterType.Int
                : kind == "Trigger" ? AnimatorControllerParameterType.Trigger : AnimatorControllerParameterType.Float;
            float def = Get(p, "default") == null ? 0f : N(p["default"]);
            ctrl.AddParameter(new AnimatorControllerParameter
            {
                name = name, type = type, defaultBool = def != 0f, defaultFloat = def, defaultInt = (int)def,
            });
        }
        var clips = new Dictionary<string, AnimationClip>();
        foreach (var kv in D(c["clips"]))
        {
            var spec = D(kv.Value);
            var clip = new AnimationClip { name = kv.Key };
            float length = N(spec["length"]);
            foreach (List<object> curve in L(spec["curves"]))
            {
                var target = (string)curve[0];
                int dot = target.LastIndexOf('.');
                var (path, prop) = (target.Substring(0, dot), target.Substring(dot + 1));
                var keys = L(curve[1]).Select(k => (N(L(k)[0]), N(L(k)[1]))).ToList();
                // Linear tangents, as golden.rs builds them; a flat key at the clip's length gives Unity its length.
                if (keys[keys.Count - 1].Item1 < length)
                    keys.Add((length, keys[keys.Count - 1].Item2));
                var frames = new Keyframe[keys.Count];
                for (int i = 0; i < keys.Count; i++)
                {
                    float slope(int a, int b) => (keys[b].Item2 - keys[a].Item2) / (keys[b].Item1 - keys[a].Item1);
                    frames[i] = new Keyframe(keys[i].Item1, keys[i].Item2, i > 0 ? slope(i - 1, i) : 0f, i + 1 < keys.Count ? slope(i, i + 1) : 0f);
                }
                if (prop == "active")
                    clip.SetCurve(path, typeof(GameObject), "m_IsActive", new AnimationCurve(frames));
                else
                    clip.SetCurve(path, typeof(Transform), "m_LocalPosition." + prop, new AnimationCurve(frames));
            }
            var settings = AnimationUtility.GetAnimationClipSettings(clip);
            settings.loopTime = Get(spec, "looping") is bool loop && loop;
            AnimationUtility.SetAnimationClipSettings(clip, settings);
            clips[kv.Key] = clip;
        }
        Motion MotionOf(object m)
        {
            var d = D(m);
            if (d.ContainsKey("clip"))
                return clips[(string)d["clip"]];
            var tree = new BlendTree { useAutomaticThresholds = false };
            if (d.ContainsKey("simple1d"))
            {
                var spec = L(d["simple1d"]);
                tree.blendType = BlendTreeType.Simple1D;
                tree.blendParameter = (string)spec[0];
                tree.children = L(spec[1]).Select(ch => new ChildMotion { threshold = N(L(ch)[0]), motion = clips[(string)L(ch)[1]], timeScale = 1f }).ToArray();
            }
            else
            {
                tree.blendType = BlendTreeType.Direct;
                tree.children = L(d["direct"]).Select(ch => new ChildMotion { directBlendParameter = (string)L(ch)[0], motion = clips[(string)L(ch)[1]], timeScale = 1f }).ToArray();
            }
            return tree;
        }
        var layerNames = new List<string>();
        var stateNames = new List<List<string>>();
        foreach (Dictionary<string, object> l in L(c["layers"]))
        {
            var sm = new AnimatorStateMachine { name = (string)l["name"] };
            var states = new Dictionary<string, AnimatorState>();
            var names = new List<string>();
            foreach (Dictionary<string, object> st in L(l["states"]))
            {
                var state = sm.AddState((string)st["name"]);
                state.writeDefaultValues = (bool)st["wd"];
                state.motion = Get(st, "motion") == null ? null : MotionOf(st["motion"]);
                states[state.name] = state;
                names.Add(state.name);
            }
            sm.defaultState = states[names[0]];
            foreach (Dictionary<string, object> st in L(l["states"]))
            {
                foreach (Dictionary<string, object> t in L(Get(st, "transitions")))
                {
                    var tr = states[(string)st["name"]].AddTransition(states[(string)t["to"]]);
                    foreach (var p in L(Get(t, "if")))
                        tr.AddCondition(kinds[(string)p] == "Float" ? AnimatorConditionMode.Greater : AnimatorConditionMode.If, 0f, (string)p);
                    foreach (var p in L(Get(t, "if_not")))
                        tr.AddCondition(kinds[(string)p] == "Float" ? AnimatorConditionMode.Less : AnimatorConditionMode.IfNot, kinds[(string)p] == "Float" ? 0.5f : 0f, (string)p);
                    tr.hasExitTime = Get(t, "exit_time") != null;
                    tr.exitTime = tr.hasExitTime ? N(t["exit_time"]) : 0f;
                    tr.hasFixedDuration = true;
                    tr.duration = Get(t, "duration") == null ? 0f : N(t["duration"]);
                    tr.offset = 0f;
                    tr.interruptionSource = Get(t, "interruption") == null ? TransitionInterruptionSource.None
                        : (TransitionInterruptionSource)Enum.Parse(typeof(TransitionInterruptionSource), (string)t["interruption"]);
                    tr.orderedInterruption = !(Get(t, "ordered") is bool o) || o;
                    tr.canTransitionToSelf = false;
                }
            }
            AvatarMask mask = null;
            if (Get(l, "mask") != null)
            {
                // Only the listed transform paths are written; nodes are the root's children, so a path is a name.
                var paths = L(l["mask"]).Cast<string>().ToList();
                mask = new AvatarMask { transformCount = paths.Count };
                for (int i = 0; i < paths.Count; i++)
                {
                    mask.SetTransformPath(i, paths[i]);
                    mask.SetTransformActive(i, true);
                }
            }
            ctrl.AddLayer(new AnimatorControllerLayer
            {
                avatarMask = mask,
                name = (string)l["name"],
                defaultWeight = Get(l, "weight") == null ? 1f : N(l["weight"]),
                blendingMode = Get(l, "additive") is bool add && add ? AnimatorLayerBlendingMode.Additive : AnimatorLayerBlendingMode.Override,
                stateMachine = sm,
            });
            layerNames.Add((string)l["name"]);
            stateNames.Add(names);
        }
        var animator = root.AddComponent<Animator>();
        animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
        var graph = PlayableGraph.Create();
        var playable = AnimatorControllerPlayable.Create(graph, ctrl);
        AnimationPlayableOutput.Create(graph, "o", animator).SetSourcePlayable(playable);
        float dt = N(c["dt"]);
        var s = new StringBuilder("[");
        int f = 0;
        foreach (Dictionary<string, object> frame in L(c["frames"]))
        {
            foreach (var kv in D(Get(frame, "set")))
            {
                float v = N(kv.Value);
                switch (kinds[kv.Key])
                {
                    case "Bool": playable.SetBool(kv.Key, v != 0f); break;
                    case "Int": playable.SetInteger(kv.Key, (int)v); break;
                    case "Trigger": if (v != 0f) playable.SetTrigger(kv.Key); else playable.ResetTrigger(kv.Key); break;
                    default: playable.SetFloat(kv.Key, v); break;
                }
            }
            graph.Evaluate(dt);
            var parts = new List<string>();
            foreach (var key in D(frame["expect"]).Keys)
            {
                int li = layerNames.IndexOf(key);
                if (li >= 0)
                {
                    int hash = playable.GetCurrentAnimatorStateInfo(li).shortNameHash;
                    parts.Add(Str(key) + ": " + Str(stateNames[li].FirstOrDefault(n => Animator.StringToHash(n) == hash) ?? "?"));
                    continue;
                }
                int dot = key.LastIndexOf('.');
                var go = nodes[key.Substring(0, dot)];
                var prop = key.Substring(dot + 1);
                float value = prop == "active" ? (go.activeSelf ? 1f : 0f) : go.transform.localPosition["xyz".IndexOf(prop)];
                parts.Add(Str(key) + ": " + value.ToString("R", CultureInfo.InvariantCulture));
            }
            s.Append(f++ == 0 ? "\n      {" : ",\n      {").Append(string.Join(", ", parts)).Append('}');
        }
        graph.Destroy();
        UnityEngine.Object.DestroyImmediate(root);
        return s.Append("\n    ]").ToString();
    }

    static string Str(string s) => "\"" + s.Replace("\\", "\\\\").Replace("\"", "\\\"") + "\"";

    /// A minimal JSON reader: objects, arrays, strings, numbers, booleans and null.
    static class Json
    {
        public static object Parse(string text)
        {
            int i = 0;
            return Value(text, ref i);
        }

        static void Skip(string t, ref int i)
        {
            while (i < t.Length && char.IsWhiteSpace(t[i])) i++;
        }

        static object Value(string t, ref int i)
        {
            Skip(t, ref i);
            char ch = t[i];
            if (ch == '{')
            {
                var d = new Dictionary<string, object>();
                i++;
                Skip(t, ref i);
                if (t[i] == '}') { i++; return d; }
                while (true)
                {
                    var key = (string)Value(t, ref i);
                    Skip(t, ref i);
                    i++; // ':'
                    d[key] = Value(t, ref i);
                    Skip(t, ref i);
                    if (t[i++] == '}') return d;
                }
            }
            if (ch == '[')
            {
                var l = new List<object>();
                i++;
                Skip(t, ref i);
                if (t[i] == ']') { i++; return l; }
                while (true)
                {
                    l.Add(Value(t, ref i));
                    Skip(t, ref i);
                    if (t[i++] == ']') return l;
                }
            }
            if (ch == '"')
            {
                var sb = new StringBuilder();
                i++;
                while (t[i] != '"')
                {
                    if (t[i] == '\\') { i++; sb.Append(t[i] == 'n' ? '\n' : t[i]); }
                    else sb.Append(t[i]);
                    i++;
                }
                i++;
                return sb.ToString();
            }
            if (t.Substring(i).StartsWith("true")) { i += 4; return true; }
            if (t.Substring(i).StartsWith("false")) { i += 5; return false; }
            if (t.Substring(i).StartsWith("null")) { i += 4; return null; }
            int start = i;
            while (i < t.Length && "+-.0123456789eE".IndexOf(t[i]) >= 0) i++;
            return double.Parse(t.Substring(start, i - start), CultureInfo.InvariantCulture);
        }
    }
}
