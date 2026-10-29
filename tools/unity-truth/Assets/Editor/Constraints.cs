// Records Unity's transform constraints as ground truth for civbb-anim (ADR 0013, 0018). Each case is a small
// hierarchy with constraints; the JSON holds the input local pose, the constraints in civbb-format's serde shape and
// the local pose Unity's constraints produce. Constraints only run in play mode, so this enters play mode (without
// domain or scene reload), resets the inputs every frame as an animator would, and records after a few frames.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text;
using UnityEditor;
using UnityEngine;
using UnityEngine.Animations;

public static class Constraints
{
    class Case
    {
        public string Name;
        public readonly List<Transform> Nodes = new List<Transform>();
        public readonly List<(Vector3 p, Quaternion r, Vector3 s)> Input = new List<(Vector3, Quaternion, Vector3)>();
        public readonly List<string> Json = new List<string>();
        public string[] Last;

        public Transform Node(string name, Transform parent, Vector3 p, Vector3 euler, Vector3 s)
        {
            var t = new GameObject(Name + "/" + name).transform;
            t.SetParent(parent, false);
            t.localPosition = p;
            t.localRotation = Quaternion.Euler(euler);
            t.localScale = s;
            Nodes.Add(t);
            Input.Add((p, t.localRotation, s));
            return t;
        }

        public int Index(Transform t) => Nodes.IndexOf(t);
    }

    static readonly List<Case> cases = new List<Case>();
    static string outDir;
    static int frames;

    /// Entry point: `-executeMethod Constraints.Record -civbbOut <dir>` (without `-quit`; it exits by itself).
    public static void Record()
    {
        var args = Environment.GetCommandLineArgs();
        outDir = args[Array.IndexOf(args, "-civbbOut") + 1];
        Build();
        EditorSettings.enterPlayModeOptionsEnabled = true;
        EditorSettings.enterPlayModeOptions =
            EnterPlayModeOptions.DisableDomainReload | EnterPlayModeOptions.DisableSceneReload;
        EditorApplication.update += Tick;
        EditorApplication.EnterPlaymode();
    }

    // Each editor tick: read what the last player-loop frame's constraints produced, then reset the inputs.
    static void Tick()
    {
        if (!EditorApplication.isPlaying)
            return;
        frames++;
        var stable = true;
        foreach (var c in cases)
        {
            var now = new string[c.Nodes.Count];
            for (var i = 0; i < c.Nodes.Count; i++)
            {
                var t = c.Nodes[i];
                now[i] = "{\"translation\": " + V(t.localPosition) + ", \"rotation\": " + Q(t.localRotation) +
                         ", \"scale\": " + V(t.localScale) + "}";
            }
            if (c.Last != null && string.Join(",", c.Last) != string.Join(",", now))
            {
                stable = false;
                if (frames == 6)
                    Debug.Log("CIVBB constraints: " + c.Name + " still changes between frames");
            }
            c.Last = now;
            for (var i = 0; i < c.Nodes.Count; i++)
            {
                c.Nodes[i].localPosition = c.Input[i].p;
                c.Nodes[i].localRotation = c.Input[i].r;
                c.Nodes[i].localScale = c.Input[i].s;
            }
        }
        if (frames < 6)
            return;
        EditorApplication.update -= Tick;
        try
        {
            Write(stable);
        }
        finally
        {
            EditorApplication.Exit(0);
        }
    }

    static void Write(bool stable)
    {
        var s = new StringBuilder("{\n  \"unity\": \"" + Application.unityVersion + "\",\n  \"stable\": " +
                                  (stable ? "true" : "false") + ",\n  \"cases\": [");
        for (var k = 0; k < cases.Count; k++)
        {
            var c = cases[k];
            s.Append(k == 0 ? "\n    " : ",\n    ").Append("{\n      \"name\": \"").Append(c.Name)
                .Append("\",\n      \"nodes\": [");
            for (var i = 0; i < c.Nodes.Count; i++)
            {
                var t = c.Nodes[i];
                var (p, r, sc) = c.Input[i];
                s.Append(i == 0 ? "\n        " : ",\n        ").Append("{\"name\": \"")
                    .Append(t.name.Substring(c.Name.Length + 1)).Append("\", \"parent\": ")
                    .Append(t.parent == null ? "null" : c.Index(t.parent).ToString())
                    .Append(", \"rest\": {\"translation\": ").Append(V(p)).Append(", \"rotation\": ").Append(Q(r))
                    .Append(", \"scale\": ").Append(V(sc)).Append("}, \"active\": true}");
            }
            s.Append("\n      ],\n      \"constraints\": [");
            for (var i = 0; i < c.Json.Count; i++)
                s.Append(i == 0 ? "\n        " : ",\n        ").Append(c.Json[i]);
            s.Append("\n      ],\n      \"out\": [");
            for (var i = 0; i < c.Last.Length; i++)
                s.Append(i == 0 ? "\n        " : ",\n        ").Append(c.Last[i]);
            s.Append("\n      ]\n    }");
        }
        File.WriteAllText(Path.Combine(outDir, "constraints.json"), s.Append("\n  ]\n}\n").ToString());
        Debug.Log("CIVBB constraints recorded to " + outDir + (stable ? "" : " (UNSTABLE)"));
    }

    // ---- Cases ----

    static readonly Vector3 One = Vector3.one;

    /// A case with Root, a turned and scaled Rig under it, the target T under Rig, and sources A, B, C under Root.
    static Case Start(string name, out Transform t, out Transform[] src, Vector3? targetScale = null)
    {
        var c = new Case { Name = name };
        cases.Add(c);
        var root = c.Node("Root", null, Vector3.zero, Vector3.zero, One);
        var rig = c.Node("Rig", root, new Vector3(0.2f, 1f, 0.1f), new Vector3(10f, 30f, -20f), One * 1.5f);
        t = c.Node("T", rig, new Vector3(0.3f, -0.2f, 0.1f), new Vector3(5f, -15f, 25f), targetScale ?? One);
        src = new[]
        {
            c.Node("A", root, new Vector3(1f, 2f, -0.5f), new Vector3(40f, 10f, 70f), new Vector3(2f, 1f, 0.5f)),
            c.Node("B", root, new Vector3(-0.5f, 0.3f, 1.2f), new Vector3(-30f, 120f, 15f), new Vector3(0.5f, 3f, 1f)),
            c.Node("C", root, new Vector3(0.4f, -1f, 0.7f), new Vector3(80f, -45f, 0f), One * 0.8f),
        };
        return c;
    }

    static void Common(Case c, IConstraint k, Transform t, float weight, bool active, string kind, string axes,
        string rest, params (Transform node, float w, string offset)[] sources)
    {
        var s = new StringBuilder("{\"kind\": ").Append(kind).Append(", \"target\": ").Append(c.Index(t))
            .Append(", \"sources\": [");
        for (var i = 0; i < sources.Length; i++)
        {
            k.AddSource(new ConstraintSource { sourceTransform = sources[i].node, weight = sources[i].w });
            s.Append(i == 0 ? "" : ", ").Append("{\"node\": ").Append(c.Index(sources[i].node))
                .Append(", \"weight\": ").Append(F(sources[i].w)).Append(", \"offset\": ")
                .Append(sources[i].offset ?? Tr(Vector3.zero, Quaternion.identity, One)).Append('}');
        }
        k.weight = weight;
        k.locked = true;
        k.constraintActive = active;
        c.Json.Add(s.Append("], \"weight\": ").Append(F(weight)).Append(", \"active\": ").Append(active ? "true" : "false")
            .Append(", \"axes\": ").Append(axes).Append(", \"rest\": ").Append(rest).Append('}').ToString());
    }

    static (Transform, float, string) S(Transform t, float w) => (t, w, null);

    static void Position(Case c, Transform t, Vector3 offset, Vector3 rest, Axis axes, float weight, bool active,
        params (Transform, float, string)[] sources)
    {
        var k = t.gameObject.AddComponent<PositionConstraint>();
        k.translationOffset = offset;
        k.translationAtRest = rest;
        k.translationAxis = axes;
        Common(c, k, t, weight, active, "{\"Position\": {\"offset\": " + V(offset) + "}}", Ax(axes),
            Tr(rest, Quaternion.identity, One), sources);
    }

    static void Rotation(Case c, Transform t, Vector3 offset, Vector3 rest, Axis axes, float weight,
        params (Transform, float, string)[] sources)
    {
        var k = t.gameObject.AddComponent<RotationConstraint>();
        k.rotationOffset = offset;
        k.rotationAtRest = rest;
        k.rotationAxis = axes;
        Common(c, k, t, weight, true, "{\"Rotation\": {\"offset\": " + V(offset) + "}}", Ax(axes),
            Tr(Vector3.zero, Quaternion.Euler(rest), One), sources);
    }

    static void Scale(Case c, Transform t, Vector3 offset, Vector3 rest, Axis axes, float weight,
        params (Transform, float, string)[] sources)
    {
        var k = t.gameObject.AddComponent<ScaleConstraint>();
        k.scaleOffset = offset;
        k.scaleAtRest = rest;
        k.scalingAxis = axes;
        Common(c, k, t, weight, true, "{\"Scale\": {\"offset\": " + V(offset) + "}}", Ax(axes),
            Tr(Vector3.zero, Quaternion.identity, rest), sources);
    }

    static void Parent(Case c, Transform t, Vector3 restT, Vector3 restR, Axis tAxes, Axis rAxes, float weight,
        params (Transform node, float w, Vector3 ot, Vector3 or)[] sources)
    {
        var k = t.gameObject.AddComponent<ParentConstraint>();
        k.translationAtRest = restT;
        k.rotationAtRest = restR;
        k.translationAxis = tAxes;
        k.rotationAxis = rAxes;
        var list = new (Transform, float, string)[sources.Length];
        for (var i = 0; i < sources.Length; i++)
            list[i] = (sources[i].node, sources[i].w, Tr(sources[i].ot, Quaternion.Euler(sources[i].or), One));
        Common(c, k, t, weight, true, "{\"Parent\": {\"rotation_axes\": " + Ax(rAxes) + "}}", Ax(tAxes),
            Tr(restT, Quaternion.Euler(restR), One), list);
        for (var i = 0; i < sources.Length; i++)
        {
            k.SetTranslationOffset(i, sources[i].ot);
            k.SetRotationOffset(i, sources[i].or);
        }
    }

    static void Aim(Case c, Transform t, Vector3 aim, Vector3 up, AimConstraint.WorldUpType type, Vector3 worldUp,
        Transform upObject, Vector3 offset, Vector3 rest, Axis axes, float weight, params (Transform, float, string)[] sources)
    {
        var k = t.gameObject.AddComponent<AimConstraint>();
        k.aimVector = aim;
        k.upVector = up;
        k.worldUpType = type;
        k.worldUpVector = worldUp;
        k.worldUpObject = upObject;
        k.rotationOffset = offset;
        k.rotationAtRest = rest;
        k.rotationAxis = axes;
        string w;
        switch (type)
        {
            case AimConstraint.WorldUpType.SceneUp: w = "\"SceneUp\""; break;
            case AimConstraint.WorldUpType.Vector: w = "{\"Vector\": " + V(worldUp) + "}"; break;
            case AimConstraint.WorldUpType.ObjectUp: w = "{\"ObjectUp\": " + c.Index(upObject) + "}"; break;
            case AimConstraint.WorldUpType.ObjectRotationUp: w = "{\"ObjectRotationUp\": [" + c.Index(upObject) + ", " + V(worldUp) + "]}"; break;
            default: w = "\"None\""; break;
        }
        Common(c, k, t, weight, true, "{\"Aim\": {\"offset\": " + V(offset) + ", \"aim_axis\": " + V(aim) +
            ", \"up_axis\": " + V(up) + ", \"world_up\": " + w + "}}", Ax(axes),
            Tr(Vector3.zero, Quaternion.Euler(rest), One), sources);
    }

    static void LookAt(Case c, Transform t, float roll, Transform upObject, Vector3 offset, Vector3 rest, float weight,
        params (Transform, float, string)[] sources)
    {
        var k = t.gameObject.AddComponent<LookAtConstraint>();
        k.roll = roll;
        k.useUpObject = upObject != null;
        k.worldUpObject = upObject;
        k.rotationOffset = offset;
        k.rotationAtRest = rest;
        Common(c, k, t, weight, true, "{\"LookAt\": {\"offset\": " + V(offset) + ", \"roll\": " + F(roll) +
            ", \"world_up_node\": " + (upObject == null ? "null" : c.Index(upObject).ToString()) + "}}",
            "[true, true, true]", Tr(Vector3.zero, Quaternion.Euler(rest), One), sources);
    }

    static void Build()
    {
        const Axis All = Axis.X | Axis.Y | Axis.Z;
        Transform t;
        Transform[] s;
        Case c;
        var zero = Vector3.zero;

        c = Start("position_single", out t, out s);
        Position(c, t, zero, zero, All, 1f, true, S(s[0], 1f));
        c = Start("position_offset", out t, out s);
        Position(c, t, new Vector3(0.1f, 0.2f, 0.3f), zero, All, 1f, true, S(s[0], 1f));
        c = Start("position_two_weights", out t, out s);
        Position(c, t, zero, zero, All, 1f, true, S(s[0], 0.25f), S(s[1], 0.5f));
        c = Start("position_weights_over_one", out t, out s);
        Position(c, t, zero, zero, All, 1f, true, S(s[0], 0.9f), S(s[1], 0.8f), S(s[2], 0.7f));
        c = Start("position_weight_half", out t, out s);
        Position(c, t, zero, new Vector3(1f, 0f, 0f), All, 0.5f, true, S(s[0], 1f));
        c = Start("position_axes_xz", out t, out s);
        Position(c, t, zero, new Vector3(1f, 2f, 3f), Axis.X | Axis.Z, 1f, true, S(s[0], 1f));
        c = Start("position_zero_source_weight", out t, out s);
        Position(c, t, zero, new Vector3(1f, 2f, 3f), All, 1f, true, S(s[0], 0f));
        c = Start("position_no_sources", out t, out s);
        Position(c, t, zero, new Vector3(1f, 2f, 3f), All, 1f, true);
        c = Start("position_inactive", out t, out s);
        Position(c, t, zero, new Vector3(1f, 2f, 3f), All, 1f, false, S(s[0], 1f));

        c = Start("rotation_single", out t, out s);
        Rotation(c, t, zero, zero, All, 1f, S(s[0], 1f));
        c = Start("rotation_offset", out t, out s);
        Rotation(c, t, new Vector3(10f, 20f, 30f), zero, All, 1f, S(s[0], 1f));
        c = Start("rotation_two_weights", out t, out s);
        Rotation(c, t, zero, zero, All, 1f, S(s[0], 0.3f), S(s[1], 0.7f));
        c = Start("rotation_three_weights", out t, out s);
        Rotation(c, t, zero, zero, All, 1f, S(s[0], 0.2f), S(s[1], 0.5f), S(s[2], 0.9f));
        c = Start("rotation_first_weight_zero", out t, out s);
        Rotation(c, t, zero, zero, All, 1f, S(s[0], 0f), S(s[1], 0.6f), S(s[2], 0.3f));
        c = Start("rotation_four_weights", out t, out s);
        var extra = c.Node("D", c.Nodes[0], new Vector3(0f, 0f, 1f), new Vector3(-60f, -20f, 100f), One);
        Rotation(c, t, zero, zero, All, 1f, S(s[2], 0.4f), S(extra, 0.1f), S(s[0], 0.7f), S(s[1], 0.25f));
        c = Start("rotation_weight", out t, out s);
        Rotation(c, t, zero, new Vector3(0f, 90f, 0f), All, 0.4f, S(s[0], 1f));
        c = Start("rotation_axes_y", out t, out s);
        Rotation(c, t, zero, new Vector3(20f, 40f, 60f), Axis.Y, 1f, S(s[0], 1f));
        c = Start("rotation_axes_xz_weight", out t, out s);
        Rotation(c, t, new Vector3(0f, 0f, 15f), new Vector3(20f, 40f, 60f), Axis.X | Axis.Z, 0.7f, S(s[1], 1f));

        c = Start("scale_single", out t, out s, new Vector3(1.2f, 0.8f, 1.1f));
        Scale(c, t, One, One, All, 1f, S(s[0], 1f));
        c = Start("scale_offset", out t, out s, new Vector3(1.2f, 0.8f, 1.1f));
        Scale(c, t, new Vector3(2f, 1f, 0.5f), One, All, 1f, S(s[0], 1f));
        c = Start("scale_two_weights", out t, out s, new Vector3(1.2f, 0.8f, 1.1f));
        Scale(c, t, One, One, All, 1f, S(s[0], 0.5f), S(s[1], 0.5f));
        c = Start("scale_weight_axes", out t, out s, new Vector3(1.2f, 0.8f, 1.1f));
        Scale(c, t, One, new Vector3(3f, 2f, 1f), Axis.X | Axis.Y, 0.5f, S(s[1], 1f));

        // Sources without rotation, to separate the scale blend from the sources' turns.
        foreach (var (name, ws) in new[]
                 {
                     ("scale_aligned_two", new[] { 0.5f, 0.5f }), ("scale_aligned_unequal", new[] { 0.25f, 0.75f }),
                     ("scale_aligned_three", new[] { 0.2f, 0.5f, 0.9f }),
                 })
        {
            c = Start(name, out t, out s, new Vector3(1.2f, 0.8f, 1.1f));
            var d = new[]
            {
                c.Node("D", c.Nodes[0], new Vector3(0f, 1f, 0f), zero, new Vector3(2f, 1f, 0.5f)),
                c.Node("E", c.Nodes[0], new Vector3(1f, 0f, 0f), zero, new Vector3(0.5f, 3f, 1f)),
                c.Node("F", c.Nodes[0], new Vector3(0f, 0f, 1f), zero, new Vector3(1.5f, 0.25f, 2f)),
            };
            var list = new (Transform, float, string)[ws.Length];
            for (var i = 0; i < ws.Length; i++)
                list[i] = S(d[i], ws[i]);
            Scale(c, t, One, One, All, 1f, list);
        }
        c = Start("scale_turned_unequal", out t, out s, new Vector3(1.2f, 0.8f, 1.1f));
        Scale(c, t, One, One, All, 1f, S(s[0], 0.25f), S(s[1], 0.75f));

        c = Start("parent_single", out t, out s);
        Parent(c, t, zero, zero, All, All, 1f, (s[2], 1f, new Vector3(0.1f, 0f, 0.2f), new Vector3(0f, 90f, 0f)));
        c = Start("parent_two_weights", out t, out s);
        Parent(c, t, zero, zero, All, All, 1f,
            (s[2], 0.5f, new Vector3(0.1f, 0f, 0.2f), new Vector3(0f, 90f, 0f)),
            (s[0], 0.25f, new Vector3(0f, 0.3f, 0f), new Vector3(30f, 0f, 10f)));
        c = Start("parent_weight", out t, out s);
        Parent(c, t, new Vector3(0.5f, 0.5f, 0.5f), new Vector3(0f, 45f, 0f), All, All, 0.3f,
            (s[2], 1f, new Vector3(0.1f, 0f, 0.2f), new Vector3(0f, 90f, 0f)));
        c = Start("parent_axes", out t, out s);
        Parent(c, t, new Vector3(0.5f, 0.5f, 0.5f), new Vector3(10f, 45f, 5f), Axis.X, Axis.Y, 1f,
            (s[2], 1f, new Vector3(0.1f, 0f, 0.2f), new Vector3(0f, 90f, 0f)));

        var up = new Vector3(0.3f, 0.9f, -0.2f);
        c = Start("aim_scene_up", out t, out s);
        Aim(c, t, Vector3.right, Vector3.up, AimConstraint.WorldUpType.SceneUp, up, null, zero, zero, All, 1f, S(s[0], 1f));
        c = Start("aim_vector", out t, out s);
        Aim(c, t, Vector3.forward, Vector3.up, AimConstraint.WorldUpType.Vector, new Vector3(1f, 0f, 0f), null, zero, zero,
            All, 1f, S(s[0], 1f));
        c = Start("aim_object_up", out t, out s);
        Aim(c, t, Vector3.forward, Vector3.up, AimConstraint.WorldUpType.ObjectUp, up, s[2], zero, zero, All, 1f,
            S(s[0], 1f));
        c = Start("aim_object_rotation_up", out t, out s);
        Aim(c, t, Vector3.forward, Vector3.up, AimConstraint.WorldUpType.ObjectRotationUp, new Vector3(0f, 0f, 1f), s[2],
            zero, zero, All, 1f, S(s[0], 1f));
        c = Start("aim_none", out t, out s);
        Aim(c, t, new Vector3(0f, -1f, 0f), Vector3.forward, AimConstraint.WorldUpType.None, up, null, zero, zero, All, 1f,
            S(s[0], 1f));
        c = Start("aim_odd_axes", out t, out s);
        Aim(c, t, new Vector3(1f, 1f, 0f), new Vector3(0f, 0f, -1f), AimConstraint.WorldUpType.SceneUp, up, null, zero,
            zero, All, 1f, S(s[1], 1f));
        c = Start("aim_two_offset_weight", out t, out s);
        Aim(c, t, Vector3.forward, Vector3.up, AimConstraint.WorldUpType.SceneUp, up, null, new Vector3(0f, 0f, 30f),
            new Vector3(0f, 10f, 0f), All, 0.6f, S(s[0], 0.5f), S(s[1], 1f));
        c = Start("aim_axes_y", out t, out s);
        Aim(c, t, Vector3.forward, Vector3.up, AimConstraint.WorldUpType.SceneUp, up, null, zero, new Vector3(20f, 0f, 10f),
            Axis.Y, 1f, S(s[0], 1f));

        c = Start("look_at_roll", out t, out s);
        LookAt(c, t, 30f, null, zero, zero, 1f, S(s[0], 1f));
        c = Start("look_at_up_object", out t, out s);
        LookAt(c, t, 30f, s[2], zero, zero, 1f, S(s[0], 1f));
        c = Start("look_at_weight_offset", out t, out s);
        LookAt(c, t, 0f, null, new Vector3(0f, 20f, 0f), new Vector3(10f, 0f, 0f), 0.5f, S(s[0], 0.5f), S(s[1], 1f));

        // Chains: the second constraint reads the first one's target, created in order and reversed; and a
        // constraint on a node whose parent is constrained, created child first.
        foreach (var reversed in new[] { false, true })
        {
            c = Start(reversed ? "chain_reversed" : "chain_ordered", out t, out s);
            var u = c.Node("U", c.Nodes[0], new Vector3(0f, 0.5f, 0f), new Vector3(0f, 0f, 45f), One);
            if (reversed)
                Rotation(c, u, zero, zero, All, 1f, S(t, 1f));
            Rotation(c, t, zero, zero, All, 1f, S(s[0], 1f));
            if (!reversed)
                Rotation(c, u, zero, zero, All, 1f, S(t, 1f));
        }
        c = Start("chain_parent_first", out t, out s);
        Position(c, t, zero, zero, All, 1f, true, S(s[1], 1f));
        Rotation(c, c.Nodes[1], zero, zero, All, 1f, S(s[0], 1f));
    }

    // ---- JSON ----

    static string F(float x) => x.ToString("R", CultureInfo.InvariantCulture);
    static string V(Vector3 v) => "[" + F(v.x) + ", " + F(v.y) + ", " + F(v.z) + "]";
    static string Q(Quaternion q) => "[" + F(q.x) + ", " + F(q.y) + ", " + F(q.z) + ", " + F(q.w) + "]";
    static string Tr(Vector3 p, Quaternion r, Vector3 s) =>
        "{\"translation\": " + V(p) + ", \"rotation\": " + Q(r) + ", \"scale\": " + V(s) + "}";
    static string Ax(Axis a) =>
        "[" + ((a & Axis.X) != 0 ? "true" : "false") + ", " + ((a & Axis.Y) != 0 ? "true" : "false") + ", " +
        ((a & Axis.Z) != 0 ? "true" : "false") + "]";
}
