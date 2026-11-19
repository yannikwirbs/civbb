// Records the VRChat SDK's own contact runtime as ground truth for civbb-dynamics' contacts (ADR 0017, #55, #75): a
// sender moves through a receiver along a path, one step per frame, and each frame's receiver value is recorded. Like
// VrcConstraints.cs it needs the SDK: copy it into corpus/local/unity-export-vrc/Assets/Editor and run it there; it
// reaches the SDK only by reflection and every case is generated, so the output may be committed. Contacts run in play
// mode: run it without -quit; it exits by itself.
using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using UnityEditor;
using UnityEngine;

public static class VrcContacts
{
    const string Ns = "VRC.SDK3.Dynamics.Contact.Components.";
    const float Dt = 1f / 60f;

    class Shape
    {
        public string Type = "Sphere";
        public float Radius = 0.1f, Height = 0.5f;
        public Vector3 Size = Vector3.one * 0.2f, Position, Euler;
    }

    class Case
    {
        public string Name;
        public Transform Root, Sender, Receiver, Component;
        public Shape SenderShape, ReceiverShape;
        public string Kind = "Proximity";
        public float MinVelocity;
        public bool Face;
        public Vector3[] Path;
        public Func<int, bool> ComponentActive = f => true, ReceiverActive = f => true;
        public Component Rx;
        public readonly List<string> Frames = new List<string>();
        public string Pending;
    }

    static readonly List<Case> cases = new List<Case>();
    static string outPath;
    static int tick;
    static float firstTime = -1;

    /// `-executeMethod VrcContacts.Record -civbbOut <absolute json path>`.
    public static void Record()
    {
        try
        {
            outPath = Environment.GetCommandLineArgs().SkipWhile(a => a != "-civbbOut").Skip(1).First();
            Debug.Log("CIVBB vrc contacts: shape types " + string.Join(",", Enum.GetNames(Field(Type("ContactBase"), "shapeType").FieldType)) +
                      "; receiver types " + string.Join(",", Enum.GetNames(Field(Type("ContactReceiver"), "receiverType").FieldType)));
            Build();
            foreach (var c in cases)
                Apply(c, 0);
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

    static Type Type(string name) => AppDomain.CurrentDomain.GetAssemblies().Select(a => a.GetType("VRC.Dynamics." + name) ?? a.GetType(Ns + name))
        .FirstOrDefault(t => t != null) ?? throw new TypeLoadException(name);

    static FieldInfo Field(Type t, string name) =>
        t.GetField(name, BindingFlags.Public | BindingFlags.Instance) ?? throw new MissingFieldException(t.Name, name);

    static void Set(Component k, string name, object value)
    {
        var f = Field(k.GetType(), name);
        f.SetValue(k, f.FieldType.IsEnum && value is string s ? Enum.Parse(f.FieldType, s) : value);
    }

    // Tick n (from 1) reads what frame n - 2 produced and sets frame n - 1's inputs.
    static void Tick()
    {
        if (!EditorApplication.isPlaying)
            return;
        var done = false;
        try
        {
            Time.captureDeltaTime = Dt;
            tick++;
            int frame = tick - 1;
            int frames = cases.Max(c => c.Path.Length);
            foreach (var c in cases)
            {
                if (c.Pending != null)
                {
                    float value = (float)Field(c.Rx.GetType(), "paramValue").GetValue(c.Rx);
                    c.Frames.Add(c.Pending + ", \"dt\": " + F(Time.deltaTime) + ", \"value\": " + F(value) + "}");
                    c.Pending = null;
                }
                if (frame >= c.Path.Length)
                    continue;
                if (frame > 0)
                    Apply(c, frame);
                c.Pending = "{\"sender\": " + Pose(c.Sender) + ", \"receiver\": " + Pose(c.Receiver) + ", \"component\": " +
                            B(c.Component.gameObject.activeInHierarchy) + ", \"receiver_active\": " + B(c.Receiver.gameObject.activeInHierarchy);
            }
            done = frame >= frames;
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

    static void Apply(Case c, int f)
    {
        c.Sender.localPosition = c.Path[f];
        c.Component.gameObject.SetActive(c.ComponentActive(f));
        c.Receiver.gameObject.SetActive(c.ReceiverActive(f));
    }

    // ---- Cases ----

    static Component Contact(Case c, bool receiver, Transform host, Transform root, Shape s)
    {
        var k = host.gameObject.AddComponent(Type(receiver ? "VRCContactReceiver" : "VRCContactSender"));
        Set(k, "rootTransform", root);
        Set(k, "shapeType", s.Type);
        Set(k, "radius", s.Radius);
        Set(k, "height", s.Height);
        Set(k, "size", s.Size);
        Set(k, "position", s.Position);
        Set(k, "rotation", Quaternion.Euler(s.Euler));
        Set(k, "collisionTags", new List<string> { c.Name });
        if (receiver)
        {
            Set(k, "allowSelf", true);
            Set(k, "allowOthers", true);
            Set(k, "receiverType", c.Kind);
            Set(k, "parameter", "civbb_" + c.Name);
            Set(k, "minVelocity", c.MinVelocity);
            Set(k, "useFaceProximity", c.Face);
        }
        k.GetType().GetMethod("ApplyConfigurationChanges")?.Invoke(k, null);
        return k;
    }

    /// A case at its own place in the scene: Root (scaled by `rootScale`), the receiver node R turned by `receiverEuler`,
    /// the receiver component on R or on a separate node C, and the sender node S moving along `path` (Root space).
    static Case Add(string name, Shape sender, Shape receiver, Vector3[] path, string kind = "Proximity", float minVelocity = 0,
        bool face = false, Vector3? receiverEuler = null, float rootScale = 1, bool separate = false)
    {
        var c = new Case { Name = name, SenderShape = sender, ReceiverShape = receiver, Kind = kind, MinVelocity = minVelocity, Face = face, Path = path };
        c.Root = new GameObject(name).transform;
        c.Root.position = new Vector3(20f * cases.Count, 0f, 0f);
        c.Root.localScale = Vector3.one * rootScale;
        c.Receiver = new GameObject(name + "/R").transform;
        c.Receiver.SetParent(c.Root, false);
        c.Receiver.localRotation = Quaternion.Euler(receiverEuler ?? Vector3.zero);
        c.Component = c.Receiver;
        if (separate)
        {
            c.Component = new GameObject(name + "/C").transform;
            c.Component.SetParent(c.Root, false);
        }
        c.Sender = new GameObject(name + "/S").transform;
        c.Sender.SetParent(c.Root, false);
        Contact(c, false, c.Sender, c.Sender, sender);
        c.Rx = Contact(c, true, c.Component, c.Receiver, receiver);
        cases.Add(c);
        return c;
    }

    /// `n + 1` points from `a` towards `b`, a quarter step off the round distances where shapes would just touch (float
    /// noise decides those).
    static Vector3[] Line(Vector3 a, Vector3 b, int n) =>
        Enumerable.Range(0, n + 1).Select(i => Vector3.LerpUnclamped(a, b, (i + 0.25f) / n)).ToArray();

    static void Build()
    {
        var sphere = new Shape { Radius = 0.1f };
        var big = new Shape { Radius = 0.5f };
        var along = Line(new Vector3(1f, 0f, 0f), new Vector3(-1f, 0f, 0f), 40); // 0.05 m a frame: 3 m/s
        foreach (var kind in new[] { "Constant", "OnEnter", "Proximity" })
            Add("sphere_" + kind.ToLowerInvariant(), sphere, big, along, kind);
        Add("on_enter_min_2", sphere, big, along, "OnEnter", 2f);
        Add("on_enter_min_4", sphere, big, along, "OnEnter", 4f);
        Add("sphere_sender_big", new Shape { Radius = 0.3f }, new Shape { Radius = 0.2f }, along);
        Add("sphere_offset", sphere, new Shape { Radius = 0.4f, Position = new Vector3(0.1f, 0.2f, 0f) }, Line(new Vector3(1f, 0.2f, 0.05f), new Vector3(-1f, 0.2f, 0.05f), 40));
        Add("sphere_scaled_root", sphere, big, along, rootScale: 2f);

        var capsule = new Shape { Type = "Capsule", Radius = 0.2f, Height = 1f };
        Add("capsule_along_axis", sphere, capsule, Line(new Vector3(0f, 1.2f, 0f), new Vector3(0f, -1.2f, 0f), 48));
        Add("capsule_across", sphere, capsule, Line(new Vector3(0.6f, 0.3f, 0f), new Vector3(-0.6f, 0.3f, 0f), 24));
        Add("capsule_turned", sphere, new Shape { Type = "Capsule", Radius = 0.2f, Height = 1f, Euler = new Vector3(0f, 0f, 90f), Position = new Vector3(0f, 0.1f, 0f) },
            Line(new Vector3(1.2f, 0.1f, 0.05f), new Vector3(-1.2f, 0.1f, 0.05f), 48), receiverEuler: new Vector3(0f, 30f, 0f));
        Add("capsule_short", sphere, new Shape { Type = "Capsule", Radius = 0.3f, Height = 0.4f }, Line(new Vector3(0f, 1f, 0f), new Vector3(0f, -1f, 0f), 40));
        Add("capsule_sender", new Shape { Type = "Capsule", Radius = 0.1f, Height = 0.6f, Euler = new Vector3(0f, 0f, 45f) }, big, along);
        Add("capsule_constant", sphere, capsule, Line(new Vector3(0.6f, 0.45f, 0f), new Vector3(-0.6f, 0.45f, 0f), 24), "Constant");

        var box = new Shape { Type = "Box", Size = new Vector3(0.4f, 0.6f, 0.8f) };
        Add("box_x", sphere, box, along);
        Add("box_y", sphere, box, Line(new Vector3(0.05f, 1f, 0.1f), new Vector3(0.05f, -1f, 0.1f), 40));
        Add("box_z", sphere, box, Line(new Vector3(0.05f, 0.1f, 1f), new Vector3(0.05f, 0.1f, -1f), 40));
        Add("box_diagonal", sphere, box, Line(new Vector3(0.8f, 0.8f, 0.8f), new Vector3(-0.8f, -0.8f, -0.8f), 40));
        Add("box_constant", sphere, box, along, "Constant");
        Add("box_face_z", sphere, box, Line(new Vector3(0.05f, 0.1f, 1f), new Vector3(0.05f, 0.1f, -1f), 40), face: true);
        Add("box_face_x", sphere, box, along, face: true);
        Add("box_face_turned", sphere, new Shape { Type = "Box", Size = new Vector3(0.4f, 0.6f, 0.8f), Euler = new Vector3(0f, 90f, 0f) }, along, face: true, receiverEuler: new Vector3(10f, 0f, 0f));
        Add("sphere_face", sphere, big, along, face: true);
        Add("box_sender", new Shape { Type = "Box", Size = new Vector3(0.2f, 0.2f, 0.6f) }, big, along);
        Add("box_sender_turned", new Shape { Type = "Box", Size = new Vector3(0.6f, 0.1f, 0.1f), Euler = new Vector3(0f, 0f, 45f) }, big, along, "Constant");

        // Box overlap and proximity against other sender shapes, and face proximity at the face's edges and plane.
        Add("box_constant_diagonal", sphere, box, Line(new Vector3(0.8f, 0.8f, 0.8f), new Vector3(-0.8f, -0.8f, -0.8f), 40), "Constant");
        Add("box_capsule_sender", new Shape { Type = "Capsule", Radius = 0.1f, Height = 0.6f, Euler = new Vector3(0f, 0f, 45f) }, box, along);
        Add("box_capsule_sender_diagonal", new Shape { Type = "Capsule", Radius = 0.05f, Height = 0.5f, Euler = new Vector3(30f, 0f, 60f) }, box,
            Line(new Vector3(0.7f, 0.5f, 0.3f), new Vector3(-0.7f, -0.5f, -0.3f), 40));
        Add("box_box_sender", new Shape { Type = "Box", Size = new Vector3(0.2f, 0.1f, 0.3f), Euler = new Vector3(0f, 0f, 30f) }, box,
            Line(new Vector3(0.7f, 0.2f, 0.1f), new Vector3(-0.7f, 0.2f, 0.1f), 40));
        Add("box_box_constant", new Shape { Type = "Box", Size = new Vector3(0.6f, 0.1f, 0.1f), Euler = new Vector3(0f, 0f, 45f) }, box,
            Line(new Vector3(0.8f, 0.1f, 0f), new Vector3(-0.8f, 0.1f, 0f), 40), "Constant");
        Add("face_z_fine", sphere, box, Line(new Vector3(0.05f, 0.1f, 0.6f), new Vector3(0.05f, 0.1f, 0.2f), 40), face: true);
        Add("face_z_outside_x", sphere, box, Line(new Vector3(0.27f, 0f, 0.6f), new Vector3(0.27f, 0f, -0.6f), 40), face: true);
        Add("face_x_near_face", sphere, box, Line(new Vector3(0.4f, 0.1f, 0.35f), new Vector3(-0.4f, 0.1f, 0.35f), 40), face: true);
        Add("face_diagonal", sphere, box, Line(new Vector3(0.4f, 0.5f, 0.6f), new Vector3(-0.4f, -0.5f, -0.6f), 40), face: true);
        Add("face_capsule_sender", new Shape { Type = "Capsule", Radius = 0.05f, Height = 0.4f, Euler = new Vector3(60f, 0f, 0f) }, box,
            Line(new Vector3(0.1f, 0.1f, 0.7f), new Vector3(0.1f, 0.1f, -0.7f), 40), face: true);
        Add("face_box_sender", new Shape { Type = "Box", Size = new Vector3(0.1f, 0.1f, 0.2f), Euler = new Vector3(20f, 0f, 0f) }, box,
            Line(new Vector3(0.1f, 0.1f, 0.7f), new Vector3(0.1f, 0.1f, -0.7f), 40), face: true);
        Add("face_big_sender", new Shape { Radius = 0.3f }, box, Line(new Vector3(0.05f, 0f, 1f), new Vector3(0.05f, 0f, -1f), 40), face: true);

        // The component's node governs enabled state, the root transform placement (#55).
        var still = Enumerable.Repeat(new Vector3(0.2f, 0f, 0f), 30).ToArray();
        var c = Add("component_off", sphere, big, still, separate: true);
        c.ComponentActive = f => f < 10 || f >= 20;
        c = Add("root_off", sphere, big, still, separate: true);
        c.ReceiverActive = f => f < 10 || f >= 20;
        c = Add("component_elsewhere", sphere, big, along, separate: true);
        c.Component.localPosition = new Vector3(0f, 5f, 0f);
    }

    // ---- JSON ----

    static void Write()
    {
        var s = new StringBuilder("{\n  \"unity\": \"" + Application.unityVersion + "\",\n  \"cases\": [");
        for (int k = 0; k < cases.Count; k++)
        {
            var c = cases[k];
            s.Append(k == 0 ? "\n    " : ",\n    ").Append("{\n      \"name\": \"").Append(c.Name).Append("\", \"kind\": \"").Append(c.Kind)
                .Append("\", \"min_velocity\": ").Append(F(c.MinVelocity)).Append(", \"face\": ").Append(B(c.Face))
                .Append(", \"root_scale\": ").Append(F(c.Root.localScale.x))
                .Append(",\n      \"sender_shape\": ").Append(ShapeJson(c.SenderShape)).Append(",\n      \"receiver_shape\": ").Append(ShapeJson(c.ReceiverShape))
                .Append(",\n      \"frames\": [\n        ").Append(string.Join(",\n        ", c.Frames)).Append("\n      ]\n    }");
        }
        File.WriteAllText(outPath, s.Append("\n  ]\n}\n").ToString());
        Debug.Log($"CIVBB vrc contacts recorded {cases.Count} cases to {outPath}");
    }

    static string ShapeJson(Shape s) => "{\"type\": \"" + s.Type + "\", \"radius\": " + F(s.Radius) + ", \"height\": " + F(s.Height) + ", \"size\": " +
                                        V(s.Size) + ", \"position\": " + V(s.Position) + ", \"rotation\": " + Q(Quaternion.Euler(s.Euler)) + "}";

    /// World pose: position, rotation, uniform scale.
    static string Pose(Transform t) => "{\"position\": " + V(t.position) + ", \"rotation\": " + Q(t.rotation) + ", \"scale\": " + F(t.lossyScale.x) + "}";

    static string B(bool b) => b ? "true" : "false";
    static string F(float x) => x.ToString("R", CultureInfo.InvariantCulture);
    static string V(Vector3 v) => "[" + F(v.x) + ", " + F(v.y) + ", " + F(v.z) + "]";
    static string Q(Quaternion q) => "[" + F(q.x) + ", " + F(q.y) + ", " + F(q.z) + ", " + F(q.w) + "]";
}
