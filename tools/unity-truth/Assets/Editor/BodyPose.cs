// Records Unity's humanoid body pose (#52) on the generated test skeletons of `Muscles.cs`: HumanPoseHandler poses
// with muscles, body position and body rotation set, where every node lands, and GetHumanPose of each result.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using UnityEditor;
using UnityEngine;

public static class BodyPose
{
    /// Entry point: `-executeMethod BodyPose.Record -civbbOut <absolute json path>`.
    public static void Record()
    {
        try
        {
            var args = Environment.GetCommandLineArgs();
            var outPath = args[Array.IndexOf(args, "-civbbOut") + 1];
            var s = new StringBuilder("{\n  \"unity\": \"" + Application.unityVersion + "\",\n  \"skeletons\": [");
            bool first = true;
            // Plain, rolled, and plain with optional bones left unmapped (kept as nodes) as corpus avatars do: no
            // upper chest (9 of 13 exports), and no upper chest or toes (4).
            var noChest = new[] { "UpperChest" };
            var noChestToes = new[] { "UpperChest", "LeftToes", "RightToes" };
            foreach (var (rolled, unmapped) in new[] { (false, new string[0]), (true, new string[0]), (false, noChest), (false, noChestToes) })
            {
                s.Append(first ? "\n    " : ",\n    ").Append(Skeleton(rolled, unmapped));
                first = false;
            }
            File.WriteAllText(outPath, s.Append("\n  ]\n}\n").ToString());
            Debug.Log("CIVBB body pose recorded to " + outPath);
        }
        catch (Exception e)
        {
            Debug.LogError($"CIVBB failed: {e}");
        }
        EditorApplication.Exit(0);
    }

    static string Skeleton(bool rolled, string[] unmapped)
    {
        var root = new GameObject("Avatar");
        var avatar = Muscles.BuildAvatar(root, rolled, new[] { 0.5f, 0.5f, 0.5f, 0.5f }, out var nodes, out _, unmapped);
        var animator = root.AddComponent<Animator>();
        animator.avatar = avatar;
        var handler = new HumanPoseHandler(avatar, root.transform);
        var s = new StringBuilder("{\"rolled\": " + (rolled ? "true" : "false") + ", \"unmapped\": [" +
            string.Join(", ", unmapped.Select(u => "\"" + u + "\"")) + "], \"human_scale\": " + F(animator.humanScale) + ", \"nodes\": [");
        for (int i = 0; i < nodes.Count; i++)
        {
            s.Append(i == 0 ? "" : ", ").Append("{\"name\": \"").Append(nodes[i].name).Append("\", \"parent\": ")
                .Append(i == 0 ? "null" : nodes.IndexOf(nodes[i].parent).ToString())
                .Append(", \"position\": ").Append(V(nodes[i].localPosition)).Append(", \"rotation\": ").Append(Q(nodes[i].localRotation)).Append('}');
        }
        s.Append("],\n      \"cases\": [");
        var rng = new System.Random(52);
        var pose = new HumanPose();
        handler.GetHumanPose(ref pose);
        var rest = pose.muscles.ToArray();
        int count = 0;
        void Case(string name, float[] muscles, Vector3 position, Quaternion rotation)
        {
            pose.muscles = muscles;
            pose.bodyPosition = position;
            pose.bodyRotation = rotation;
            handler.SetHumanPose(ref pose);
            var got = new HumanPose();
            handler.GetHumanPose(ref got);
            s.Append(count++ == 0 ? "\n        " : ",\n        ").Append("{\"name\": \"").Append(name).Append("\", \"muscles\": ").Append(A(muscles))
                .Append(", \"body_position\": ").Append(V(position)).Append(", \"body_rotation\": ").Append(Q(rotation))
                .Append(", \"world\": [").Append(string.Join(", ", nodes.Select(t => "[" + V(t.position) + ", " + Q(t.rotation) + "]"))).Append(']')
                .Append(", \"got_position\": ").Append(V(got.bodyPosition)).Append(", \"got_rotation\": ").Append(Q(got.bodyRotation)).Append('}');
        }
        float R() => (float)(rng.NextDouble() * 2.0 - 1.0);
        Case("rest", rest, new Vector3(0f, 1f, 0f), Quaternion.identity);
        Case("zero", new float[HumanTrait.MuscleCount], new Vector3(0f, 1f, 0f), Quaternion.identity);
        Case("zero_moved", new float[HumanTrait.MuscleCount], new Vector3(0.3f, 0.8f, -0.2f), Quaternion.identity);
        Case("zero_turned", new float[HumanTrait.MuscleCount], new Vector3(0f, 1f, 0f), Quaternion.Euler(20f, 35f, -10f));
        // One muscle group at a time at 0.8 (spine, legs, arms), then random mixes, then random mixes moved and turned.
        foreach (var group in new[] { "Spine", "Chest", "Leg", "Arm", "Neck", "Shoulder" })
        {
            var m = new float[HumanTrait.MuscleCount];
            for (int k = 0; k < m.Length; k++)
                if (HumanTrait.MuscleName[k].Contains(group))
                    m[k] = 0.8f * (k % 2 == 0 ? 1f : -1f);
            Case("group_" + group.ToLowerInvariant(), m, new Vector3(0f, 1f, 0f), Quaternion.identity);
        }
        for (int i = 0; i < 24; i++)
        {
            var m = Enumerable.Range(0, HumanTrait.MuscleCount).Select(_ => 0.7f * R()).ToArray();
            bool moved = i >= 12;
            Case("mix_" + i, m, moved ? new Vector3(0.4f * R(), 1f + 0.3f * R(), 0.4f * R()) : new Vector3(0f, 1f, 0f),
                moved ? Quaternion.Euler(40f * R(), 180f * R(), 40f * R()) : Quaternion.identity);
        }
        UnityEngine.Object.DestroyImmediate(root);
        return s.Append("\n      ]}").ToString();
    }

    static string F(float x) => x.ToString("R", CultureInfo.InvariantCulture);
    static string V(Vector3 v) => "[" + F(v.x) + ", " + F(v.y) + ", " + F(v.z) + "]";
    static string Q(Quaternion q) => "[" + F(q.x) + ", " + F(q.y) + ", " + F(q.z) + ", " + F(q.w) + "]";
    static string A(float[] a) => "[" + string.Join(", ", a.Select(F)) + "]";
}
