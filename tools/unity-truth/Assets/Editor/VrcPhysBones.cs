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
    /// Seconds recorded per case.
    const float Duration = 1.5f;
    const int Bones = 4;
    const float BoneLength = 0.1f;
    /// Cases on a grid this far apart, near the origin (float precision).
    const float Spacing = 5f;
    const int Columns = 10;

    /// The frame time (`-civbbDt <seconds>`, default 1/60); other rates record only the cases marked `Rates`.
    static float Dt = 1f / 60f;
    static int Frames = 90;

    class Case
    {
        public string Name;
        public bool Rates;
        public GameObject Host;
        public Transform Root;
        public Transform[] Bones;
        public Component Chain;
        public Func<float, (Vector3 position, Quaternion rotation)> Path;
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
            var args = Environment.GetCommandLineArgs();
            outPath = args.SkipWhile(a => a != "-civbbOut").Skip(1).First();
            var dt = args.SkipWhile(a => a != "-civbbDt").Skip(1).FirstOrDefault();
            if (dt != null)
                Dt = float.Parse(dt, CultureInfo.InvariantCulture);
            Frames = Mathf.RoundToInt(Duration / Dt);
            var pb = Type("VRCPhysBone");
            Debug.Log("CIVBB vrc physbones: " + string.Join("; ", new[] { "version", "integrationType", "limitType", "immobileType", "allowCollision", "multiChildType" }
                .Select(f => f + " " + string.Join(",", Enum.GetNames(Field(pb, f).FieldType)))) +
                "; collider shapes " + string.Join(",", Enum.GetNames(Field(Type("VRCPhysBoneCollider"), "shapeType").FieldType)));
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

    // Tick 1 builds the cases in play mode, so their first step already runs at the pinned frame time (built in edit
    // mode, the first play-mode frame stepped them at an unpinned one). Tick n (from 2) reads what frame n - 3
    // produced and sets frame n - 2's inputs, as in VrcContacts.cs.
    static void Tick()
    {
        if (!EditorApplication.isPlaying)
            return;
        var done = false;
        try
        {
            Time.captureDeltaTime = Dt;
            tick++;
            if (tick == 1)
            {
                Build();
                foreach (var c in cases)
                {
                    Place(c, 0);
                    c.Host.SetActive(true);
                }
                return;
            }
            int frame = tick - 2;
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
                c.Pending = "{\"root\": {\"position\": " + V(c.Root.position) + ", \"rotation\": " + Q(c.Root.rotation) + ", \"scale\": " +
                            F(c.Root.localScale.x) + "}";
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
        var (p, r) = c.Path(f * Dt);
        int i = cases.IndexOf(c);
        c.Root.position = p + new Vector3(Spacing * (i % Columns), 0f, Spacing * (i / Columns));
        c.Root.rotation = r;
    }

    // ---- Cases ----

    /// A case at its own place in the scene: Root, then `Bones` bones of `BoneLength` along +X (or the `boneAxis`
    /// setting), the PhysBone on the first, turned by the `rootRotation` setting (Euler); settings are field name to
    /// value (enum values by name) on top of the defaults below, those two not PhysBone fields.
    static Case Add(string name, Func<float, (Vector3, Quaternion)> path, params (string field, object value)[] settings)
    {
        var c = new Case { Name = name, Path = path, Rates = RateCases.Contains(name) };
        // Inactive until configured: the SDK reads the settings when the component is enabled.
        c.Host = new GameObject(name);
        c.Host.SetActive(false);
        c.Root = c.Host.transform;
        c.Bones = new Transform[Bones + 1];
        var axis = settings.Where(s => s.field == "boneAxis").Select(s => (Vector3)s.value).DefaultIfEmpty(Vector3.right).First();
        var parent = c.Root;
        for (int i = 0; i <= Bones; i++)
        {
            c.Bones[i] = new GameObject(name + "/B" + i).transform;
            c.Bones[i].SetParent(parent, false);
            c.Bones[i].localPosition = i == 0 ? Vector3.zero : axis * BoneLength;
            parent = c.Bones[i];
        }
        c.Bones[0].localRotation = Quaternion.Euler(settings.Where(s => s.field == "rootRotation").Select(s => (Vector3)s.value)
            .DefaultIfEmpty(Vector3.zero).First());
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
            if (field != "boneAxis" && field != "rootRotation")
            {
                Set(k, field, value);
            }
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

    // Paths by time in seconds.
    static (Vector3, Quaternion) Still(float t) => (Vector3.zero, Quaternion.identity);

    /// Across the chain (Z), ±0.2 m with a one-second period for a second, then still.
    static (Vector3, Quaternion) Sway(float t) => (new Vector3(0f, 0f, t < 1f ? 0.2f * Mathf.Sin(2f * Mathf.PI * t) : 0f), Quaternion.identity);

    /// A quarter turn about Y over half a second, then still.
    static (Vector3, Quaternion) Turn(float t) => (Vector3.zero, Quaternion.Euler(0f, 90f * Mathf.Clamp01(t / 0.5f), 0f));

    /// Up along Y by 0.3 m over a quarter second, then still (immobile against translation).
    static (Vector3, Quaternion) Lift(float t) => (new Vector3(0f, 0.3f * Mathf.Clamp01(t / 0.25f), 0f), Quaternion.identity);

    /// Cases also recorded at other frame times.
    static readonly HashSet<string> RateCases = new HashSet<string>
    {
        "sag_v10_simplified", "sag_v11_simplified", "sway_v11_simplified", "sway_v11_advanced", "turn_v11_advanced", "stiff_v11_advanced",
        "immobile_all", "simplified_p40_s70", "advanced_p20_m50_k50", "limit_angle", "collider_sphere", "falloff_v10_100", "falloff_v11_50",
        "gravity_v11_sway", "immobile_advanced", "collider_sway", "limit_angle_sway", "scaled",
    };

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
        Add("immobile_all_turn", Turn, ("pull", 0.2f), ("spring", 0.3f), ("immobile", 0.5f));
        Add("immobile_advanced", Lift, ("integrationType", "Advanced"), ("pull", 0.2f), ("spring", 0.5f), ("stiffness", 0.3f), ("immobile", 0.5f));
        Add("immobile_advanced_turn", Turn, ("integrationType", "Advanced"), ("pull", 0.2f), ("spring", 0.5f), ("stiffness", 0.3f), ("immobile", 0.5f));
        // Scale: the whole case (chain and collider) at twice the size.
        var big = Add("scaled", Sway, ("pull", 0.2f), ("spring", 0.5f), ("gravity", 0.3f), ("radius", 0.01f));
        Collide(big, "Sphere", 0.04f, 0f, new Vector3(0.25f, -0.05f, 0.09f), Vector3.zero);
        big.Root.localScale = Vector3.one * 2f;
        big = Add("scaled_v10_advanced", Sway, ("version", "Version_1_0"), ("integrationType", "Advanced"), ("pull", 0.2f), ("spring", 0.5f),
            ("stiffness", 0.3f), ("gravity", 0.3f));
        big.Root.localScale = Vector3.one * 2f;
        // Limits and colliders need pull: with pull 0 the SDK leaves the chain where it is.
        Add("limit_angle", Still, ("pull", 0.2f), ("spring", 0f), ("gravity", 1f), ("limitType", "Angle"), ("maxAngleX", 20f));
        Add("limit_angle_rotated", Still, ("pull", 0.2f), ("spring", 0f), ("gravity", 1f), ("limitType", "Angle"), ("maxAngleX", 20f),
            ("limitRotation", new Vector3(0f, 30f, 0f)));
        Add("limit_hinge", Sway, ("pull", 0.2f), ("spring", 0.2f), ("gravity", 1f), ("limitType", "Hinge"), ("maxAngleX", 30f));
        Add("limit_hinge_rotated", Sway, ("pull", 0.2f), ("spring", 0.2f), ("gravity", 1f), ("limitType", "Hinge"), ("maxAngleX", 30f),
            ("limitRotation", new Vector3(0f, 90f, 0f)));
        Add("limit_polar", Still, ("pull", 0.2f), ("spring", 0f), ("gravity", 1f), ("limitType", "Polar"), ("maxAngleX", 20f), ("maxAngleZ", 40f),
            ("limitRotation", new Vector3(0f, 0f, 15f)));
        Add("limit_polar_sway", Sway, ("pull", 0.2f), ("spring", 0.3f), ("limitType", "Polar"), ("maxAngleX", 10f), ("maxAngleZ", 30f));
        Add("limit_polar_diagonal", Sway, ("pull", 0.2f), ("spring", 0.3f), ("gravity", 0.6f), ("limitType", "Polar"), ("maxAngleX", 20f),
            ("maxAngleZ", 40f));
        Add("limit_angle_sway", Sway, ("pull", 0.2f), ("spring", 0.5f), ("gravity", 0.6f), ("limitType", "Angle"), ("maxAngleX", 25f));
        Add("endpoint", Sway, ("pull", 0.2f), ("spring", 0.5f), ("endpointPosition", new Vector3(0.1f, 0f, 0f)));
        var c = Add("collider_sphere", Still, ("pull", 0.2f), ("spring", 0f), ("gravity", 1f), ("radius", 0.02f));
        Collide(c, "Sphere", 0.05f, 0f, new Vector3(0.22f, -0.12f, 0f), Vector3.zero);
        c = Add("collider_capsule", Still, ("pull", 0.2f), ("spring", 0f), ("gravity", 1f), ("radius", 0.02f));
        Collide(c, "Capsule", 0.03f, 0.3f, new Vector3(0.25f, -0.1f, 0f), new Vector3(90f, 0f, 0f));
        c = Add("collider_plane", Still, ("pull", 0.2f), ("spring", 0f), ("gravity", 1f), ("radius", 0.02f));
        Collide(c, "Plane", 0f, 0f, new Vector3(0f, -0.15f, 0f), Vector3.zero);
        c = Add("collider_inside", Still, ("pull", 0.2f), ("spring", 0f), ("gravity", 1f), ("radius", 0.02f));
        Collide(c, "Sphere", 0.25f, 0f, new Vector3(0.1f, 0f, 0f), Vector3.zero, inside: true);
        // Inside capsules (#90): along the chain, bone radius below and above the capsule's, the tip starting outside.
        foreach (var (name, boneRadius, height) in new[] { ("thin", 0.02f, 0.6f), ("fat", 0.08f, 0.6f), ("short", 0.02f, 0.3f), ("short_fat", 0.08f, 0.3f) })
        {
            c = Add("collider_inside_capsule_" + name, Still, ("pull", 0.2f), ("spring", 0f), ("gravity", 1f), ("radius", boneRadius));
            Collide(c, "Capsule", 0.06f, height, new Vector3(height / 2f - 0.05f, 0f, 0f), new Vector3(0f, 0f, 90f), inside: true);
        }
        c = Add("collider_sway", Sway, ("pull", 0.2f), ("spring", 0.5f), ("radius", 0.01f));
        Collide(c, "Sphere", 0.04f, 0f, new Vector3(0.25f, 0f, 0.09f), Vector3.zero);
        // Sweeps, one force at a time: spring by pull (simplified), momentum by stiffness (advanced), gravity by pull
        // (both versions, still), falloff.
        foreach (var pull in new[] { 0.1f, 0.4f })
            foreach (var spring in new[] { 0f, 0.3f, 0.7f, 1f })
                Add($"simplified_p{Pct(pull)}_s{Pct(spring)}", Sway, ("pull", pull), ("spring", spring));
        foreach (var momentum in new[] { 0f, 0.5f, 1f })
            foreach (var stiffness in new[] { 0f, 0.5f, 1f })
                Add($"advanced_p20_m{Pct(momentum)}_k{Pct(stiffness)}", Sway, ("integrationType", "Advanced"), ("pull", 0.2f), ("spring", momentum),
                    ("stiffness", stiffness));
        Add("advanced_p50_m50_k0", Sway, ("integrationType", "Advanced"), ("pull", 0.5f), ("spring", 0.5f), ("stiffness", 0f));
        Add("advanced_p50_m50_k100", Sway, ("integrationType", "Advanced"), ("pull", 0.5f), ("spring", 0.5f), ("stiffness", 1f));
        foreach (var version in new[] { "Version_1_0", "Version_1_1" })
        {
            string v = version == "Version_1_0" ? "v10" : "v11";
            foreach (var pull in new[] { 0.2f, 0.6f })
                foreach (var gravity in new[] { 0.25f, 1f, -0.5f })
                    Add($"gravity_{v}_p{Pct(pull)}_g{(gravity < 0 ? "m" : "")}{Pct(Mathf.Abs(gravity))}", Still, ("version", version), ("pull", pull),
                        ("spring", 0f), ("gravity", gravity));
            Add($"gravity_{v}_sway", Sway, ("version", version), ("pull", 0.2f), ("spring", 0.5f), ("gravity", 0.5f));
            Add($"gravity_{v}_advanced", Sway, ("version", version), ("integrationType", "Advanced"), ("pull", 0.2f), ("spring", 0.5f),
                ("stiffness", 0.5f), ("gravity", 0.5f));
            foreach (var falloff in new[] { 0.5f, 1f })
                Add($"falloff_{v}_{Pct(falloff)}", Still, ("version", version), ("pull", 0.2f), ("spring", 0f), ("gravity", 0.5f),
                    ("gravityFalloff", falloff));
        }
        // Scaled bones against unscaled colliders (#90): which scale the bone radius takes. Last, so earlier cases
        // keep their place on the grid.
        c = Add("collider_inside_capsule_chain_half", Still, ("pull", 0.2f), ("spring", 0f), ("gravity", 1f), ("radius", 0.08f));
        Collide(c, "Capsule", 0.06f, 0.6f, new Vector3(0.25f, 0f, 0f), new Vector3(0f, 0f, 90f), inside: true);
        c.Bones[0].localScale = Vector3.one * 0.5f;
        c = Add("collider_inside_sphere_mid_double", Still, ("pull", 0.2f), ("spring", 0f), ("gravity", 1f), ("radius", 0.1f));
        Collide(c, "Sphere", 0.5f, 0f, new Vector3(0.2f, 0f, 0f), Vector3.zero, inside: true);
        c.Bones[2].localScale = Vector3.one * 2f;
        // Only the leaf scaled: its own scale or its parent's.
        c = Add("collider_inside_sphere_leaf_triple", Still, ("pull", 0.2f), ("spring", 0f), ("gravity", 1f), ("radius", 0.05f));
        Collide(c, "Sphere", 0.4f, 0f, new Vector3(0.1f, 0f, 0f), Vector3.zero, inside: true);
        c.Bones[4].localScale = Vector3.one * 3f;
        // Polar limits on a chain along its bones' own +Y, rotated about X and Z (Yumeka's skirt and hair ribbons, #108).
        Add("limit_polar_up_rotx", Still, ("boneAxis", Vector3.up), ("pull", 0.2f), ("spring", 0f), ("gravity", 1f), ("limitType", "Polar"),
            ("maxAngleX", 40f), ("maxAngleZ", 0f), ("limitRotation", new Vector3(-40f, 0f, 0f)));
        Add("limit_polar_up_rotx_sway", Sway, ("boneAxis", Vector3.up), ("pull", 0.2f), ("spring", 0.3f), ("gravity", 0.1f),
            ("limitType", "Polar"), ("maxAngleX", 40f), ("maxAngleZ", 10f), ("limitRotation", new Vector3(-40f, 0f, 0f)));
        Add("limit_polar_up_rotxz", Sway, ("boneAxis", Vector3.up), ("pull", 0.2f), ("spring", 0.5f), ("gravity", 0.3f),
            ("limitType", "Polar"), ("maxAngleX", 60f), ("maxAngleZ", 90f), ("limitRotation", new Vector3(-54.3f, 0f, -69.3f)));
        Add("limit_polar_x_rotx", Still, ("pull", 0.2f), ("spring", 0f), ("gravity", 1f), ("limitType", "Polar"),
            ("maxAngleX", 40f), ("maxAngleZ", 0f), ("limitRotation", new Vector3(-40f, 0f, 0f)));
        // All three limit rotation angles, so the composition order shows.
        Add("limit_polar_up_rotxyz", Sway, ("boneAxis", Vector3.up), ("pull", 0.2f), ("spring", 0.5f), ("gravity", 0.3f),
            ("limitType", "Polar"), ("maxAngleX", 30f), ("maxAngleZ", 50f), ("limitRotation", new Vector3(-30f, 25f, 50f)));
        Add("limit_angle_up_rotxyz", Sway, ("boneAxis", Vector3.up), ("pull", 0.2f), ("spring", 0.5f), ("gravity", 0.3f),
            ("limitType", "Angle"), ("maxAngleX", 20f), ("limitRotation", new Vector3(-30f, 25f, 50f)));
        Add("limit_hinge_up_rotxyz", Sway, ("boneAxis", Vector3.up), ("pull", 0.2f), ("spring", 0.5f), ("gravity", 0.3f),
            ("limitType", "Hinge"), ("maxAngleX", 30f), ("limitRotation", new Vector3(-30f, 25f, 50f)));
        // Yumeka's front skirt: hanging 9 degrees forward, the limit's rotation and its edge at the rest direction.
        Add("yumeka_skirt", Still, ("boneAxis", Vector3.up), ("rootRotation", new Vector3(9.143f, -180f, -180f)), ("pull", 0.146f),
            ("spring", 0.165f), ("stiffness", 0.2f), ("gravity", 0.08f), ("immobile", 0.85f), ("radius", 0.049f), ("limitType", "Polar"),
            ("maxAngleX", 40f), ("maxAngleZ", 0f), ("limitRotation", new Vector3(-40f, 0f, 0f)));
    }

    static string Pct(float x) => Mathf.RoundToInt(x * 100f).ToString(CultureInfo.InvariantCulture);

    // ---- JSON ----

    static void Write()
    {
        var s = new StringBuilder("{\n  \"unity\": \"" + Application.unityVersion + "\", \"bone_length\": " + F(BoneLength) + ", \"dt\": " + F(Dt) +
                                  ", \"cases\": [");
        var written = Mathf.Approximately(Dt, 1f / 60f) ? cases : cases.Where(c => c.Rates).ToList();
        for (int k = 0; k < written.Count; k++)
        {
            var c = written[k];
            var settings = c.Settings.Select(kv => "\"" + kv.Key + "\": " + Value(kv.Value));
            s.Append(k == 0 ? "\n    " : ",\n    ").Append("{\n      \"name\": \"").Append(c.Name).Append("\",\n      \"settings\": {")
                .Append(string.Join(", ", settings)).Append("},\n      \"bone_scales\": [").Append(string.Join(", ", c.Bones.Select(b => F(b.localScale.x))))
                .Append("],\n      \"colliders\": [").Append(string.Join(", ", c.Colliders))
                .Append("],\n      \"frames\": [\n        ").Append(string.Join(",\n        ", c.Frames)).Append("\n      ]\n    }");
        }
        File.WriteAllText(outPath, s.Append("\n  ]\n}\n").ToString());
        Debug.Log($"CIVBB vrc physbones recorded {written.Count} cases to {outPath}");
    }

    static string Value(object v) => v is float f ? F(f) : v is Vector3 x ? V(x) : "\"" + v + "\"";

    static string F(float x) => x.ToString("R", CultureInfo.InvariantCulture);
    /// Positions to the micrometre, which keeps the goldens small.
    static string P(float x) => x.ToString("0.######", CultureInfo.InvariantCulture);
    static string V(Vector3 v) => "[" + P(v.x) + ", " + P(v.y) + ", " + P(v.z) + "]";
    static string Q(Quaternion q) => "[" + F(q.x) + ", " + F(q.y) + ", " + F(q.z) + ", " + F(q.w) + "]";
}
