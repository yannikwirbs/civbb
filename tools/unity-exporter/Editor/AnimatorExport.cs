// Animator controllers and their clips as X_animator (ADR 0041): format `Animator` and `Clip` JSON. Sub-state
// machines are flattened, muted transitions dropped, curves converted per ADR 0039 (bake weighted segments, split
// equal key times, X mirror).
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using static Civbb.Exporter.Json;

namespace Civbb.Exporter
{
    static class AnimatorExport
    {
        /// The extension JSON (null without layers) and a report fragment.
        public static (string json, string report) Write(Transform root, IList<(string group, string path)> layers)
        {
            if (layers.Count == 0)
            {
                return (null, "no animator");
            }
            var w = new Writer();
            var controllers = layers.Select(l => AssetDatabase.LoadAssetAtPath<AnimatorController>(l.path)
                ?? throw new ArgumentException($"no animator controller at {l.path}")).ToList();
            // Parameters first: a clip in one controller may drive another's parameter.
            controllers.ForEach(w.Parameters);
            for (int g = 0; g < controllers.Count; g++)
            {
                w.Layers(controllers[g], g);
            }
            var groups = layers.Select(l => $"{{\"name\":{Str(l.group)},\"weight\":1}}");
            var json = $"{{\"animator\":{{\"parameters\":[{string.Join(",", w.parameters)}]," +
                $"\"groups\":[{string.Join(",", groups)}],\"layers\":[{string.Join(",", w.layers)}]}}," +
                $"\"clips\":[{string.Join(",", w.clips)}]}}";
            return (json, w.Report());
        }

        sealed class Writer
        {
            public readonly List<string> parameters = new List<string>(), layers = new List<string>(), clips = new List<string>();
            readonly Dictionary<string, (int index, AnimatorControllerParameterType type)> paramIndex =
                new Dictionary<string, (int, AnimatorControllerParameterType)>();
            readonly Dictionary<AnimationClip, int> clipIndex = new Dictionary<AnimationClip, int>();
            readonly HashSet<string> muscles = Muscles();
            int states, subMachines, machineTransitions, droppedTransitions, syncedLayers, typeClashes, emptyMotions,
                curves, objectCurves, componentCurves, bakedSegments, jumps, emptyLayers;
            int emptyClip = -1;

            public string Report() =>
                $"animator {layers.Count} layers, {parameters.Count} parameters, {states} states, {clips.Count} clips, " +
                $"{curves} curves ({componentCurves} other component, {objectCurves} object curves skipped, " +
                $"{bakedSegments} weighted segments baked, {jumps} jumps split), {subMachines} sub-state machines, " +
                $"{machineTransitions} state machine transitions skipped, {droppedTransitions} transitions dropped, " +
                $"{syncedLayers} synced layers, {typeClashes} parameter type clashes, {emptyMotions} empty motions, " +
                $"{emptyLayers} layers without states";

            public void Parameters(AnimatorController ac)
            {
                foreach (var p in ac.parameters)
                {
                    if (paramIndex.TryGetValue(p.name, out var known))
                    {
                        typeClashes += known.type != p.type ? 1 : 0;
                        continue;
                    }
                    paramIndex[p.name] = (parameters.Count, p.type);
                    string kind = p.type == AnimatorControllerParameterType.Float ? "Float"
                        : p.type == AnimatorControllerParameterType.Int ? "Int"
                        : p.type == AnimatorControllerParameterType.Bool ? "Bool" : "Trigger";
                    float value = p.type == AnimatorControllerParameterType.Float ? p.defaultFloat
                        : p.type == AnimatorControllerParameterType.Int ? p.defaultInt : p.defaultBool ? 1 : 0;
                    parameters.Add($"{{\"name\":{Str(p.name)},\"kind\":\"{kind}\",\"default\":{Num(value)}}}");
                }
            }

            public void Layers(AnimatorController ac, int group)
            {
                foreach (var l in ac.layers)
                {
                    syncedLayers += l.syncedLayerIndex >= 0 ? 1 : 0;
                    var mask = l.avatarMask != null ? Mask(l.avatarMask) : "null";
                    layers.Add($"{{\"name\":{Str(l.name)},\"group\":{group},\"weight\":{Num(l.defaultWeight)}," +
                        $"\"blending\":\"{(l.blendingMode == AnimatorLayerBlendingMode.Additive ? "Additive" : "Override")}\"," +
                        $"\"mask\":{mask},\"state_machine\":{Machine(l.stateMachine)}}}");
                }
            }

            static string Mask(AvatarMask m)
            {
                var parts = new List<string>();
                for (int i = 0; i < (int)AvatarMaskBodyPart.LastBodyPart; i++)
                {
                    if (m.GetHumanoidBodyPartActive((AvatarMaskBodyPart)i))
                    {
                        parts.Add(Str(((AvatarMaskBodyPart)i).ToString().Replace("IK", "Ik")));
                    }
                }
                string transforms = "null";
                if (m.transformCount > 0)
                {
                    var paths = new List<string>();
                    for (int i = 0; i < m.transformCount; i++)
                    {
                        if (m.GetTransformActive(i))
                        {
                            paths.Add(Str(m.GetTransformPath(i)));
                        }
                    }
                    transforms = "[" + string.Join(",", paths) + "]";
                }
                return $"{{\"body_parts\":[{string.Join(",", parts)}],\"transforms\":{transforms}}}";
            }

            string Machine(AnimatorStateMachine root)
            {
                var all = new List<AnimatorState>();
                var machines = new List<AnimatorStateMachine>();
                Gather(root, all, machines);
                subMachines += machines.Count - 1;
                states += all.Count;
                var index = new Dictionary<AnimatorState, int>();
                for (int i = 0; i < all.Count; i++)
                {
                    index[all[i]] = i;
                }
                var body = new List<string>();
                foreach (var s in all)
                {
                    var motion = s.motion != null ? Motion(s.motion) : "null";
                    body.Add($"{{\"name\":{Str(s.name)},\"motion\":{motion},\"speed\":{Num(s.speed)}," +
                        $"\"speed_param\":{Param(s.speedParameterActive, s.speedParameter)}," +
                        $"\"cycle_offset\":{Num(s.cycleOffset)}," +
                        $"\"cycle_offset_param\":{Param(s.cycleOffsetParameterActive, s.cycleOffsetParameter)}," +
                        $"\"time_param\":{Param(s.timeParameterActive, s.timeParameter)}," +
                        $"\"write_defaults\":{Bool(s.writeDefaultValues)}," +
                        $"\"transitions\":[{Transitions(s.transitions, index)}],\"behaviours\":[]}}");
                }
                foreach (var m in machines)
                {
                    foreach (var sub in m.stateMachines)
                    {
                        machineTransitions += m.GetStateMachineTransitions(sub.stateMachine).Length;
                    }
                }
                if (body.Count == 0)
                {
                    // A layer without states animates nothing; one empty state without Write Defaults keeps it so.
                    emptyLayers++;
                    body.Add("{\"name\":\"(empty)\",\"motion\":null,\"speed\":1,\"speed_param\":null,\"cycle_offset\":0," +
                        "\"cycle_offset_param\":null,\"time_param\":null,\"write_defaults\":false,\"transitions\":[],\"behaviours\":[]}");
                }
                var any = machines.SelectMany(m => m.anyStateTransitions).ToArray();
                int defaultState = root.defaultState != null && index.TryGetValue(root.defaultState, out int d) ? d : 0;
                return $"{{\"states\":[{string.Join(",", body)}],\"default_state\":{defaultState}," +
                    $"\"entry_transitions\":[{Transitions(root.entryTransitions, index)}]," +
                    $"\"any_state_transitions\":[{Transitions(any, index)}]}}";
            }

            static void Gather(AnimatorStateMachine m, List<AnimatorState> states, List<AnimatorStateMachine> machines)
            {
                machines.Add(m);
                states.AddRange(m.states.Select(s => s.state));
                foreach (var sub in m.stateMachines)
                {
                    Gather(sub.stateMachine, states, machines);
                }
            }

            string Param(bool active, string name) =>
                active && paramIndex.TryGetValue(name, out var p) ? p.index.ToString() : "null";

            /// Unmuted transitions (only the solo ones when any is solo), as format `Transition`s.
            string Transitions(AnimatorTransitionBase[] list, Dictionary<AnimatorState, int> index)
            {
                var live = list.Where(t => !t.mute).ToList();
                if (live.Any(t => t.solo))
                {
                    live = live.Where(t => t.solo).ToList();
                }
                droppedTransitions += list.Length - live.Count;
                var out_ = new List<string>();
                foreach (var t in live)
                {
                    string dest;
                    var target = t.destinationState ?? t.destinationStateMachine?.defaultState;
                    if (t.isExit)
                    {
                        dest = "\"Exit\"";
                    }
                    else if (target != null && index.TryGetValue(target, out int i))
                    {
                        dest = $"{{\"State\":{i}}}";
                    }
                    else
                    {
                        droppedTransitions++;
                        continue;
                    }
                    var conds = new List<string>();
                    bool ok = true;
                    foreach (var c in t.conditions)
                    {
                        if (!paramIndex.TryGetValue(c.parameter, out var p))
                        {
                            ok = false;
                            break;
                        }
                        conds.Add($"{{\"param\":{p.index},\"mode\":\"{c.mode}\",\"threshold\":{Num(c.threshold)}}}");
                    }
                    if (!ok)
                    {
                        droppedTransitions++;
                        continue;
                    }
                    var s = t as AnimatorStateTransition;
                    out_.Add($"{{\"dest\":{dest},\"conditions\":[{string.Join(",", conds)}]," +
                        $"\"has_exit_time\":{Bool(s != null && s.hasExitTime)},\"exit_time\":{Num(s?.exitTime ?? 0)}," +
                        $"\"fixed_duration\":{Bool(s == null || s.hasFixedDuration)},\"duration\":{Num(s?.duration ?? 0)}," +
                        $"\"offset\":{Num(s?.offset ?? 0)},\"interruption\":\"{s?.interruptionSource ?? TransitionInterruptionSource.None}\"," +
                        $"\"ordered_interruption\":{Bool(s == null || s.orderedInterruption)}," +
                        $"\"can_transition_to_self\":{Bool(s != null && s.canTransitionToSelf)}}}");
                }
                return string.Join(",", out_);
            }

            string Motion(Motion m)
            {
                if (m is AnimationClip clip)
                {
                    return $"{{\"Clip\":{Clip(clip)}}}";
                }
                var tree = (BlendTree)m;
                string kind;
                string x = Index(tree.blendParameter), y = Index(tree.blendParameterY);
                switch (tree.blendType)
                {
                    case BlendTreeType.Simple1D: kind = $"{{\"Simple1D\":{{\"param\":{x}}}}}"; break;
                    case BlendTreeType.SimpleDirectional2D: kind = $"{{\"SimpleDirectional2D\":{{\"x\":{x},\"y\":{y}}}}}"; break;
                    case BlendTreeType.FreeformDirectional2D: kind = $"{{\"FreeformDirectional2D\":{{\"x\":{x},\"y\":{y}}}}}"; break;
                    case BlendTreeType.FreeformCartesian2D: kind = $"{{\"FreeformCartesian2D\":{{\"x\":{x},\"y\":{y}}}}}"; break;
                    default: kind = "\"Direct\""; break;
                }
                var children = tree.children.Select(c =>
                {
                    string motion;
                    if (c.motion != null)
                    {
                        motion = Motion(c.motion);
                    }
                    else
                    {
                        emptyMotions++;
                        motion = $"{{\"Clip\":{EmptyClip()}}}";
                    }
                    string direct = tree.blendType == BlendTreeType.Direct && paramIndex.TryGetValue(c.directBlendParameter ?? "", out var p)
                        ? p.index.ToString() : "null";
                    return $"{{\"motion\":{motion},\"threshold\":{Num(c.threshold)},\"position\":{Nums(c.position.x, c.position.y)}," +
                        $"\"direct_param\":{direct},\"time_scale\":{Num(c.timeScale)},\"cycle_offset\":{Num(c.cycleOffset)}}}";
                });
                bool normalize = new SerializedObject(tree).FindProperty("m_NormalizedBlendValues")?.boolValue ?? false;
                return $"{{\"BlendTree\":{{\"kind\":{kind},\"children\":[{string.Join(",", children)}],\"normalize\":{Bool(normalize)}}}}}";
            }

            /// A blend parameter's index; a missing one becomes 0 with a parameter added for it, as Unity reads 0.
            string Index(string name)
            {
                if (!paramIndex.TryGetValue(name ?? "", out var p))
                {
                    p = (parameters.Count, AnimatorControllerParameterType.Float);
                    paramIndex[name ?? ""] = p;
                    parameters.Add($"{{\"name\":{Str(name ?? "")},\"kind\":\"Float\",\"default\":0}}");
                }
                return p.index.ToString();
            }

            int EmptyClip()
            {
                if (emptyClip < 0)
                {
                    emptyClip = clips.Count;
                    clips.Add("{\"name\":\"(none)\",\"length\":0,\"looping\":false,\"curves\":[]}");
                }
                return emptyClip;
            }

            int Clip(AnimationClip clip)
            {
                if (clipIndex.TryGetValue(clip, out int found))
                {
                    return found;
                }
                var list = new List<string>();
                foreach (var b in AnimationUtility.GetCurveBindings(clip))
                {
                    var (property, mirror) = Property(b);
                    var keys = Keys(AnimationUtility.GetEditorCurve(clip, b), mirror);
                    list.Add($"{{\"binding\":{{\"path\":{Str(b.path)},\"property\":{property}}},\"keys\":[{keys}]}}");
                    curves++;
                }
                objectCurves += AnimationUtility.GetObjectReferenceCurveBindings(clip).Length;
                bool loop = AnimationUtility.GetAnimationClipSettings(clip).loopTime;
                clipIndex[clip] = clips.Count;
                clips.Add($"{{\"name\":{Str(clip.name)},\"length\":{Num(clip.length)},\"looping\":{Bool(loop)},\"curves\":[{string.Join(",", list)}]}}");
                return clips.Count - 1;
            }

            /// The format `Property` of a binding, and whether its values negate under the X mirror (ADR 0039).
            (string property, bool mirror) Property(EditorCurveBinding b)
            {
                var name = b.propertyName;
                int Axis(string prefix) => name.Length == prefix.Length + 2 && name.StartsWith(prefix + ".")
                    ? "xyzw".IndexOf(name[name.Length - 1]) : -1;
                if (b.type == typeof(Transform))
                {
                    int i;
                    if ((i = Axis("m_LocalPosition")) >= 0 && i < 3) return ($"{{\"Position\":{i}}}", i == 0);
                    if ((i = Axis("m_LocalRotation")) >= 0) return ($"{{\"Rotation\":{i}}}", i == 1 || i == 2);
                    if (((i = Axis("localEulerAnglesRaw")) >= 0 || (i = Axis("localEulerAngles")) >= 0) && i < 3)
                        return ($"{{\"EulerRotation\":{i}}}", i > 0);
                    if ((i = Axis("m_LocalScale")) >= 0 && i < 3) return ($"{{\"Scale\":{i}}}", false);
                }
                if (b.type == typeof(GameObject) && name == "m_IsActive") return ("\"Active\"", false);
                if (typeof(Renderer).IsAssignableFrom(b.type))
                {
                    if (name == "m_Enabled") return ("\"RendererEnabled\"", false);
                    if (name.StartsWith("blendShape.")) return ($"{{\"BlendShape\":{Str(name.Substring(11))}}}", false);
                    if (name.StartsWith("material.")) return ($"{{\"Material\":{Str(name.Substring(9))}}}", false);
                }
                if (b.type == typeof(Animator))
                {
                    int i;
                    if ((i = Axis("RootT")) >= 0 && i < 3) return ($"{{\"RootMotion\":{i}}}", i == 0);
                    if ((i = Axis("RootQ")) >= 0) return ($"{{\"RootMotion\":{3 + i}}}", i == 1 || i == 2);
                    if (muscles.Contains(name)) return ($"{{\"Muscle\":{Str(name)}}}", false);
                    if (paramIndex.ContainsKey(name)) return ($"{{\"Parameter\":{Str(name)}}}", false);
                }
                var constraint = ConstraintType(b.type);
                if (constraint != null)
                {
                    string field = name == "m_Weight" ? "\"Weight\"" : name == "m_Active" ? "\"Active\"" : null;
                    const string source = "m_Sources.Array.data[";
                    if (name.StartsWith(source) && name.EndsWith("].weight") &&
                        int.TryParse(name.Substring(source.Length, name.Length - source.Length - 8), out int n) && n < 256)
                    {
                        field = $"{{\"SourceWeight\":{n}}}";
                    }
                    if (field != null) return ($"{{\"Constraint\":{{\"kind\":\"{constraint}\",\"field\":{field}}}}}", false);
                }
                componentCurves++;
                return ($"{{\"Component\":{{\"component\":{Str(b.type.Name)},\"property\":{Str(name)}}}}}", false);
            }

            static string ConstraintType(Type t) =>
                t == typeof(UnityEngine.Animations.PositionConstraint) ? "Position"
                : t == typeof(UnityEngine.Animations.RotationConstraint) ? "Rotation"
                : t == typeof(UnityEngine.Animations.ScaleConstraint) ? "Scale"
                : t == typeof(UnityEngine.Animations.ParentConstraint) ? "Parent"
                : t == typeof(UnityEngine.Animations.AimConstraint) ? "Aim"
                : t == typeof(UnityEngine.Animations.LookAtConstraint) ? "LookAt" : null;

            /// Humanoid muscle clip property names: HumanTrait's names, with fingers as clips name them
            /// (`LeftHand.Index.1 Stretched`, `LeftHand.Index.Spread`).
            static HashSet<string> Muscles()
            {
                var set = new HashSet<string>(HumanTrait.MuscleName);
                foreach (var side in new[] { "Left", "Right" })
                {
                    foreach (var finger in new[] { "Thumb", "Index", "Middle", "Ring", "Little" })
                    {
                        for (int n = 1; n <= 3; n++)
                        {
                            set.Add($"{side}Hand.{finger}.{n} Stretched");
                        }
                        set.Add($"{side}Hand.{finger}.Spread");
                    }
                }
                return set;
            }

            /// Hermite keys per ADR 0039: weighted segments baked, equal key times split, values mirrored.
            string Keys(AnimationCurve curve, bool mirror)
            {
                var src = curve.keys;
                var keys = new List<(float t, float v, float i, float o)>();
                for (int k = 0; k < src.Length; k++)
                {
                    var a = src[k];
                    keys.Add((a.time, a.value, a.inTangent, a.outTangent));
                    if (k + 1 < src.Length)
                    {
                        var b = src[k + 1];
                        bool weighted = (a.weightedMode & WeightedMode.Out) != 0 || (b.weightedMode & WeightedMode.In) != 0;
                        if (weighted && b.time > a.time)
                        {
                            bakedSegments++;
                            var last = keys[keys.Count - 1];
                            keys[keys.Count - 1] = (last.t, last.v, last.i, Slope(curve, a.time, b.time, a.time));
                            Bake(curve, a.time, b.time, keys, 64);
                            // The next key's in tangent follows the baked curve too.
                            src[k + 1].inTangent = Slope(curve, a.time, b.time, b.time);
                        }
                    }
                }
                for (int k = 0; k + 1 < keys.Count; k++)
                {
                    if (keys[k + 1].t <= keys[k].t)
                    {
                        jumps++;
                        keys[k] = (keys[k].t, keys[k].v, keys[k].i, float.PositiveInfinity);
                        keys[k + 1] = (keys[k].t + 1e-4f, keys[k + 1].v, keys[k + 1].i, keys[k + 1].o);
                    }
                }
                float m = mirror ? -1 : 1;
                return string.Join(",", keys.Select(k =>
                    $"{{\"time\":{Num(k.t)},\"value\":{Num(m * k.v)},\"in_tangent\":{Tangent(m * k.i)},\"out_tangent\":{Tangent(m * k.o)}}}"));
            }

            /// The curve's slope at `t`, by differences inside [lo, hi].
            static float Slope(AnimationCurve c, float lo, float hi, float t)
            {
                float h = (hi - lo) * 1e-3f;
                float a = Mathf.Max(lo, t - h), b = Mathf.Min(hi, t + h);
                return (c.Evaluate(b) - c.Evaluate(a)) / (b - a);
            }

            /// Adds keys strictly inside (t0, t1) until Hermite segments match `Evaluate` within 1e-4 of the value
            /// range at the quarter points, splitting at midpoints, at most `budget` keys.
            static void Bake(AnimationCurve c, float t0, float t1, List<(float t, float v, float i, float o)> keys, int budget)
            {
                float v0 = c.Evaluate(t0), v1 = c.Evaluate(t1);
                float m0 = Slope(c, t0, t1, t0), m1 = Slope(c, t0, t1, t1);
                float lo = Mathf.Min(v0, v1), hi = Mathf.Max(v0, v1);
                bool ok = true;
                for (int q = 1; q <= 3; q++)
                {
                    float u = q / 4f, t = t0 + u * (t1 - t0), e = c.Evaluate(t);
                    lo = Mathf.Min(lo, e);
                    hi = Mathf.Max(hi, e);
                    float h = Hermite(v0, m0 * (t1 - t0), v1, m1 * (t1 - t0), u);
                    ok &= Mathf.Abs(h - e) <= 1e-4f * Mathf.Max(hi - lo, 1e-6f);
                }
                if (ok || budget <= 1)
                {
                    return;
                }
                float mid = (t0 + t1) / 2;
                Bake(c, t0, mid, keys, budget / 2);
                float s = Slope(c, t0, t1, mid);
                keys.Add((mid, c.Evaluate(mid), s, s));
                Bake(c, mid, t1, keys, budget / 2);
            }

            static float Hermite(float p0, float m0, float p1, float m1, float u)
            {
                float u2 = u * u, u3 = u2 * u;
                return (2 * u3 - 3 * u2 + 1) * p0 + (u3 - 2 * u2 + u) * m0 + (-2 * u3 + 3 * u2) * p1 + (u3 - u2) * m1;
            }
        }
    }
}
