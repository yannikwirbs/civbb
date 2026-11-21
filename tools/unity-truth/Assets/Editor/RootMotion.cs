//! Records how a humanoid clip's root-motion settings turn its RootT/RootQ curves into the body pose Unity applies
//! with `applyRootMotion` off, as VRChat runs avatars (#52): one clip whose body pose moves and turns over time,
//! played in one state under every combination of the bake, keep-original, feet and loop-pose flags, with and
//! without orientation and level offsets, on `Muscles.cs`'s generated skeleton.
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

public static class RootMotion
{
    /// State times sampled in every case (the clip is 1 s long; 1.2 is past its end).
    static readonly float[] Times = { 0f, 0.3f, 0.6f, 0.9f, 1.2f };

    /// Entry point: `-executeMethod RootMotion.Record -civbbOut <abs path>/crates/anim/tests/unity/root_motion.json`.
    public static void Record()
    {
        try
        {
            var args = Environment.GetCommandLineArgs();
            var outPath = args[Array.IndexOf(args, "-civbbOut") + 1];
            var go = new GameObject("Avatar");
            var animator = go.AddComponent<Animator>();
            animator.avatar = Muscles.BuildAvatar(go, false, new[] { 0.5f, 0.5f, 0.5f, 0.5f }, out var nodes, out var byName);
            animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
            animator.applyRootMotion = false;
            var hips = byName["Hips"];
            var curves = Curves();
            var s = new StringBuilder("{\n  \"unity\": \"" + Application.unityVersion + "\",\n  \"human_scale\": " + F(animator.humanScale) + ",\n  \"nodes\": [");
            for (int i = 0; i < nodes.Count; i++)
            {
                s.Append(i == 0 ? "" : ", ").Append("{\"name\": \"").Append(nodes[i].name).Append("\", \"parent\": ")
                    .Append(i == 0 ? "null" : nodes.IndexOf(nodes[i].parent).ToString())
                    .Append(", \"position\": ").Append(V(nodes[i].localPosition)).Append(", \"rotation\": ").Append(Q(nodes[i].localRotation)).Append('}');
            }
            s.Append("],\n  \"times\": ").Append(A(Times)).Append(",\n  \"curves\": [");
            for (int c = 0; c < curves.Length; c++)
            {
                s.Append(c == 0 ? "\n    " : ",\n    ").Append("{\"property\": \"").Append(curves[c].name).Append("\", \"keys\": [")
                    .Append(string.Join(", ", curves[c].curve.keys.Select(k => "[" + F(k.time) + ", " + F(k.value) + ", " + F(k.inTangent) + ", " + F(k.outTangent) + "]")))
                    .Append("]}");
            }
            s.Append("\n  ],\n  \"cases\": [");
            int count = 0;
            void Case(AnimationClipSettings settings)
            {
                var clip = new AnimationClip();
                foreach (var (name, curve) in curves)
                    clip.SetCurve("", typeof(Animator), name, curve);
                // A muscle curve makes the clip humanoid; root curves alone do not.
                clip.SetCurve("", typeof(Animator), "Spine Front-Back", AnimationCurve.Constant(0f, 1f, 0f));
                AnimationUtility.SetAnimationClipSettings(clip, settings);
                var ctrl = new AnimatorController();
                ctrl.AddLayer("L");
                ctrl.layers[0].stateMachine.AddState("s").motion = clip;
                var graph = PlayableGraph.Create();
                AnimationPlayableOutput.Create(graph, "o", animator).SetSourcePlayable(AnimatorControllerPlayable.Create(graph, ctrl));
                var handler = new HumanPoseHandler(animator.avatar, animator.transform);
                s.Append(count++ == 0 ? "\n    " : ",\n    ").Append("{\"settings\": {")
                    .Append("\"loop_time\": ").Append(B(settings.loopTime))
                    .Append(", \"loop_blend\": ").Append(B(settings.loopBlend))
                    .Append(", \"cycle_offset\": ").Append(F(settings.cycleOffset))
                    .Append(", \"bake_orientation\": ").Append(B(settings.loopBlendOrientation))
                    .Append(", \"bake_position_y\": ").Append(B(settings.loopBlendPositionY))
                    .Append(", \"bake_position_xz\": ").Append(B(settings.loopBlendPositionXZ))
                    .Append(", \"original_orientation\": ").Append(B(settings.keepOriginalOrientation))
                    .Append(", \"original_position_y\": ").Append(B(settings.keepOriginalPositionY))
                    .Append(", \"original_position_xz\": ").Append(B(settings.keepOriginalPositionXZ))
                    .Append(", \"height_from_feet\": ").Append(B(settings.heightFromFeet))
                    .Append(", \"orientation_offset_y\": ").Append(F(settings.orientationOffsetY))
                    .Append(", \"level\": ").Append(F(settings.level))
                    .Append("}, \"samples\": [");
                float last = 0f;
                for (int i = 0; i < Times.Length; i++)
                {
                    graph.Evaluate(Times[i] - last);
                    last = Times[i];
                    var pose = new HumanPose();
                    handler.GetHumanPose(ref pose);
                    s.Append(i == 0 ? "" : ", ").Append("{\"hips\": [").Append(V(hips.position)).Append(", ").Append(Q(hips.rotation))
                        .Append("], \"body\": [").Append(V(pose.bodyPosition)).Append(", ").Append(Q(pose.bodyRotation)).Append("]}");
                }
                s.Append("]}");
                handler.Dispose();
                graph.Destroy();
            }
            // Every combination of the eight flags, without and then with orientation and level offsets.
            foreach (var offsets in new[] { false, true })
            {
                for (int bits = 0; bits < 256; bits++)
                {
                    Case(new AnimationClipSettings
                    {
                        loopTime = true,
                        stopTime = 1f,
                        loopBlendOrientation = (bits & 1) != 0,
                        loopBlendPositionY = (bits & 2) != 0,
                        loopBlendPositionXZ = (bits & 4) != 0,
                        keepOriginalOrientation = (bits & 8) != 0,
                        keepOriginalPositionY = (bits & 16) != 0,
                        keepOriginalPositionXZ = (bits & 32) != 0,
                        heightFromFeet = (bits & 64) != 0,
                        loopBlend = (bits & 128) != 0,
                        orientationOffsetY = offsets ? 40f : 0f,
                        level = offsets ? 0.07f : 0f,
                    });
                }
            }
            // Not looping, and a cycle offset, for the fully baked and the fully unbaked clip.
            foreach (var baked in new[] { false, true })
            {
                Case(new AnimationClipSettings
                {
                    loopTime = false, stopTime = 1f,
                    loopBlendOrientation = baked, loopBlendPositionY = baked, loopBlendPositionXZ = baked,
                });
                Case(new AnimationClipSettings
                {
                    loopTime = true, stopTime = 1f, cycleOffset = 0.3f,
                    loopBlendOrientation = baked, loopBlendPositionY = baked, loopBlendPositionXZ = baked,
                });
            }
            File.WriteAllText(outPath, s.Append("\n  ]\n}\n").ToString());
            Debug.Log($"CIVBB root motion recorded ({count} cases) to {outPath}");
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

    /// RootT and RootQ curves keyed every 0.25 s: the body moves sideways and forward, bobs, turns 90 degrees about
    /// Y and tilts, so every component and the start and end of the clip differ.
    static (string name, AnimationCurve curve)[] Curves()
    {
        var times = new[] { 0f, 0.25f, 0.5f, 0.75f, 1f };
        var pos = times.Select(t => new Vector3(0.1f + 0.3f * t, 0.95f + 0.08f * Mathf.Sin(2f * Mathf.PI * t) + 0.04f * t, -0.1f + 0.25f * t * t)).ToArray();
        var rot = times.Select(t => Quaternion.Euler(12f * Mathf.Sin(Mathf.PI * t), 20f + 90f * t, -8f * t)).ToArray();
        AnimationCurve Curve(Func<int, float> value)
        {
            // Explicit tangents (central differences) so a replay needs no tangent mode.
            var keys = new Keyframe[times.Length];
            for (int i = 0; i < times.Length; i++)
            {
                int a = Math.Max(i - 1, 0), b = Math.Min(i + 1, times.Length - 1);
                float slope = (value(b) - value(a)) / (times[b] - times[a]);
                keys[i] = new Keyframe(times[i], value(i), slope, slope);
            }
            return new AnimationCurve(keys);
        }
        return new[]
        {
            ("RootT.x", Curve(i => pos[i].x)), ("RootT.y", Curve(i => pos[i].y)), ("RootT.z", Curve(i => pos[i].z)),
            ("RootQ.x", Curve(i => rot[i].x)), ("RootQ.y", Curve(i => rot[i].y)), ("RootQ.z", Curve(i => rot[i].z)),
            ("RootQ.w", Curve(i => rot[i].w)),
        };
    }

    static string F(float x) => x.ToString("R", CultureInfo.InvariantCulture);
    static string B(bool b) => b ? "true" : "false";
    static string V(Vector3 v) => "[" + F(v.x) + ", " + F(v.y) + ", " + F(v.z) + "]";
    static string Q(Quaternion q) => "[" + F(q.x) + ", " + F(q.y) + ", " + F(q.z) + ", " + F(q.w) + "]";
    static string A(float[] a) => "[" + string.Join(", ", a.Select(F)) + "]";
}
