//! Records the muscle frames Unity's `AvatarBuilder` derives from a skeleton (#53): `Muscles.cs`'s generated humanoid
//! reshaped one way at a time (sloped shoulders, A-pose, arms forward, curved spine, legs apart, bent hands and
//! fingers, tilted hips, rolled bone axes) and each mapped bone's pre and post rotation and limit sign.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using UnityEditor;
using UnityEngine;

public static class MuscleFrames
{
    const BindingFlags Internal = BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public;

    /// Entry point: `-executeMethod MuscleFrames.Record -civbbOut <abs path>/crates/anim/tests/unity/muscle_frames.json`.
    public static void Record()
    {
        try
        {
            var args = Environment.GetCommandLineArgs();
            var outPath = args[Array.IndexOf(args, "-civbbOut") + 1];
            // Each variant turns bones about their joint (Unity axes: +X right of the avatar is the character's left
            // at -X, +Y up, +Z forward); the right side mirrors the left. Rotations are then baked into positions,
            // so every node keeps an identity rest rotation, unless `rolled`.
            var variants = new (string name, bool rolled, (string bone, Vector3 axis, float degrees)[] turns)[]
            {
                ("tpose", false, new (string, Vector3, float)[0]),
                ("rolled", true, new (string, Vector3, float)[0]),
                ("shoulders_sloped", false, new[] { ("LeftShoulder", Vector3.forward, 20f) }),
                ("apose", false, new[] { ("LeftUpperArm", Vector3.forward, 35f) }),
                ("arms_forward", false, new[] { ("LeftUpperArm", Vector3.up, -25f) }),
                ("elbows_bent", false, new[] { ("LeftLowerArm", Vector3.up, -30f) }),
                ("spine_curved", false, new[] { ("Chest", Vector3.right, 15f), ("Neck", Vector3.right, -20f) }),
                ("legs_apart", false, new[] { ("LeftUpperLeg", Vector3.forward, -12f), ("LeftLowerLeg", Vector3.right, -10f) }),
                ("hands_bent", false, new[] { ("LeftHand", Vector3.forward, 20f), ("LeftHand", Vector3.up, 15f) }),
                ("fingers_curled", false, new[] { ("LeftIndexProximal", Vector3.forward, 30f), ("LeftThumbProximal", Vector3.up, 25f), ("LeftLittleIntermediate", Vector3.forward, 40f) }),
                ("feet_turned", false, new[] { ("LeftFoot", Vector3.up, 20f), ("LeftToes", Vector3.right, 15f) }),
                ("hips_tilted", false, new[] { ("Hips", Vector3.right, 12f) }),
                ("head_turned", false, new[] { ("Head", Vector3.up, 25f), ("Head", Vector3.right, 10f) }),
                // The hand's geometry (#95): `~` moves a bone and everything below it by axis x value (metres).
                ("thumb_root_low", false, new[] { ("~LeftThumbProximal", Vector3.down, 0.02f) }),
                ("thumb_root_wrist", false, new[] { ("~LeftThumbProximal", Vector3.right, 0.03f) }),
                ("thumb_root_forward", false, new[] { ("~LeftThumbProximal", Vector3.forward, 0.02f) }),
                ("thumb_turned_down", false, new[] { ("LeftThumbProximal", Vector3.forward, 25f) }),
                ("thumb_turned_out", false, new[] { ("LeftThumbProximal", Vector3.up, -30f) }),
                ("thumb_twisted", false, new[] { ("LeftThumbProximal", Vector3.right, 40f) }),
                ("fingers_fanned", false, new[] { ("LeftIndexProximal", Vector3.up, 15f), ("LeftLittleProximal", Vector3.up, -15f) }),
                ("index_forward", false, new[] { ("~LeftIndexProximal", Vector3.forward, 0.02f) }),
                ("little_low", false, new[] { ("~LeftLittleProximal", Vector3.down, 0.015f) }),
                ("middle_high", false, new[] { ("~LeftMiddleProximal", Vector3.up, 0.01f) }),
                ("hand_rolled", false, new[] { ("LeftHand", Vector3.right, 30f) }),
            };
            var s = new StringBuilder("{\n  \"unity\": \"" + Application.unityVersion + "\",\n  \"variants\": [");
            for (int v = 0; v < variants.Length; v++)
            {
                s.Append(v == 0 ? "\n    " : ",\n    ").Append(Variant(variants[v].name, variants[v].rolled, variants[v].turns));
            }
            File.WriteAllText(outPath, s.Append("\n  ]\n}\n").ToString());
            Debug.Log($"CIVBB muscle frames recorded ({variants.Length} variants) to {outPath}");
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

    static string Variant(string name, bool rolled, (string bone, Vector3 axis, float degrees)[] turns)
    {
        var root = new GameObject("Avatar");
        void Tweak(Dictionary<string, Transform> named)
        {
            foreach (var (bone, axis, degrees) in turns)
            {
                bool move = bone.StartsWith("~");
                var name = move ? bone.Substring(1) : bone;
                foreach (var side in name.StartsWith("Left") ? new[] { "Left", "Right" } : new[] { "" })
                {
                    var t = named[side == "" ? name : name.Replace("Left", side)];
                    if (move)
                    {
                        var by = axis * degrees;
                        if (side == "Right")
                            by.x = -by.x;
                        t.position += by;
                        continue;
                    }
                    // Mirroring X keeps rotations about X and negates those about Y and Z.
                    float sign = side == "Right" && axis.x == 0f ? -1f : 1f;
                    var below = t.GetComponentsInChildren<Transform>();
                    var world = below.ToDictionary(x => x, x => x.position);
                    var turn = Quaternion.AngleAxis(sign * degrees, axis);
                    foreach (var x in below)
                        world[x] = t.position + turn * (world[x] - t.position);
                    foreach (var x in below)
                        x.position = world[x];
                }
            }
        }
        var avatar = Muscles.BuildAvatar(root, rolled, new[] { 0.5f, 0.5f, 0.5f, 0.5f }, out var nodes, out var byName, null, Tweak);
        var s = new StringBuilder("{\"name\": \"" + name + "\", \"nodes\": [");
        for (int i = 0; i < nodes.Count; i++)
        {
            var t = nodes[i];
            s.Append(i == 0 ? "" : ", ").Append("{\"name\": \"").Append(t.name).Append("\", \"parent\": ")
                .Append(i == 0 ? "null" : nodes.IndexOf(t.parent).ToString())
                .Append(", \"position\": ").Append(V(t.localPosition)).Append(", \"rotation\": ").Append(Q(t.localRotation)).Append('}');
        }
        var pre = typeof(Avatar).GetMethod("GetPreRotation", Internal);
        var post = typeof(Avatar).GetMethod("GetPostRotation", Internal);
        var sign = typeof(Avatar).GetMethod("GetLimitSign", Internal);
        s.Append("],\n      \"frames\": [");
        bool first = true;
        for (int b = 0; b < HumanTrait.BoneCount; b++)
        {
            var bone = HumanTrait.BoneName[b].Replace(" ", "");
            if (!byName.TryGetValue(bone, out var t))
                continue;
            var o = new object[] { b };
            s.Append(first ? "\n        " : ",\n        ").Append("{\"bone\": \"").Append(bone).Append("\", \"node\": ").Append(nodes.IndexOf(t))
                .Append(", \"pre\": ").Append(Q((Quaternion)pre.Invoke(avatar, o)))
                .Append(", \"post\": ").Append(Q((Quaternion)post.Invoke(avatar, o)))
                .Append(", \"sign\": ").Append(V((Vector3)sign.Invoke(avatar, o))).Append('}');
            first = false;
        }
        UnityEngine.Object.DestroyImmediate(root);
        return s.Append("\n      ]}").ToString();
    }

    static string F(float x) => x.ToString("R", CultureInfo.InvariantCulture);
    static string V(Vector3 v) => "[" + F(v.x) + ", " + F(v.y) + ", " + F(v.z) + "]";
    static string Q(Quaternion q) => "[" + F(q.x) + ", " + F(q.y) + ", " + F(q.z) + ", " + F(q.w) + "]";
}
