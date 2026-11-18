// Records the VRChat SDK's own constraint runtime as ground truth for civbb-anim's VRChat options (ADR 0040, #73,
// #75): Solve In Local Space, Freeze To World and Rebake Offsets When Unfrozen. The SDK is not in this project: copy
// this file into the exporter's VRChat host project (corpus/local/unity-export-vrc/Assets/Editor) and run it there;
// it reaches the SDK's components only by reflection, and every case is generated (no third-party data), so the
// output may be committed. Each case runs a timeline: every frame sets the inputs (sources, the target's parent and,
// unless the case says otherwise, the target as an animator would) and option changes, and records the inputs and
// what the SDK made of them. Constraints only run in play mode: run it without -quit; it exits by itself.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using UnityEditor;
using UnityEngine;

public static class VrcConstraints
{
    const string Ns = "VRC.SDK3.Dynamics.Constraint.Components.VRC";
    const int Frames = 10;

    class Case
    {
        public string Name;
        public bool AnimateTarget = true;
        public readonly List<Transform> Nodes = new List<Transform>();
        public readonly List<Component> Constraints = new List<Component>();
        public readonly List<string> Json = new List<string>();
        public readonly List<(int frame, int constraint, string field, float value)> Script = new List<(int, int, string, float)>();
        public readonly List<string> FrameJson = new List<string>();
        public string PendingIn, PendingSet;

        public Transform Node(string name, Transform parent)
        {
            var t = new GameObject(Name + "/" + name).transform;
            t.SetParent(parent, false);
            Nodes.Add(t);
            return t;
        }

        public int Index(Transform t) => Nodes.IndexOf(t);
        public Transform this[string name] => Nodes.First(n => n.name == Name + "/" + name);
    }

    static readonly List<Case> cases = new List<Case>();
    static string outPath;
    static int tick, lastFrame = -1;
    static bool stable = true;

    /// `-executeMethod VrcConstraints.Record -civbbOut <absolute json path>`.
    public static void Record()
    {
        try
        {
            var args = Environment.GetCommandLineArgs();
            outPath = args[Array.IndexOf(args, "-civbbOut") + 1];
            Build();
            foreach (var c in cases)
                Inputs(c, 0);
            EditorSettings.enterPlayModeOptionsEnabled = true;
            EditorSettings.enterPlayModeOptions = EnterPlayModeOptions.DisableDomainReload | EnterPlayModeOptions.DisableSceneReload;
            EditorApplication.update += Tick;
            EditorApplication.EnterPlaymode();
        }
        catch (Exception e)
        {
            Debug.LogError("CIVBB failed: " + e);
            EditorApplication.Exit(1);
        }
    }

    // Tick n (from 1) reads what frame n - 2 produced and sets frame n - 1's inputs.
    static void Tick()
    {
        if (!EditorApplication.isPlaying)
            return;
        var done = false;
        try
        {
            tick++;
            if (lastFrame >= 0 && Time.frameCount != lastFrame + 1)
            {
                stable = false;
                Debug.Log($"CIVBB vrc constraints: frame {lastFrame} then {Time.frameCount}");
            }
            lastFrame = Time.frameCount;
            int frame = tick - 1;
            foreach (var c in cases)
            {
                if (c.PendingIn != null)
                {
                    c.FrameJson.Add("{\"set\": " + c.PendingSet + ", \"input\": " + c.PendingIn + ", \"out\": " + Locals(c) +
                                    ", \"fields\": " + Fields(c) + "}");
                    c.PendingIn = null;
                }
                if (frame >= Frames)
                    continue;
                if (frame > 0)
                    Inputs(c, frame);
                var set = new List<string>();
                foreach (var (_, k, field, value) in c.Script.Where(s => s.frame == frame))
                {
                    Set(c.Constraints[k], field, value);
                    set.Add($"[{k}, {FieldJson(field)}, {F(value)}]");
                }
                c.PendingSet = "[" + string.Join(", ", set) + "]";
                c.PendingIn = Locals(c);
            }
            done = frame >= Frames;
            if (done)
                Write();
        }
        catch (Exception e)
        {
            Debug.LogError("CIVBB failed: " + e);
            done = true;
        }
        if (done)
        {
            EditorApplication.update -= Tick;
            EditorApplication.Exit(0);
        }
    }

    // ---- Inputs ----

    /// Frame `f`'s inputs: Rig (the target's parent) and the sources turn and move every frame, so world and local
    /// values, and held and followed ones, differ; the target too when an animator would write it.
    static void Inputs(Case c, int f)
    {
        void Set(string n, Vector3 p, Vector3 e, Vector3 s)
        {
            var t = c[n];
            t.localPosition = p;
            t.localRotation = Quaternion.Euler(e);
            t.localScale = s;
        }
        var one = Vector3.one;
        Set("Root", Vector3.zero, Vector3.zero, one);
        Set("Rig", new Vector3(0.2f + 0.05f * f, 1f, 0.1f - 0.03f * f), new Vector3(10f + 4f * f, 30f - 6f * f, -20f + 3f * f),
            one * (1.5f + 0.05f * f));
        Set("A", new Vector3(1f - 0.1f * f, 2f, -0.5f + 0.07f * f), new Vector3(40f + 9f * f, 10f, 70f - 5f * f),
            new Vector3(2f, 1f + 0.1f * f, 0.5f));
        Set("B", new Vector3(-0.5f, 0.3f + 0.08f * f, 1.2f), new Vector3(-30f, 120f - 7f * f, 15f + 2f * f),
            new Vector3(0.5f + 0.05f * f, 3f, 1f));
        Set("C", new Vector3(0.4f + 0.06f * f, -1f, 0.7f), new Vector3(80f - 3f * f, -45f, 8f * f), one * 0.8f);
        if (c.AnimateTarget || f == 0)
            Set("T", new Vector3(0.3f, -0.2f + 0.04f * f, 0.1f), new Vector3(5f + 5f * f, -15f, 25f - 4f * f),
                new Vector3(1.2f, 0.8f + 0.02f * f, 1.1f));
    }

    static Case Start(string name, bool animateTarget = true)
    {
        var c = new Case { Name = name, AnimateTarget = animateTarget };
        cases.Add(c);
        var root = c.Node("Root", null);
        var rig = c.Node("Rig", root);
        c.Node("T", rig);
        foreach (var n in new[] { "A", "B", "C" })
            c.Node(n, root);
        return c;
    }

    // ---- SDK components, by reflection ----

    static Type SdkType(string name) => AppDomain.CurrentDomain.GetAssemblies()
        .Select(a => a.GetType(name)).FirstOrDefault(t => t != null) ?? throw new TypeLoadException(name);

    static void Field(Component k, string name, object value)
    {
        var f = k.GetType().GetField(name, BindingFlags.Public | BindingFlags.Instance) ?? throw new MissingFieldException(name);
        f.SetValue(k, f.FieldType.IsEnum && value is string s ? Enum.Parse(f.FieldType, s) : value);
    }

    static object Get(Component k, string name) =>
        k.GetType().GetField(name, BindingFlags.Public | BindingFlags.Instance)?.GetValue(k);

    static void Set(Component k, string field, float value)
    {
        switch (field)
        {
            case "Weight": Field(k, "GlobalWeight", value); break;
            case "Active": Field(k, "IsActive", value >= 0.5f); break;
            case "Freeze": Field(k, "FreezeToWorld", value >= 0.5f); break;
            default:
                int i = int.Parse(field.Substring("Source".Length));
                var get = k.GetType().GetMethod("GetSource");
                var src = get.Invoke(k, new object[] { i });
                src.GetType().GetField("Weight").SetValue(src, value);
                k.GetType().GetMethod("SetSource").Invoke(k, new[] { (object)i, src });
                break;
        }
        k.GetType().GetMethod("ApplyConfigurationChanges").Invoke(k, null);
    }

    static string FieldJson(string field) => field.StartsWith("Source")
        ? "{\"SourceWeight\": " + field.Substring("Source".Length) + "}"
        : "\"" + field + "\"";

    class Opt
    {
        public bool Local, Freeze, Rebake, Active = true;
        public float Weight = 1f;
        public bool[] Axes = { true, true, true }, RotationAxes = { true, true, true };
        public Vector3 Offset, RestP, RestR;
        public Vector3 RestS = Vector3.one, ScaleOffset = Vector3.one;
        public Vector3 Aim = Vector3.forward, Up = Vector3.up, WorldUpVector = Vector3.up;
        public string WorldUp = "SceneUp";
        public float Roll;
        public Transform UpNode;
    }

    /// Adds a VRChat constraint of `kind` (Position, Rotation, Scale, Parent, Aim, LookAt) on T and returns its index.
    static int Add(Case c, string kind, Opt o, params (Transform node, float w, Vector3 ot, Vector3 or)[] sources)
    {
        var t = c["T"];
        var k = t.gameObject.AddComponent(SdkType(Ns + kind + "Constraint"));
        string AxesJson(bool[] a) => "[" + string.Join(", ", a.Select(b => b ? "true" : "false")) + "]";
        void Axes(string what, bool[] a)
        {
            Field(k, $"Affects{what}X", a[0]);
            Field(k, $"Affects{what}Y", a[1]);
            Field(k, $"Affects{what}Z", a[2]);
        }
        string json, axes = AxesJson(o.Axes), rest;
        switch (kind)
        {
            case "Position":
                Field(k, "PositionOffset", o.Offset);
                Field(k, "PositionAtRest", o.RestP);
                Axes("Position", o.Axes);
                json = "{\"Position\": {\"offset\": " + V(o.Offset) + "}}";
                rest = Tr(o.RestP, Quaternion.identity, Vector3.one);
                break;
            case "Rotation":
                Field(k, "RotationOffset", o.Offset);
                Field(k, "RotationAtRest", o.RestR);
                Axes("Rotation", o.Axes);
                json = "{\"Rotation\": {\"offset\": " + V(o.Offset) + "}}";
                rest = Tr(Vector3.zero, Quaternion.Euler(o.RestR), Vector3.one);
                break;
            case "Scale":
                Field(k, "ScaleOffset", o.ScaleOffset);
                Field(k, "ScaleAtRest", o.RestS);
                Axes("Scale", o.Axes);
                json = "{\"Scale\": {\"offset\": " + V(o.ScaleOffset) + "}}";
                rest = Tr(Vector3.zero, Quaternion.identity, o.RestS);
                break;
            case "Parent":
                Field(k, "PositionAtRest", o.RestP);
                Field(k, "RotationAtRest", o.RestR);
                Axes("Position", o.Axes);
                Axes("Rotation", o.RotationAxes);
                json = "{\"Parent\": {\"rotation_axes\": " + AxesJson(o.RotationAxes) + "}}";
                rest = Tr(o.RestP, Quaternion.Euler(o.RestR), Vector3.one);
                break;
            case "Aim":
                Field(k, "RotationOffset", o.Offset);
                Field(k, "RotationAtRest", o.RestR);
                Axes("Rotation", o.Axes);
                Field(k, "AimAxis", o.Aim);
                Field(k, "UpAxis", o.Up);
                Field(k, "WorldUp", o.WorldUp);
                Field(k, "WorldUpVector", o.WorldUpVector);
                Field(k, "WorldUpTransform", o.UpNode);
                string w = o.WorldUp == "Vector" ? "{\"Vector\": " + V(o.WorldUpVector) + "}"
                    : o.WorldUp == "ObjectUp" ? "{\"ObjectUp\": " + c.Index(o.UpNode) + "}"
                    : o.WorldUp == "ObjectRotationUp" ? "{\"ObjectRotationUp\": [" + c.Index(o.UpNode) + ", " + V(o.WorldUpVector) + "]}"
                    : "\"" + o.WorldUp + "\"";
                json = "{\"Aim\": {\"offset\": " + V(o.Offset) + ", \"aim_axis\": " + V(o.Aim) + ", \"up_axis\": " + V(o.Up) +
                       ", \"world_up\": " + w + "}}";
                rest = Tr(Vector3.zero, Quaternion.Euler(o.RestR), Vector3.one);
                break;
            default:
                Field(k, "RotationOffset", o.Offset);
                Field(k, "RotationAtRest", o.RestR);
                Field(k, "Roll", o.Roll);
                Field(k, "UseUpTransform", o.UpNode != null);
                Field(k, "WorldUpTransform", o.UpNode);
                json = "{\"LookAt\": {\"offset\": " + V(o.Offset) + ", \"roll\": " + F(o.Roll) + ", \"world_up_node\": " +
                       (o.UpNode == null ? "null" : c.Index(o.UpNode).ToString()) + "}}";
                axes = "[true, true, true]";
                rest = Tr(Vector3.zero, Quaternion.Euler(o.RestR), Vector3.one);
                break;
        }
        var srcType = SdkType("VRC.Dynamics.VRCConstraintSource");
        var parts = new List<string>();
        foreach (var (node, weight, ot, or) in sources)
        {
            var s = Activator.CreateInstance(srcType);
            srcType.GetField("SourceTransform").SetValue(s, node);
            srcType.GetField("Weight").SetValue(s, weight);
            srcType.GetField("ParentPositionOffset").SetValue(s, ot);
            srcType.GetField("ParentRotationOffset").SetValue(s, or);
            k.GetType().GetMethod("AddSource").Invoke(k, new[] { s });
            parts.Add("{\"node\": " + c.Index(node) + ", \"weight\": " + F(weight) + ", \"offset\": " +
                      (kind == "Parent" ? Tr(ot, Quaternion.Euler(or), Vector3.one) : Tr(Vector3.zero, Quaternion.identity, Vector3.one)) + "}");
        }
        Field(k, "GlobalWeight", o.Weight);
        Field(k, "IsActive", o.Active);
        Field(k, "Locked", true);
        Field(k, "SolveInLocalSpace", o.Local);
        Field(k, "FreezeToWorld", o.Freeze);
        Field(k, "RebakeOffsetsWhenUnfrozen", o.Rebake);
        k.GetType().GetMethod("ApplyConfigurationChanges").Invoke(k, null);
        c.Constraints.Add(k);
        c.Json.Add("{\"kind\": " + json + ", \"target\": " + c.Index(t) + ", \"sources\": [" + string.Join(", ", parts) +
                   "], \"weight\": " + F(o.Weight) + ", \"active\": " + (o.Active ? "true" : "false") + ", \"axes\": " + axes +
                   ", \"rest\": " + rest + ", \"options\": {\"local_space\": " + B(o.Local) + ", \"freeze\": " + B(o.Freeze) +
                   ", \"rebake\": " + B(o.Rebake) + "}}");
        return c.Constraints.Count - 1;
    }

    static (Transform, float, Vector3, Vector3) S(Transform t, float w) => (t, w, Vector3.zero, Vector3.zero);

    // ---- Cases ----

    static void Build()
    {
        var kinds = new[] { "Position", "Rotation", "Scale", "Parent", "Aim", "LookAt" };
        Opt Base(string kind) => kind == "Rotation" || kind == "Aim" || kind == "LookAt"
            ? new Opt { Offset = new Vector3(5f, -10f, 15f) }
            : kind == "Position" ? new Opt { Offset = new Vector3(0.1f, 0.2f, -0.1f) }
            : kind == "Scale" ? new Opt { ScaleOffset = new Vector3(1.5f, 1f, 0.8f) } : new Opt();
        (Transform, float, Vector3, Vector3)[] Sources(Case c, string kind) => kind == "Parent"
            ? new[] { (c["C"], 1f, new Vector3(0.1f, 0f, 0.2f), new Vector3(0f, 90f, 0f)), (c["A"], 0.5f, new Vector3(0f, 0.3f, 0f), new Vector3(30f, 0f, 10f)) }
            : new[] { S(c["A"], 1f), S(c["B"], 0.5f) };

        // Plain: no options, to check the frame timing against civbb's evaluation of the same inputs.
        foreach (var kind in kinds)
        {
            var c = Start("plain_" + kind.ToLowerInvariant());
            Add(c, kind, Base(kind), Sources(c, kind));
        }
        // Solve In Local Space.
        foreach (var kind in kinds)
        {
            var c = Start("local_" + kind.ToLowerInvariant());
            var o = Base(kind);
            o.Local = true;
            Add(c, kind, o, Sources(c, kind));
        }
        // Scale: how the offset combines with source weights.
        foreach (var (name, ws, offset) in new[]
                 {
                     ("one", new[] { 1f }, new Vector3(1.5f, 1f, 0.8f)), ("one_half", new[] { 0.5f }, new Vector3(1.5f, 1f, 0.8f)),
                     ("two_equal", new[] { 1f, 1f }, new Vector3(1.5f, 1f, 0.8f)), ("two_halves", new[] { 0.5f, 0.5f }, new Vector3(1.5f, 1f, 0.8f)),
                     ("two_quarter", new[] { 0.25f, 0.75f }, new Vector3(1.5f, 1f, 0.8f)), ("three", new[] { 0.2f, 0.5f, 0.9f }, new Vector3(1.5f, 1f, 0.8f)),
                     ("two_no_offset", new[] { 1f, 0.5f }, Vector3.one), ("two_one_zero", new[] { 1f, 0f }, new Vector3(1.5f, 1f, 0.8f)),
                     ("three_one_zero", new[] { 0.5f, 0f, 1f }, new Vector3(1.5f, 1f, 0.8f)),
                     ("zero_one", new[] { 0f, 1f }, new Vector3(1.5f, 1f, 0.8f)), ("half_zero", new[] { 0.5f, 0f }, new Vector3(1.5f, 1f, 0.8f)),
                     ("one_zero_zero", new[] { 1f, 0f, 0f }, new Vector3(1.5f, 1f, 0.8f)), ("zero_half_one", new[] { 0f, 0.5f, 1f }, new Vector3(1.5f, 1f, 0.8f)),
                     ("one_zero_one", new[] { 1f, 0f, 1f }, new Vector3(1.5f, 1f, 0.8f)), ("one_one_zero", new[] { 1f, 1f, 0f }, new Vector3(1.5f, 1f, 0.8f)), ("two_big_offset", new[] { 1f, 0.5f }, new Vector3(3f, 2f, 0.25f)),
                 })
        {
            var c = Start("scale_" + name);
            var nodes = new[] { c["A"], c["B"], c["C"] };
            Add(c, "Scale", new Opt { ScaleOffset = offset }, ws.Select((w, i) => S(nodes[i], w)).ToArray());
        }
        {
            var c = Start("scale_weight_half");
            Add(c, "Scale", new Opt { Weight = 0.5f, ScaleOffset = new Vector3(1.5f, 1f, 0.8f), RestS = new Vector3(2f, 1f, 1f) }, S(c["A"], 1f), S(c["B"], 0.5f));
        }
        {
            var c = Start("local_aim_object_up");
            Add(c, "Aim", new Opt { Local = true, WorldUp = "ObjectUp", UpNode = c["C"] }, S(c["A"], 1f));
            c = Start("local_rotation_weight_axes");
            Add(c, "Rotation", new Opt { Local = true, Weight = 0.6f, RestR = new Vector3(20f, 40f, 60f), Axes = new[] { true, false, true } }, S(c["B"], 1f));
        }
        // Freeze To World: on at frame 3, off at frame 6; with and without rebaking; target animated or left alone.
        foreach (var kind in kinds)
        {
            foreach (var rebake in new[] { false, true })
            {
                foreach (var animated in new[] { true, false })
                {
                    var c = Start($"freeze_{kind.ToLowerInvariant()}{(rebake ? "_rebake" : "")}{(animated ? "" : "_still")}", animated);
                    var o = Base(kind);
                    o.Rebake = rebake;
                    int k = Add(c, kind, o, Sources(c, kind));
                    c.Script.Add((3, k, "Freeze", 1f));
                    c.Script.Add((6, k, "Freeze", 0f));
                }
            }
        }
        // Per-axis, weight, local space, freeze at start, freeze while inactive, deactivation while frozen.
        foreach (var rebake in new[] { false, true })
        {
            string r = rebake ? "_rebake" : "";
            var c = Start("freeze_position_axes_xz" + r);
            int k = Add(c, "Position", new Opt { Rebake = rebake, Axes = new[] { true, false, true }, RestP = new Vector3(1f, 2f, 3f) }, S(c["A"], 1f));
            c.Script.Add((3, k, "Freeze", 1f));
            c.Script.Add((6, k, "Freeze", 0f));
            c = Start("freeze_rotation_axes_y" + r);
            k = Add(c, "Rotation", new Opt { Rebake = rebake, Axes = new[] { false, true, false } }, S(c["A"], 1f));
            c.Script.Add((3, k, "Freeze", 1f));
            c.Script.Add((6, k, "Freeze", 0f));
            c = Start("freeze_parent_axes" + r);
            k = Add(c, "Parent", new Opt { Rebake = rebake, Axes = new[] { true, false, false }, RotationAxes = new[] { false, true, false } },
                (c["C"], 1f, new Vector3(0.1f, 0f, 0.2f), new Vector3(0f, 90f, 0f)));
            c.Script.Add((3, k, "Freeze", 1f));
            c.Script.Add((6, k, "Freeze", 0f));
            c = Start("freeze_rotation_weight" + r);
            k = Add(c, "Rotation", new Opt { Rebake = rebake, Weight = 0.5f, RestR = new Vector3(0f, 90f, 0f) }, S(c["A"], 1f));
            c.Script.Add((3, k, "Freeze", 1f));
            c.Script.Add((6, k, "Freeze", 0f));
            c = Start("freeze_local_rotation" + r);
            k = Add(c, "Rotation", new Opt { Rebake = rebake, Local = true }, S(c["A"], 1f));
            c.Script.Add((3, k, "Freeze", 1f));
            c.Script.Add((6, k, "Freeze", 0f));
            c = Start("freeze_at_start_parent" + r);
            k = Add(c, "Parent", new Opt { Rebake = rebake, Freeze = true }, (c["C"], 1f, new Vector3(0.1f, 0f, 0.2f), new Vector3(0f, 90f, 0f)));
            c.Script.Add((5, k, "Freeze", 0f));
            c = Start("freeze_while_inactive" + r);
            k = Add(c, "Rotation", new Opt { Rebake = rebake }, S(c["A"], 1f));
            c.Script.Add((1, k, "Active", 0f));
            c.Script.Add((3, k, "Freeze", 1f));
            c.Script.Add((5, k, "Active", 1f));
            c.Script.Add((7, k, "Freeze", 0f));
            c = Start("freeze_then_inactive" + r);
            k = Add(c, "Parent", new Opt { Rebake = rebake }, (c["C"], 1f, new Vector3(0.1f, 0f, 0.2f), new Vector3(0f, 90f, 0f)));
            c.Script.Add((2, k, "Freeze", 1f));
            c.Script.Add((4, k, "Active", 0f));
            c.Script.Add((6, k, "Active", 1f));
            c.Script.Add((8, k, "Freeze", 0f));
            c = Start("freeze_twice" + r);
            k = Add(c, "Position", new Opt { Rebake = rebake }, S(c["A"], 1f));
            c.Script.Add((2, k, "Freeze", 1f));
            c.Script.Add((4, k, "Freeze", 0f));
            c.Script.Add((6, k, "Freeze", 1f));
            c.Script.Add((8, k, "Freeze", 0f));
            c = Start("freeze_weight_change" + r);
            k = Add(c, "Rotation", new Opt { Rebake = rebake }, S(c["A"], 1f), S(c["B"], 1f));
            c.Script.Add((2, k, "Freeze", 1f));
            c.Script.Add((3, k, "Source1", 0f));
            c.Script.Add((4, k, "Weight", 0.5f));
            c.Script.Add((5, k, "Freeze", 0f));
        }
    }

    // ---- JSON ----

    static void Write()
    {
        var s = new StringBuilder("{\n  \"unity\": \"" + Application.unityVersion + "\",\n  \"stable\": " + B(stable) + ",\n  \"cases\": [");
        for (var k = 0; k < cases.Count; k++)
        {
            var c = cases[k];
            s.Append(k == 0 ? "\n    " : ",\n    ").Append("{\n      \"name\": \"").Append(c.Name).Append("\",\n      \"nodes\": [");
            for (var i = 0; i < c.Nodes.Count; i++)
            {
                var t = c.Nodes[i];
                s.Append(i == 0 ? "\n        " : ",\n        ").Append("{\"name\": \"").Append(t.name.Substring(c.Name.Length + 1))
                    .Append("\", \"parent\": ").Append(t.parent == null ? "null" : c.Index(t.parent).ToString())
                    .Append(", \"rest\": ").Append(Tr(Vector3.zero, Quaternion.identity, Vector3.one)).Append(", \"active\": true}");
            }
            s.Append("\n      ],\n      \"constraints\": [\n        ").Append(string.Join(",\n        ", c.Json))
                .Append("\n      ],\n      \"frames\": [\n        ").Append(string.Join(",\n        ", c.FrameJson)).Append("\n      ]\n    }");
        }
        File.WriteAllText(outPath, s.Append("\n  ]\n}\n").ToString());
        Debug.Log($"CIVBB vrc constraints recorded {cases.Count} cases to {outPath}{(stable ? "" : " (UNSTABLE)")}");
    }

    static string Locals(Case c) => "[" + string.Join(", ", c.Nodes.Select(t => Tr(t.localPosition, t.localRotation, t.localScale))) + "]";

    /// The offsets and at-rest values as the component holds them now (rebaking rewrites them).
    static string Fields(Case c) => "[" + string.Join(", ", c.Constraints.Select(k =>
    {
        var parts = new List<string>();
        foreach (var n in new[] { "PositionOffset", "RotationOffset", "ScaleOffset", "PositionAtRest", "RotationAtRest", "ScaleAtRest" })
            if (Get(k, n) is Vector3 v)
                parts.Add("\"" + n + "\": " + V(v));
        int count = (int)k.GetType().GetProperty("SourceCount").GetValue(k);
        var src = new List<string>();
        for (int i = 0; i < count; i++)
        {
            var o = k.GetType().GetMethod("GetSource").Invoke(k, new object[] { i });
            src.Add("[" + V((Vector3)o.GetType().GetField("ParentPositionOffset").GetValue(o)) + ", " +
                    V((Vector3)o.GetType().GetField("ParentRotationOffset").GetValue(o)) + "]");
        }
        parts.Add("\"sources\": [" + string.Join(", ", src) + "]");
        return "{" + string.Join(", ", parts) + "}";
    })) + "]";

    static string B(bool b) => b ? "true" : "false";
    static string F(float x) => x.ToString("R", CultureInfo.InvariantCulture);
    static string V(Vector3 v) => "[" + F(v.x) + ", " + F(v.y) + ", " + F(v.z) + "]";
    static string Q(Quaternion q) => "[" + F(q.x) + ", " + F(q.y) + ", " + F(q.z) + ", " + F(q.w) + "]";
    static string Tr(Vector3 p, Quaternion r, Vector3 s) =>
        "{\"translation\": " + V(p) + ", \"rotation\": " + Q(r) + ", \"scale\": " + V(s) + "}";
}
