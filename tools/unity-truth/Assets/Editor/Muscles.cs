// Records Unity's humanoid muscle space as ground truth for civbb-anim (ADR 0013, 0014): the muscle table and clip
// property names, each test skeleton's per-bone axes, and HumanPoseHandler poses for single muscles and random mixes.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Reflection;
using System.Text;
using UnityEditor;
using UnityEngine;

public static class Muscles
{
    const BindingFlags Internal = BindingFlags.Instance | BindingFlags.NonPublic | BindingFlags.Public;

    // A T-posed humanoid in Unity coordinates (metres, facing +Z, the character's left at -X): name, parent, local
    // position. Right-side bones mirror the left ones.
    static readonly (string bone, string parent, Vector3 pos)[] Left =
    {
        ("LeftUpperLeg", "Hips", new Vector3(-0.09f, -0.05f, 0f)),
        ("LeftLowerLeg", "LeftUpperLeg", new Vector3(0f, -0.42f, 0.01f)),
        ("LeftFoot", "LeftLowerLeg", new Vector3(0f, -0.42f, -0.02f)),
        ("LeftToes", "LeftFoot", new Vector3(0f, -0.06f, 0.12f)),
        ("LeftShoulder", "UpperChest", new Vector3(-0.04f, 0.1f, 0f)),
        ("LeftUpperArm", "LeftShoulder", new Vector3(-0.11f, 0f, 0f)),
        ("LeftLowerArm", "LeftUpperArm", new Vector3(-0.27f, 0f, 0f)),
        ("LeftHand", "LeftLowerArm", new Vector3(-0.25f, 0f, 0f)),
        ("LeftThumbProximal", "LeftHand", new Vector3(-0.02f, -0.01f, 0.03f)),
        ("LeftThumbIntermediate", "LeftThumbProximal", new Vector3(-0.025f, 0f, 0.02f)),
        ("LeftThumbDistal", "LeftThumbIntermediate", new Vector3(-0.02f, 0f, 0.012f)),
        ("LeftIndexProximal", "LeftHand", new Vector3(-0.08f, 0f, 0.03f)),
        ("LeftIndexIntermediate", "LeftIndexProximal", new Vector3(-0.04f, 0f, 0f)),
        ("LeftIndexDistal", "LeftIndexIntermediate", new Vector3(-0.025f, 0f, 0f)),
        ("LeftMiddleProximal", "LeftHand", new Vector3(-0.085f, 0f, 0.01f)),
        ("LeftMiddleIntermediate", "LeftMiddleProximal", new Vector3(-0.045f, 0f, 0f)),
        ("LeftMiddleDistal", "LeftMiddleIntermediate", new Vector3(-0.028f, 0f, 0f)),
        ("LeftRingProximal", "LeftHand", new Vector3(-0.08f, 0f, -0.01f)),
        ("LeftRingIntermediate", "LeftRingProximal", new Vector3(-0.04f, 0f, 0f)),
        ("LeftRingDistal", "LeftRingIntermediate", new Vector3(-0.025f, 0f, 0f)),
        ("LeftLittleProximal", "LeftHand", new Vector3(-0.075f, 0f, -0.03f)),
        ("LeftLittleIntermediate", "LeftLittleProximal", new Vector3(-0.03f, 0f, 0f)),
        ("LeftLittleDistal", "LeftLittleIntermediate", new Vector3(-0.02f, 0f, 0f)),
        ("LeftEye", "Head", new Vector3(-0.03f, 0.06f, 0.08f)),
    };

    static readonly (string bone, string parent, Vector3 pos)[] Center =
    {
        ("Hips", null, new Vector3(0f, 1f, 0f)),
        ("Spine", "Hips", new Vector3(0f, 0.1f, 0f)),
        ("Chest", "Spine", new Vector3(0f, 0.15f, 0.01f)),
        ("UpperChest", "Chest", new Vector3(0f, 0.15f, 0f)),
        ("Neck", "UpperChest", new Vector3(0f, 0.15f, -0.01f)),
        ("Head", "Neck", new Vector3(0f, 0.1f, 0f)),
        ("Jaw", "Head", new Vector3(0f, -0.02f, 0.03f)),
    };

    /// Entry point: `-executeMethod Muscles.Record -civbbOut <dir>`.
    public static void Record()
    {
        var args = Environment.GetCommandLineArgs();
        var outDir = args[Array.IndexOf(args, "-civbbOut") + 1];
        Directory.CreateDirectory(outDir);
        File.WriteAllText(Path.Combine(outDir, "muscles.json"), Table());
        File.WriteAllText(Path.Combine(outDir, "tpose.json"), Skeleton(false, new[] { 0.5f, 0.5f, 0.5f, 0.5f }));
        File.WriteAllText(Path.Combine(outDir, "rolled.json"), Skeleton(true, new[] { 0.5f, 0.5f, 0.5f, 0.5f }));
        File.WriteAllText(Path.Combine(outDir, "twist.json"), Skeleton(false, new[] { 0.2f, 0.3f, 0.7f, 0.9f }));
        Debug.Log("CIVBB muscles recorded to " + outDir);
    }

    static string Table()
    {
        var s = new StringBuilder("{\n  \"unity\": \"" + Application.unityVersion + "\",\n  \"bones\": [");
        for (int b = 0; b < HumanTrait.BoneCount; b++)
            s.Append(b == 0 ? "\n    " : ",\n    ").Append('"').Append(HumanTrait.BoneName[b]).Append('"');
        s.Append("\n  ],\n  \"muscles\": [");
        for (int m = 0; m < HumanTrait.MuscleCount; m++)
        {
            int bone = HumanTrait.BoneFromMuscle(m);
            int dof = -1;
            for (int d = 0; d < 3; d++)
                if (HumanTrait.MuscleFromBone(bone, d) == m)
                    dof = d;
            s.Append(m == 0 ? "\n    " : ",\n    ")
                .Append("{\"name\": \"").Append(HumanTrait.MuscleName[m])
                .Append("\", \"bone\": ").Append(bone)
                .Append(", \"dof\": ").Append(dof)
                .Append(", \"min\": ").Append(F(HumanTrait.GetMuscleDefaultMin(m)))
                .Append(", \"max\": ").Append(F(HumanTrait.GetMuscleDefaultMax(m))).Append('}');
        }

        // The Animator properties a clip can bind on a humanoid: muscles under their clip names, root and IK goals.
        s.Append("\n  ],\n  \"clip_properties\": [");
        var go = new GameObject("Avatar");
        var animator = go.AddComponent<Animator>();
        animator.avatar = BuildAvatar(go, false, new[] { 0.5f, 0.5f, 0.5f, 0.5f }, out _, out _);
        bool first = true;
        foreach (var b in AnimationUtility.GetAnimatableBindings(go, go))
        {
            if (b.type != typeof(Animator) || b.path != "")
                continue;
            s.Append(first ? "\n    " : ",\n    ").Append('"').Append(b.propertyName).Append('"');
            first = false;
        }
        UnityEngine.Object.DestroyImmediate(go);
        return s.Append("\n  ]\n}\n").ToString();
    }

    // Builds the test skeleton under `root` and its humanoid avatar. `twist` is upper arm, lower arm, upper leg,
    // lower leg twist.
    static Avatar BuildAvatar(GameObject root, bool rolled, float[] twist, out List<Transform> nodes, out Dictionary<string, Transform> byName)
    {
        var all = new List<Transform> { root.transform };
        var named = new Dictionary<string, Transform>();
        void Add(string name, string parent, Vector3 pos)
        {
            var t = new GameObject(name).transform;
            t.SetParent(parent == null ? root.transform : named[parent], false);
            t.localPosition = pos;
            named[name] = t;
            all.Add(t);
        }
        foreach (var (bone, parent, pos) in Center)
            Add(bone, parent, pos);
        foreach (var (bone, parent, pos) in Left)
            Add(bone, parent, pos);
        foreach (var (bone, parent, pos) in Left)
            Add(bone.Replace("Left", "Right"), parent.Replace("Left", "Right"), new Vector3(-pos.x, pos.y, pos.z));
        if (rolled)
        {
            // Same world positions, arbitrary bone orientations, as in rigs exported from DCC tools.
            var rng = new System.Random(7);
            var world = new Dictionary<Transform, Vector3>();
            foreach (var t in all)
                world[t] = t.position;
            foreach (var t in all)
            {
                if (t == root.transform)
                    continue;
                t.rotation = Quaternion.Euler(R(rng) * 180f, R(rng) * 180f, R(rng) * 180f);
                t.position = world[t];
            }
        }

        var human = new List<HumanBone>();
        for (int b = 0; b < HumanTrait.BoneCount; b++)
        {
            var name = HumanTrait.BoneName[b].Replace(" ", "");
            if (named.ContainsKey(name))
                human.Add(new HumanBone { humanName = HumanTrait.BoneName[b], boneName = name, limit = new HumanLimit { useDefaultValues = true } });
        }
        var skeleton = new List<SkeletonBone>();
        foreach (var t in all)
            skeleton.Add(new SkeletonBone { name = t.name, position = t.localPosition, rotation = t.localRotation, scale = t.localScale });
        var desc = new HumanDescription
        {
            human = human.ToArray(),
            skeleton = skeleton.ToArray(),
            upperArmTwist = twist[0],
            lowerArmTwist = twist[1],
            upperLegTwist = twist[2],
            lowerLegTwist = twist[3],
            armStretch = 0.05f,
            legStretch = 0.05f,
            feetSpacing = 0f,
            hasTranslationDoF = false,
        };
        var avatar = AvatarBuilder.BuildHumanAvatar(root, desc);
        if (!avatar.isValid || !avatar.isHuman)
            throw new Exception("avatar invalid");
        nodes = all;
        byName = named;
        return avatar;
    }

    static string Skeleton(bool rolled, float[] twist)
    {
        var root = new GameObject("Avatar");
        var avatar = BuildAvatar(root, rolled, twist, out var nodes, out var byName);

        var s = new StringBuilder("{\n  \"twist\": " + A(twist) + ",\n  \"nodes\": [");
        for (int i = 0; i < nodes.Count; i++)
        {
            var t = nodes[i];
            s.Append(i == 0 ? "\n    " : ",\n    ")
                .Append("{\"name\": \"").Append(t.name).Append("\", \"parent\": ")
                .Append(t == root.transform ? "null" : nodes.IndexOf(t.parent).ToString())
                .Append(", \"position\": ").Append(V(t.localPosition))
                .Append(", \"rotation\": ").Append(Q(t.localRotation)).Append('}');
        }

        // Per-bone axes, from Avatar's internal accessors (the same ones Unity's Avatar Muscle inspector uses).
        var pre = typeof(Avatar).GetMethod("GetPreRotation", Internal);
        var post = typeof(Avatar).GetMethod("GetPostRotation", Internal);
        var sign = typeof(Avatar).GetMethod("GetLimitSign", Internal);
        s.Append("\n  ],\n  \"axes\": [");
        var bones = new List<Transform>();
        for (int b = 0; b < HumanTrait.BoneCount; b++)
        {
            if (!byName.TryGetValue(HumanTrait.BoneName[b].Replace(" ", ""), out var t))
                continue;
            var o = new object[] { b };
            s.Append(bones.Count == 0 ? "\n    " : ",\n    ")
                .Append("{\"bone\": ").Append(b).Append(", \"node\": ").Append(nodes.IndexOf(t))
                .Append(", \"pre\": ").Append(Q((Quaternion)pre.Invoke(avatar, o)))
                .Append(", \"post\": ").Append(Q((Quaternion)post.Invoke(avatar, o)))
                .Append(", \"sign\": ").Append(V((Vector3)sign.Invoke(avatar, o))).Append('}');
            bones.Add(t);
        }

        var handler = new HumanPoseHandler(avatar, root.transform);
        var pose = new HumanPose();
        handler.GetHumanPose(ref pose);
        s.Append("\n  ],\n  \"rest\": {\"body_position\": ").Append(V(pose.bodyPosition))
            .Append(", \"body_rotation\": ").Append(Q(pose.bodyRotation))
            .Append(", \"muscles\": ").Append(A(pose.muscles)).Append('}');

        // Poses: all muscles 0 with every bone's local rotation, then every muscle alone at four values and random
        // mixes, listing only bones whose rotation differs from the first pose as [axes index, rotation].
        var cases = new List<float[]> { new float[HumanTrait.MuscleCount] };
        foreach (var v in new[] { -1f, -0.5f, 0.5f, 1f })
            for (int m = 0; m < HumanTrait.MuscleCount; m++)
            {
                var mu = new float[HumanTrait.MuscleCount];
                mu[m] = v;
                cases.Add(mu);
            }
        var mix = new System.Random(11);
        for (int c = 0; c < 24; c++)
        {
            var mu = new float[HumanTrait.MuscleCount];
            for (int m = 0; m < mu.Length; m++)
                mu[m] = R(mix);
            cases.Add(mu);
        }
        s.Append(",\n  \"poses\": [");
        var zero = new Quaternion[bones.Count];
        for (int c = 0; c < cases.Count; c++)
        {
            pose.bodyPosition = new Vector3(0f, 1f, 0f);
            pose.bodyRotation = Quaternion.identity;
            pose.muscles = cases[c];
            handler.SetHumanPose(ref pose);
            s.Append(c == 0 ? "\n    " : ",\n    ").Append("{\"muscles\": ").Append(Sparse(cases[c])).Append(", \"local\": [");
            bool first = true;
            for (int i = 0; i < bones.Count; i++)
            {
                var q = bones[i].localRotation;
                if (c == 0)
                    zero[i] = q;
                else if (Mathf.Abs(Quaternion.Dot(q, zero[i])) > 1f - 1e-7f)
                    continue;
                s.Append(first ? "" : ", ").Append('[').Append(i).Append(", ").Append(Q(q)).Append(']');
                first = false;
            }
            s.Append("], \"hips_position\": ").Append(V(bones[0].localPosition)).Append('}');
        }
        handler.Dispose();
        UnityEngine.Object.DestroyImmediate(root);
        return s.Append("\n  ]\n}\n").ToString();
    }

    static float R(System.Random r) => (float)(r.NextDouble() * 2.0 - 1.0);

    static string F(float f) => f.ToString("R", CultureInfo.InvariantCulture);

    static string V(Vector3 v) => "[" + F(v.x) + ", " + F(v.y) + ", " + F(v.z) + "]";

    static string Q(Quaternion q) => "[" + F(q.x) + ", " + F(q.y) + ", " + F(q.z) + ", " + F(q.w) + "]";

    static string A(float[] a)
    {
        var s = new StringBuilder("[");
        for (int i = 0; i < a.Length; i++)
            s.Append(i == 0 ? "" : ", ").Append(F(a[i]));
        return s.Append(']').ToString();
    }

    // Non-zero muscles as [[index, value], ...].
    static string Sparse(float[] a)
    {
        var s = new StringBuilder("[");
        bool first = true;
        for (int i = 0; i < a.Length; i++)
        {
            if (a[i] == 0f)
                continue;
            s.Append(first ? "" : ", ").Append('[').Append(i).Append(", ").Append(F(a[i])).Append(']');
            first = false;
        }
        return s.Append(']').ToString();
    }
}
