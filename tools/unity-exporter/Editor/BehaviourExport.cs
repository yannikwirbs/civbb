// VRChat state behaviours as format `Behaviour` JSON (ADR 0042), read by type name and serialized field names: parameter
// drivers, tracking control, layer and playable layer control, locomotion control. Others are counted and left out.
using System;
using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEngine;
using static Civbb.Exporter.Json;

namespace Civbb.Exporter
{
    sealed class BehaviourExport
    {
        const string Ns = "VRC.SDK3.Avatars.Components.";

        static readonly (string field, string part)[] Tracking =
        {
            ("trackingHead", "Head"), ("trackingLeftHand", "LeftHand"), ("trackingRightHand", "RightHand"),
            ("trackingHip", "Hip"), ("trackingLeftFoot", "LeftFoot"), ("trackingRightFoot", "RightFoot"),
            ("trackingLeftFingers", "LeftFingers"), ("trackingRightFingers", "RightFingers"),
            ("trackingEyes", "Eyes"), ("trackingMouth", "Mouth"),
        };

        readonly Dictionary<string, int> skipped = new Dictionary<string, int>();

        void Skip(string what) => skipped[what] = skipped.TryGetValue(what, out int n) ? n + 1 : 1;

        public string Report() => skipped.Count == 0 ? "none skipped"
            : "skipped " + string.Join(", ", skipped.Select(s => $"{s.Key} {s.Value}"));

        static string EnumName(SerializedProperty p) =>
            p.enumValueIndex >= 0 && p.enumValueIndex < p.enumNames.Length ? p.enumNames[p.enumValueIndex] : "";

        static SerializedProperty P(SerializedObject so, string path) =>
            so.FindProperty(path) ?? throw new MissingFieldException(so.targetObject.GetType().Name, path);

        static SerializedProperty P(SerializedProperty p, string path) =>
            p.FindPropertyRelative(path) ?? throw new MissingFieldException(p.propertyPath, path);

        /// One behaviour's JSON, or null when it has no format counterpart. `param` gives a parameter's index (-1
        /// unknown), `group` a playable layer's group index, `layer` a group's layer as a flattened layer index.
        public string Write(StateMachineBehaviour b, Func<string, int> param, Func<string, int> group,
            Func<string, int, int> layer)
        {
            if (b == null)
            {
                Skip("missing script");
                return null;
            }
            var type = b.GetType().FullName ?? "";
            try
            {
                var so = new SerializedObject(b);
                switch (type)
                {
                    case Ns + "VRCAvatarParameterDriver": return Driver(so, param);
                    case Ns + "VRCAnimatorTrackingControl":
                        var parts = Tracking.Select(t => (t.part, mode: EnumName(P(so, t.field))))
                            .Where(t => t.mode != "NoChange").Select(t => $"[\"{t.part}\",\"{t.mode}\"]");
                        return $"{{\"TrackingControl\":[{string.Join(",", parts)}]}}";
                    case Ns + "VRCAnimatorLayerControl":
                        int l = layer(EnumName(P(so, "playable")), P(so, "layer").intValue);
                        if (l < 0)
                        {
                            Skip("layer control of a missing layer");
                            return null;
                        }
                        return $"{{\"LayerWeight\":{{\"layer\":{l},\"goal\":{Num(P(so, "goalWeight").floatValue)}," +
                            $"\"duration\":{Num(P(so, "blendDuration").floatValue)}}}}}";
                    case Ns + "VRCPlayableLayerControl":
                        int g = group(EnumName(P(so, "layer")));
                        if (g < 0)
                        {
                            Skip("playable layer control of a missing group");
                            return null;
                        }
                        return $"{{\"GroupWeight\":{{\"group\":{g},\"goal\":{Num(P(so, "goalWeight").floatValue)}," +
                            $"\"duration\":{Num(P(so, "blendDuration").floatValue)}}}}}";
                    case Ns + "VRCAnimatorLocomotionControl":
                        return $"{{\"Locomotion\":{{\"disabled\":{Bool(P(so, "disableLocomotion").boolValue)}}}}}";
                    default:
                        Skip(b.GetType().Name);
                        return null;
                }
            }
            catch (MissingFieldException)
            {
                Skip($"{b.GetType().Name} (unknown fields)");
                return null;
            }
        }

        string Driver(SerializedObject so, Func<string, int> param)
        {
            var list = P(so, "parameters");
            var ops = new List<string>();
            for (int i = 0; i < list.arraySize; i++)
            {
                var p = list.GetArrayElementAtIndex(i);
                int dst = param(P(p, "name").stringValue);
                if (dst < 0)
                {
                    Skip("driver operation on a missing parameter");
                    continue;
                }
                float value = P(p, "value").floatValue;
                switch (EnumName(P(p, "type")))
                {
                    case "Set": ops.Add($"{{\"Set\":{{\"param\":{dst},\"value\":{Num(value)}}}}}"); break;
                    case "Add": ops.Add($"{{\"Add\":{{\"param\":{dst},\"value\":{Num(value)}}}}}"); break;
                    case "Random":
                        ops.Add($"{{\"Random\":{{\"param\":{dst},\"min\":{Num(P(p, "valueMin").floatValue)}," +
                            $"\"max\":{Num(P(p, "valueMax").floatValue)},\"chance\":{Num(P(p, "chance").floatValue)}}}}}");
                        if (P(p, "preventRepeats").boolValue)
                        {
                            Skip("random prevent repeats");
                        }
                        break;
                    case "Copy":
                        int src = param(P(p, "source").stringValue);
                        if (src < 0)
                        {
                            Skip("driver copy from a missing parameter");
                            continue;
                        }
                        var remap = P(p, "convertRange").boolValue
                            ? $"[{Nums(P(p, "sourceMin").floatValue, P(p, "sourceMax").floatValue)}," +
                              $"{Nums(P(p, "destMin").floatValue, P(p, "destMax").floatValue)}]"
                            : "null";
                        ops.Add($"{{\"Copy\":{{\"src\":{src},\"dst\":{dst},\"remap\":{remap}}}}}");
                        break;
                    default:
                        Skip("driver operation type");
                        break;
                }
            }
            return $"{{\"ParameterDriver\":{{\"local_only\":{Bool(P(so, "localOnly").boolValue)},\"ops\":[{string.Join(",", ops)}]}}}}";
        }
    }
}
