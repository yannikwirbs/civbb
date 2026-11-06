// Unity's constraint components as X_constraints (ADR 0041): format `Constraint` JSON in glTF space, as the
// unity-truth recorder writes them (ADR 0018), with the X mirror of ADR 0038. At-rest values are the target's local
// transform, except for look-at: instantiating an object at runtime resets them to it (ADR 0042).
using System;
using System.Collections.Generic;
using System.Linq;
using UnityEngine;
using UnityEngine.Animations;
using static Civbb.Exporter.Json;

namespace Civbb.Exporter
{
    static class ConstraintExport
    {
        /// The extension JSON (null without constraints) and a report fragment; `extra` (VRChat constraints, ADR
        /// 0042) go after Unity's.
        public static (string json, string report) Write(Transform root, Func<Transform, int> node, IList<string> extra)
        {
            var list = new List<string>();
            int missingSources = 0, skipped = 0;
            foreach (var t in root.GetComponentsInChildren<Transform>(true))
            {
                foreach (var c in t.GetComponents<IConstraint>())
                {
                    var kind = Kind(c, node, out var axes, out var rest, out var offsets);
                    if (kind == null)
                    {
                        skipped++;
                        continue;
                    }
                    var sources = new List<ConstraintSource>();
                    c.GetSources(sources);
                    var parts = new List<string>();
                    for (int i = 0; i < sources.Count; i++)
                    {
                        int n = node(sources[i].sourceTransform);
                        if (n < 0)
                        {
                            missingSources++;
                            continue;
                        }
                        var offset = offsets != null ? offsets(i) : Transform(Vector3.zero, Quaternion.identity, Vector3.one);
                        parts.Add($"{{\"node\":{n},\"weight\":{Num(sources[i].weight)},\"offset\":{offset}}}");
                    }
                    list.Add($"{{\"kind\":{kind},\"target\":{node(t)},\"sources\":[{string.Join(",", parts)}]," +
                        $"\"weight\":{Num(c.weight)},\"active\":{Bool(c.constraintActive)},\"axes\":{axes},\"rest\":{rest}}}");
                }
            }
            var report = $"{list.Count} Unity constraints ({skipped} skipped, {missingSources} sources outside the prefab)";
            list.AddRange(extra);
            return (list.Count > 0 ? "[" + string.Join(",", list) + "]" : null, report);
        }

        static string Axes(Axis a) => $"[{Bool((a & Axis.X) != 0)},{Bool((a & Axis.Y) != 0)},{Bool((a & Axis.Z) != 0)}]";

        static string Rest(Vector3 t, Vector3 euler, Vector3 s) => Transform(t, Quaternion.Euler(euler), s);

        static string Kind(IConstraint c, Func<Transform, int> node, out string axes, out string rest,
            out Func<int, string> offsets)
        {
            offsets = null;
            var local = ((Component)c).transform;
            switch (c)
            {
                case PositionConstraint p:
                    axes = Axes(p.translationAxis);
                    rest = Transform(local.localPosition, Quaternion.identity, Vector3.one);
                    return $"{{\"Position\":{{\"offset\":{Vec(p.translationOffset)}}}}}";
                case RotationConstraint r:
                    axes = Axes(r.rotationAxis);
                    rest = Transform(Vector3.zero, local.localRotation, Vector3.one);
                    return $"{{\"Rotation\":{{\"offset\":{Euler(r.rotationOffset)}}}}}";
                case ScaleConstraint s:
                    axes = Axes(s.scalingAxis);
                    rest = Transform(Vector3.zero, Quaternion.identity, local.localScale);
                    return $"{{\"Scale\":{{\"offset\":{Nums(s.scaleOffset.x, s.scaleOffset.y, s.scaleOffset.z)}}}}}";
                case ParentConstraint p:
                    axes = Axes(p.translationAxis);
                    rest = Transform(local.localPosition, local.localRotation, Vector3.one);
                    offsets = i => Transform(p.GetTranslationOffset(i), Quaternion.Euler(p.GetRotationOffset(i)), Vector3.one);
                    return $"{{\"Parent\":{{\"rotation_axes\":{Axes(p.rotationAxis)}}}}}";
                case AimConstraint a:
                    axes = Axes(a.rotationAxis);
                    rest = Transform(Vector3.zero, local.localRotation, Vector3.one);
                    int up = node(a.worldUpObject);
                    string w;
                    switch (a.worldUpType)
                    {
                        case AimConstraint.WorldUpType.SceneUp: w = "\"SceneUp\""; break;
                        case AimConstraint.WorldUpType.Vector: w = $"{{\"Vector\":{Vec(a.worldUpVector)}}}"; break;
                        case AimConstraint.WorldUpType.ObjectUp when up >= 0: w = $"{{\"ObjectUp\":{up}}}"; break;
                        case AimConstraint.WorldUpType.ObjectRotationUp when up >= 0:
                            w = $"{{\"ObjectRotationUp\":[{up},{Vec(a.worldUpVector)}]}}"; break;
                        case AimConstraint.WorldUpType.None: w = "\"None\""; break;
                        default: w = "\"SceneUp\""; break; // an up object outside the prefab: Unity falls back to scene up
                    }
                    return $"{{\"Aim\":{{\"offset\":{Euler(a.rotationOffset)},\"aim_axis\":{Vec(a.aimVector)}," +
                        $"\"up_axis\":{Vec(a.upVector)},\"world_up\":{w}}}}}";
                case LookAtConstraint l:
                    axes = "[true,true,true]";
                    rest = Rest(Vector3.zero, l.rotationAtRest, Vector3.one);
                    int upNode = l.useUpObject ? node(l.worldUpObject) : -1;
                    return $"{{\"LookAt\":{{\"offset\":{Euler(l.rotationOffset)},\"roll\":{Num(-l.roll)}," +
                        $"\"world_up_node\":{(upNode >= 0 ? upNode.ToString() : "null")}}}}}";
                default:
                    axes = rest = null;
                    return null;
            }
        }
    }
}
