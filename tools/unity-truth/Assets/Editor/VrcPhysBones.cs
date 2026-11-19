// Records the VRChat SDK's own PhysBone runtime as ground truth for civbb-dynamics' chain solver (ADR 0015, #75): a
// chain of bones on a root that moves along a scripted path, one step per frame, with each frame's bone positions
// recorded. Like VrcContacts.cs it needs the SDK: copy it into corpus/local/unity-export-vrc/Assets/Editor and run it
// there; it reaches the SDK only by reflection and every case is generated, so the output may be committed. PhysBones
// run in play mode: run it without -quit; it exits by itself.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using UnityEditor;
using UnityEngine;

public static class VrcPhysBones
{
    const float Dt = 1f / 60f;
    const int Frames = 90;
    const int Bones = 4;
    const float BoneLength = 0.1f;

    class Case
    {
        public string Name;
        public Transform Root;
        public Transform[] Bones;
        public Component Chain;
        public Func<int, (Vector3 position, Quaternion rotation)> Path;
        public readonly Dictionary<string, object> Settings = new Dictionary<string, object>();
        public readonly List<string> Colliders = new List<string>();
        public readonly List<string> Frames = new List<string>();
        public string Pending;
    }

    static readonly List<Case> cases = new List<Case>();
    static string outPath;
    static int tick;

    /// `-executeMethod VrcPhysBones.Record -civbbOut <absolute json path>`.
    public static void Record()
    {
        try
        {
            outPath = Environment.GetCommandLineArgs().SkipWhile(a => a != "-civbbOut").Skip(1).First();
            var pb = Type("VRCPhysBone");
            Debug.Log("CIVBB vrc physbones: " + string.Join("; ", new[] { "version", "integrationType", "limitType", "immobileType", "allowCollision", "multiChildType" }
                .Select(f => f + " " + string.Join(",", Enum.GetNames(Field(pb, f).FieldType)))) +
                "; collider shapes " + string.Join(",", Enum.GetNames(Field(Type("VRCPhysBoneCollider"), "shapeType").FieldType)));
            Build();
            foreach (var c in cases)
                Place(c, 0);
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

    static Type Type(string name) => AppDomain.CurrentDomain.GetAssemblies()
        .Select(a => a.GetType("VRC.SDK3.Dynamics.PhysBone.Components." + name)).FirstOrDefault(t => t != null) ?? throw new TypeLoadException(name);

    static FieldInfo Field(Type t, string name) =>
        t.GetField(name, BindingFlags.Public | BindingFlags.Instance) ?? throw new MissingFieldException(t.Name, name);

    static void Set(Component k, string name, object value)
    {
        var f = Field(k.GetType(), name);
        f.SetValue(k, f.FieldType.IsEnum && value is string s ? Enum.Parse(f.FieldType, s) : value);
    }

    // Tick n (from 1) reads what frame n - 2 produced and sets frame n - 1's inputs, as in VrcContacts.cs.
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
            foreach (var c in cases)
            {
                if (c.Pending != null)
                {
                    c.Frames.Add(c.Pending + ", \"dt\": " + F(Time.deltaTime) + ", \"bones\": [" + string.Join(", ", c.Bones.Select(b => V(b.position))) + "]}");
                    c.Pending = null;
                }
                if (frame >= Frames)
                    continue;
                if (frame > 0)
                    Place(c, frame);
                c.Pending = "{\"root\": {\"position\": " + V(c.Root.position) + ", \"rotation\": " + Q(c.Root.rotation) + "}";
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

    static void Place(Case c, int f)
    {
        var (p, r) = c.Path(f);
        c.Root.position = p + new Vector3(20f * cases.IndexOf(c), 0f, 0f);
        c.Root.rotation = r;
    }

    // ---- Cases ----

    /// A case at its own place in the scene: Root, then `Bones` bones of `BoneLength` along +X, the PhysBone on the
    /// first; settings are field name to value (enum values by name) on top of the defaults below.
    static Case Add(string name, Func<int, (Vector3, Quaternion)> path, params (string field, object value)[] settings)
    {
        var c = new Case { Name = name, Path = path };
        c.Root = new GameObject(name).transform;
        c.Bones = new Transform[Bones + 1];
        var parent = c.Root;
        for (int i = 0; i <= Bones; i++)
        {
            c.Bones[i] = new GameObject(name + "/B" + i).transform;
            c.Bones[i].SetParent(parent, false);
            c.Bones[i].localPosition = i == 0 ? Vector3.zero : new Vector3(BoneLength, 0f, 0f);
            parent = c.Bones[i];
        }
        var k = c.Bones[0].gameObject.AddComponent(Type("VRCPhysBone"));
        var defaults = new (string, object)[]
        {
            ("version", "Version_1_1"), ("integrationType", "Simplified"), ("pull", 0.2f), ("spring", 0.2f), ("stiffness", 0.2f),
            ("gravity", 0f), ("gravityFalloff", 0f), ("immobileType", "AllMotion"), ("immobile", 0f), ("limitType", "None"),
            ("maxAngleX", 45f), ("maxAngleZ", 45f), ("limitRotation", Vector3.zero), ("radius", 0f), ("endpointPosition", Vector3.zero),
            ("maxStretch", 0f), ("maxSquish", 0f), ("stretchMotion", 0f), ("allowCollision", "True"),
        };
        foreach (var (field, value) in defaults.Concat(settings))
        {
            c.Settings[field] = value;
            Set(k, field, value);
        }
        Set(k, "rootTransform", c.Bones[0]);
        c.Chain = k;
        cases.Add(c);
        return c;
    }

    static void Collide(Case c, string shape, float radius, float height, Vector3 position, Vector3 euler, bool inside = false)
    {
        var host = new GameObject(c.Name + "/Collider").transform;
        host.SetParent(c.Root, false);
        var k = host.gameObject.AddComponent(Type("VRCPhysBoneCollider"));
        Set(k, "shapeType", shape);
        Set(k, "radius", radius);
        Set(k, "height", height);
        Set(k, "position", position);
        Set(k, "rotation", Quaternion.Euler(euler));
        Set(k, "insideBounds", inside);
        Set(k, "rootTransform", host);
        k.GetType().GetMethod("ApplyConfigurationChanges")?.Invoke(k, null);
        var list = (System.Collections.IList)Field(c.Chain.GetType(), "colliders").GetValue(c.Chain);
        list.Add(k);
        c.Colliders.Add("{\"shape\": \"" + shape + "\", \"radius\": " + F(radius) + ", \"height\": " + F(height) + ", \"position\": " + V(position) +
                        ", \"rotation\": " + Q(Quaternion.Euler(euler)) + ", \"inside\": " + (inside ? "true" : "false") + "}");
    }

    static (Vector3, Quaternion) Still(int f) => (Vector3.zero, Quaternion.identity);

    /// Across the chain (Z), ±0.2 m with a one-second period for a second, then still.
    static (Vector3, Quaternion) Sway(int f) => (new Vector3(0f, 0f, f < 60 ? 0.2f * Mathf.Sin(2f * Mathf.PI * f / 60f) : 0f), Quaternion.identity);

    /// A quarter turn about Y over half a second, then still.
    static (Vector3, Quaternion) Turn(int f) => (Vector3.zero, Quaternion.Euler(0f, 90f * Mathf.Clamp01(f / 30f), 0f));

    /// Up along Y by 0.3 m over a quarter second, then still (immobile against translation).
    static (Vector3, Quaternion) Lift(int f) => (new Vector3(0f, 0.3f * Mathf.Clamp01(f / 15f), 0f), Quaternion.identity);

    static void Build()
    {
        foreach (var version in new[] { "Version_1_0", "Version_1_1" })
            foreach (var integration in new[] { "Simplified", "Advanced" })
            {
                string tag = (version == "Version_1_0" ? "v10" : "v11") + "_" + integration.ToLowerInvariant();
                var common = new (string, object)[] { ("version", version), ("integrationType", integration) };
                Add("sag_" + tag, Still, common.Concat(new (string, object)[] { ("pull", 0.2f), ("spring", 0f), ("gravity", 0.5f) }).ToArray());
                Add("sway_" + tag, Sway, common.Concat(new (string, object)[] { ("pull", 0.2f), ("spring", 0.5f) }).ToArray());
                Add("turn_" + tag, Turn, common.Concat(new (string, object)[] { ("pull", 0.3f), ("spring", 0.3f), ("gravity", 0.2f) }).ToArray());
                Add("stiff_" + tag, Sway, common.Concat(new (string, object)[] { ("pull", 0.1f), ("spring", 0.5f), ("stiffness", 0.8f) }).ToArray());
            }
        // One parameter at a time on version 1.1, simplified unless it needs advanced.
        Add("pull_high", Sway, ("pull", 0.8f), ("spring", 0f));
        Add("pull_zero_gravity", Still, ("pull", 0f), ("spring", 0f), ("gravity", 1f));
        Add("falloff", Still, ("pull", 0.2f), ("spring", 0f), ("gravity", 0.5f), ("gravityFalloff", 0.5f));
        Add("gravity_negative", Still, ("pull", 0.2f), ("spring", 0f), ("gravity", -0.5f));
        Add("immobile_all", Lift, ("pull", 0.2f), ("spring", 0.3f), ("immobile", 0.5f));
        Add("immobile_world", Lift, ("pull", 0.2f), ("spring", 0.3f), ("immobile", 0.5f), ("immobileType", "World"));
        Add("immobile_world_turn", Turn, ("pull", 0.2f), ("spring", 0.3f), ("immobile", 0.5f), ("immobileType", "World"));
        Add("limit_angle", Still, ("pull", 0f), ("spring", 0f), ("gravity", 1f), ("limitType", "Angle"), ("maxAngleX", 20f));
        Add("limit_hinge", Sway, ("pull", 0f), ("spring", 0.2f), ("gravity", 1f), ("limitType", "Hinge"), ("maxAngleX", 30f));
        Add("limit_polar", Still, ("pull", 0f), ("spring", 0f), ("gravity", 1f), ("limitType", "Polar"), ("maxAngleX", 20f), ("maxAngleZ", 40f),
            ("limitRotation", new Vector3(0f, 0f, 15f)));
        Add("endpoint", Sway, ("pull", 0.2f), ("spring", 0.5f), ("endpointPosition", new Vector3(0.1f, 0f, 0f)));
        var c = Add("collider_sphere", Still, ("pull", 0f), ("spring", 0f), ("gravity", 1f), ("radius", 0.02f));
        Collide(c, "Sphere", 0.05f, 0f, new Vector3(0.22f, -0.12f, 0f), Vector3.zero);
        c = Add("collider_capsule", Still, ("pull", 0f), ("spring", 0f), ("gravity", 1f), ("radius", 0.02f));
        Collide(c, "Capsule", 0.03f, 0.3f, new Vector3(0.25f, -0.1f, 0f), new Vector3(90f, 0f, 0f));
        c = Add("collider_plane", Still, ("pull", 0f), ("spring", 0f), ("gravity", 1f), ("radius", 0.02f));
        Collide(c, "Plane", 0f, 0f, new Vector3(0f, -0.15f, 0f), Vector3.zero);
    }

    // ---- JSON ----

    static void Write()
    {
        var s = new StringBuilder("{\n  \"unity\": \"" + Application.unityVersion + "\", \"bone_length\": " + F(BoneLength) + ", \"cases\": [");
        for (int k = 0; k < cases.Count; k++)
        {
            var c = cases[k];
            var settings = c.Settings.Select(kv => "\"" + kv.Key + "\": " + Value(kv.Value));
            s.Append(k == 0 ? "\n    " : ",\n    ").Append("{\n      \"name\": \"").Append(c.Name).Append("\",\n      \"settings\": {")
                .Append(string.Join(", ", settings)).Append("},\n      \"colliders\": [").Append(string.Join(", ", c.Colliders))
                .Append("],\n      \"frames\": [\n        ").Append(string.Join(",\n        ", c.Frames)).Append("\n      ]\n    }");
        }
        File.WriteAllText(outPath, s.Append("\n  ]\n}\n").ToString());
        Debug.Log($"CIVBB vrc physbones recorded {cases.Count} cases to {outPath}");
    }

    static string Value(object v) => v is float f ? F(f) : v is Vector3 x ? V(x) : "\"" + v + "\"";

    static string F(float x) => x.ToString("R", CultureInfo.InvariantCulture);
    static string V(Vector3 v) => "[" + F(v.x) + ", " + F(v.y) + ", " + F(v.z) + "]";
    static string Q(Quaternion q) => "[" + F(q.x) + ", " + F(q.y) + ", " + F(q.z) + ", " + F(q.w) + "]";
}
