// VRChat components (ADR 0042): avatar descriptor (playable layers, viewpoint, visemes, blink, eye look,
// expressions), PhysBones and their colliders, contacts and VRChat constraints, as format JSON in glTF space. Read
// by type name and serialized field names through SerializedObject, so the exporter links no VRChat SDK code and
// compiles without it (the components are then missing scripts and nothing here finds them).
using System;
using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEngine;
using static Civbb.Exporter.Json;

namespace Civbb.Exporter
{
    sealed class VrcExport
    {
        const string AvatarsNs = "VRC.SDK3.Avatars.Components.", DynamicsNs = "VRC.SDK3.Dynamics.";

        /// The playable layers in VRChat's order; groups keep these names even when empty.
        static readonly string[] Playable = { "Base", "Additive", "Gesture", "Action", "FX" };

        /// The 15 visemes in VRChat's descriptor order, which is the format's `Viseme` order.
        static readonly string[] Visemes = { "Sil", "Pp", "Ff", "Th", "Dd", "Kk", "Ch", "Ss", "Nn", "Rr", "Aa", "E", "I", "O", "U" };

        readonly Transform root;
        readonly Func<Transform, int> node;
        readonly SerializedObject descriptor;
        readonly List<string> notes = new List<string>();
        readonly Dictionary<string, int> skipped = new Dictionary<string, int>();

        public VrcExport(Transform root, Func<Transform, int> node)
        {
            this.root = root;
            this.node = node;
            var d = Components(AvatarsNs + "VRCAvatarDescriptor").FirstOrDefault();
            descriptor = d != null ? new SerializedObject(d) : null;
        }

        public bool HasDescriptor => descriptor != null;

        public string Report() => descriptor == null ? "no VRChat descriptor"
            : $"VRChat: {string.Join(", ", notes)}" + (skipped.Count > 0
                ? $"; skipped {string.Join(", ", skipped.Select(s => $"{s.Key} {s.Value}"))}" : "");

        void Skip(string what) => skipped[what] = skipped.TryGetValue(what, out int n) ? n + 1 : 1;

        IEnumerable<Component> Components(string type) => root.GetComponentsInChildren<Component>(true)
            .Where(c => c != null && c.GetType().FullName == type);

        /// A serialized property that must exist; a missing one means an SDK version this exporter does not know.
        static SerializedProperty P(SerializedObject so, string path) =>
            so.FindProperty(path) ?? throw new MissingFieldException(so.targetObject.GetType().Name, path);

        static SerializedProperty P(SerializedProperty p, string path) =>
            p.FindPropertyRelative(path) ?? throw new MissingFieldException(p.propertyPath, path);

        static string EnumName(SerializedProperty p) =>
            p.enumValueIndex >= 0 && p.enumValueIndex < p.enumNames.Length ? p.enumNames[p.enumValueIndex] : "";

        /// The component's own transform when a root transform field is empty.
        static Transform Or(SerializedProperty p, Component c) => p.objectReferenceValue as Transform ?? c.transform;

        /// Playable layer controllers from the descriptor: `(group, controller asset path or null)` in VRChat's
        /// order. Default layers and SDK-owned controllers stay empty (ADR 0042); special layers are left out.
        public List<(string group, string path)> Layers()
        {
            var groups = Playable.ToDictionary(g => g, g => (string)null);
            if (descriptor == null)
            {
                return new List<(string, string)>();
            }
            bool custom = P(descriptor, "customizeAnimationLayers").boolValue;
            var layers = P(descriptor, "baseAnimationLayers");
            int used = 0, defaults = 0, masks = 0;
            for (int i = 0; i < layers.arraySize; i++)
            {
                var l = layers.GetArrayElementAtIndex(i);
                var type = EnumName(P(l, "type"));
                var controller = P(l, "animatorController").objectReferenceValue;
                if (!groups.ContainsKey(type))
                {
                    continue;
                }
                if (!custom || P(l, "isDefault").boolValue || controller == null || AnimatorExport.SdkAsset(controller))
                {
                    defaults++;
                    continue;
                }
                masks += P(l, "mask").objectReferenceValue != null ? 1 : 0;
                groups[type] = AssetDatabase.GetAssetPath(controller);
                used++;
            }
            int special = 0;
            var specials = P(descriptor, "specialAnimationLayers");
            for (int i = 0; i < specials.arraySize; i++)
            {
                var l = specials.GetArrayElementAtIndex(i);
                special += custom && !P(l, "isDefault").boolValue && P(l, "animatorController").objectReferenceValue != null ? 1 : 0;
            }
            notes.Add($"{used} playable layers ({defaults} default, {masks} masks and {special} special layers ignored)");
            return Playable.Select(g => (g, groups[g])).ToList();
        }

        /// Avatar-space eye position (VRChat's view position is relative to the avatar root).
        public Vector3? Viewpoint() => descriptor != null ? P(descriptor, "ViewPosition").vector3Value : (Vector3?)null;

        /// `X_avatar.face`: format `Face` JSON whose binds name the glTF node carrying the mesh (ADR 0042).
        public string Face()
        {
            if (descriptor == null)
            {
                return null;
            }
            var visemes = new List<string>();
            var lipSync = EnumName(P(descriptor, "lipSync"));
            if (lipSync == "VisemeBlendShape" && P(descriptor, "VisemeSkinnedMesh").objectReferenceValue is SkinnedMeshRenderer smr)
            {
                var names = P(descriptor, "VisemeBlendShapes");
                for (int v = 0; v < Visemes.Length && v < names.arraySize; v++)
                {
                    var bind = Bind(smr, names.GetArrayElementAtIndex(v).stringValue);
                    if (bind != null)
                    {
                        visemes.Add($"[\"{Visemes[v]}\",[{bind}]]");
                    }
                }
            }
            notes.Add($"lip sync {lipSync} ({visemes.Count} visemes)");

            var eye = P(descriptor, "customEyeLookSettings");
            var blink = new List<string>();
            if (EnumName(P(eye, "eyelidType")) == "Blendshapes" && P(eye, "eyelidsSkinnedMesh").objectReferenceValue is SkinnedMeshRenderer lids)
            {
                var shapes = P(eye, "eyelidsBlendshapes");
                int index = shapes.arraySize > 0 ? shapes.GetArrayElementAtIndex(0).intValue : -1;
                if (index >= 0 && lids.sharedMesh != null && index < lids.sharedMesh.blendShapeCount && node(lids.transform) >= 0)
                {
                    blink.Add($"{{\"mesh\":{node(lids.transform)},\"shape\":{index},\"weight\":1}}");
                }
                // Looking up and down eyelid shapes have no format field yet.
            }
            notes.Add($"blink {blink.Count}");

            string eyes = "null";
            var left = P(eye, "leftEye").objectReferenceValue as Transform;
            var right = P(eye, "rightEye").objectReferenceValue as Transform;
            if (P(descriptor, "enableEyeLook").boolValue && (node(left) >= 0 || node(right) >= 0))
            {
                // VRChat stores each look direction as the eyes' local rotations; the format keeps the angle from
                // looking straight, followed one to one up to it.
                var straight = P(eye, "eyesLookingStraight.left").quaternionValue;
                string Range(string direction)
                {
                    var q = P(eye, direction + ".left").quaternionValue;
                    float a = Valid(straight) && Valid(q) ? Quaternion.Angle(straight, q) : 0;
                    return $"{{\"input_max\":{Num(a)},\"output_scale\":{Num(a)}}}";
                }
                string Node(Transform t) => node(t) >= 0 ? node(t).ToString() : "null";
                // For the left eye, the nose is to its right: looking right turns it inwards.
                eyes = $"{{\"driver\":{{\"Bones\":{{\"left\":{Node(left)},\"right\":{Node(right)}}}}}," +
                    $"\"horizontal_inner\":{Range("eyesLookingRight")},\"horizontal_outer\":{Range("eyesLookingLeft")}," +
                    $"\"vertical_down\":{Range("eyesLookingDown")},\"vertical_up\":{Range("eyesLookingUp")}}}";
            }
            notes.Add(eyes == "null" ? "no eye look" : "eye look");
            if (visemes.Count == 0 && blink.Count == 0 && eyes == "null")
            {
                return null;
            }
            return $"{{\"visemes\":[{string.Join(",", visemes)}],\"blink\":[{string.Join(",", blink)}],\"eyes\":{eyes}}}";
        }

        static bool Valid(Quaternion q) => q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w > 0.5f;

        string Bind(SkinnedMeshRenderer smr, string shape)
        {
            int index = smr.sharedMesh != null && !string.IsNullOrEmpty(shape) ? smr.sharedMesh.GetBlendShapeIndex(shape) : -1;
            return index >= 0 && node(smr.transform) >= 0
                ? $"{{\"mesh\":{node(smr.transform)},\"shape\":{index},\"weight\":1}}" : null;
        }

        /// `X_expressions`: synced parameters and the menu tree (icons are not exported).
        public string Expressions()
        {
            if (descriptor == null || !P(descriptor, "customExpressions").boolValue)
            {
                return null;
            }
            var parameters = new List<string>();
            if (P(descriptor, "expressionParameters").objectReferenceValue is ScriptableObject ps)
            {
                var list = P(new SerializedObject(ps), "parameters");
                for (int i = 0; i < list.arraySize; i++)
                {
                    var p = list.GetArrayElementAtIndex(i);
                    var name = P(p, "name").stringValue;
                    if (string.IsNullOrEmpty(name))
                    {
                        continue;
                    }
                    parameters.Add($"{{\"name\":{Str(name)},\"kind\":\"{EnumName(P(p, "valueType"))}\"," +
                        $"\"default\":{Num(P(p, "defaultValue").floatValue)},\"saved\":{Bool(P(p, "saved").boolValue)}," +
                        $"\"synced\":{Bool(P(p, "networkSynced").boolValue)}}}");
                }
            }
            int controls = 0;
            var menu = Menu(P(descriptor, "expressionsMenu").objectReferenceValue as ScriptableObject,
                new HashSet<ScriptableObject>(), ref controls);
            notes.Add($"{parameters.Count} expression parameters, {controls} menu controls");
            return $"{{\"parameters\":[{string.Join(",", parameters)}],\"menu\":{menu}}}";
        }

        /// A menu page; a submenu already open higher up becomes an empty page (menus may loop).
        string Menu(ScriptableObject m, HashSet<ScriptableObject> open, ref int count)
        {
            if (m == null || !open.Add(m))
            {
                return "{\"controls\":[]}";
            }
            var list = P(new SerializedObject(m), "controls");
            var controls = new List<string>();
            for (int i = 0; i < list.arraySize; i++)
            {
                var c = list.GetArrayElementAtIndex(i);
                var type = EnumName(P(c, "type"));
                string kind = type == "SubMenu"
                    ? $"{{\"SubMenu\":{Menu(P(c, "subMenu").objectReferenceValue as ScriptableObject, open, ref count)}}}"
                    : $"\"{type}\"";
                var parameter = P(c, "parameter.name").stringValue;
                var subs = P(c, "subParameters");
                var names = Enumerable.Range(0, subs.arraySize)
                    .Select(k => Str(P(subs.GetArrayElementAtIndex(k), "name").stringValue));
                controls.Add($"{{\"name\":{Str(P(c, "name").stringValue)},\"icon\":null,\"kind\":{kind}," +
                    $"\"parameter\":{(string.IsNullOrEmpty(parameter) ? "null" : Str(parameter))}," +
                    $"\"value\":{Num(P(c, "value").floatValue)},\"sub_parameters\":[{string.Join(",", names)}]}}");
                count++;
            }
            open.Remove(m);
            return $"{{\"controls\":[{string.Join(",", controls)}]}}";
        }

        /// `X_dynamics`: PhysBones as chains and the colliders they use.
        public string Dynamics()
        {
            var colliders = new List<string>();
            var colliderIndex = new Dictionary<Component, int>();
            foreach (var c in Components(DynamicsNs + "PhysBone.Components.VRCPhysBoneCollider"))
            {
                try
                {
                    var so = new SerializedObject(c);
                    int n = node(Or(P(so, "rootTransform"), c));
                    var type = EnumName(P(so, "shapeType"));
                    float r = P(so, "radius").floatValue, h = P(so, "height").floatValue;
                    string shape = type == "Sphere" ? $"{{\"Sphere\":{{\"radius\":{Num(r)}}}}}"
                        : type == "Capsule" ? $"{{\"Capsule\":{{\"radius\":{Num(r)},\"height\":{Num(h)}}}}}"
                        : type == "Plane" ? "\"Plane\"" : null;
                    if (n < 0 || shape == null)
                    {
                        Skip("collider");
                        continue;
                    }
                    colliderIndex[c] = colliders.Count;
                    colliders.Add($"{{\"node\":{n},\"shape\":{shape},\"position\":{Vec(P(so, "position").vector3Value)}," +
                        $"\"rotation\":{Quat(P(so, "rotation").quaternionValue)},\"inside\":{Bool(P(so, "insideBounds").boolValue)}}}");
                }
                catch (MissingFieldException)
                {
                    Skip("collider (unknown fields)");
                }
            }
            var chains = new List<string>();
            int v11 = 0;
            foreach (var c in Components(DynamicsNs + "PhysBone.Components.VRCPhysBone"))
            {
                try
                {
                    var so = new SerializedObject(c);
                    int rootNode = node(Or(P(so, "rootTransform"), c));
                    if (rootNode < 0)
                    {
                        Skip("PhysBone outside the avatar");
                        continue;
                    }
                    v11 += EnumName(P(so, "version")) == "Version_1_0" ? 0 : 1;
                    var ignore = P(so, "ignoreTransforms");
                    var ignored = Enumerable.Range(0, ignore.arraySize)
                        .Select(i => node(ignore.GetArrayElementAtIndex(i).objectReferenceValue as Transform)).Where(i => i >= 0);
                    var used = new List<int>();
                    if (EnumName(P(so, "allowCollision")) != "False")
                    {
                        var list = P(so, "colliders");
                        for (int i = 0; i < list.arraySize; i++)
                        {
                            if (list.GetArrayElementAtIndex(i).objectReferenceValue is Component col && colliderIndex.TryGetValue(col, out int k))
                            {
                                used.Add(k);
                            }
                        }
                    }
                    string G(string field) => Graded(so, field);
                    var rotation = Euler(P(so, "limitRotation").vector3Value);
                    string limit;
                    switch (EnumName(P(so, "limitType")))
                    {
                        case "Angle": limit = $"{{\"Angle\":{{\"max_angle\":{G("maxAngleX")},\"rotation\":{rotation}}}}}"; break;
                        case "Hinge": limit = $"{{\"Hinge\":{{\"max_angle\":{G("maxAngleX")},\"rotation\":{rotation}}}}}"; break;
                        case "Polar": limit = $"{{\"Polar\":{{\"max_pitch\":{G("maxAngleX")},\"max_yaw\":{G("maxAngleZ")},\"rotation\":{rotation}}}}}"; break;
                        default: limit = "\"None\""; break;
                    }
                    var parameter = P(so, "parameter").stringValue;
                    chains.Add($"{{\"root\":{rootNode},\"ignore\":[{string.Join(",", ignored)}]," +
                        $"\"endpoint\":{Vec(P(so, "endpointPosition").vector3Value)}," +
                        $"\"multi_child\":\"{EnumName(P(so, "multiChildType"))}\",\"integration\":\"{EnumName(P(so, "integrationType"))}\"," +
                        $"\"pull\":{G("pull")},\"spring\":{G("spring")},\"stiffness\":{G("stiffness")},\"gravity\":{G("gravity")}," +
                        $"\"gravity_falloff\":{G("gravityFalloff")},\"immobile\":{G("immobile")}," +
                        $"\"immobile_type\":\"{EnumName(P(so, "immobileType"))}\",\"limit\":{limit},\"radius\":{G("radius")}," +
                        $"\"colliders\":[{string.Join(",", used)}],\"max_stretch\":{G("maxStretch")},\"max_squish\":{G("maxSquish")}," +
                        $"\"allow_grab\":{Bool(EnumName(P(so, "allowGrabbing")) != "False")}," +
                        $"\"allow_posing\":{Bool(EnumName(P(so, "allowPosing")) != "False")}," +
                        $"\"parameter\":{(string.IsNullOrEmpty(parameter) ? "null" : Str(parameter))}}}");
                }
                catch (MissingFieldException)
                {
                    Skip("PhysBone (unknown fields)");
                }
            }
            notes.Add($"{chains.Count} PhysBones ({v11} version 1.1), {colliders.Count} colliders");
            return chains.Count + colliders.Count > 0
                ? $"{{\"chains\":[{string.Join(",", chains)}],\"colliders\":[{string.Join(",", colliders)}]}}" : null;
        }

        /// A PhysBone value and its curve over depth, sampled at nine points (VRChat multiplies the value by it).
        static string Graded(SerializedObject so, string field)
        {
            var curve = P(so, field + "Curve").animationCurveValue;
            var points = curve != null && curve.length > 0
                ? Enumerable.Range(0, 9).Select(i => Nums(i / 8f, curve.Evaluate(i / 8f))) : Enumerable.Empty<string>();
            return $"{{\"value\":{Num(P(so, field).floatValue)},\"curve\":[{string.Join(",", points)}]}}";
        }

        /// `X_contacts`: senders and receivers (box shapes have no format type yet, #55).
        public string Contacts()
        {
            var senders = new List<string>();
            var receivers = new List<string>();
            foreach (var receiver in new[] { false, true })
            {
                var type = DynamicsNs + "Contact.Components." + (receiver ? "VRCContactReceiver" : "VRCContactSender");
                foreach (var c in Components(type))
                {
                    try
                    {
                        var so = new SerializedObject(c);
                        int n = node(Or(P(so, "rootTransform"), c));
                        var shapeType = EnumName(P(so, "shapeType"));
                        float r = P(so, "radius").floatValue, h = P(so, "height").floatValue;
                        string shape = shapeType == "Sphere" ? $"{{\"Sphere\":{{\"radius\":{Num(r)}}}}}"
                            : shapeType == "Capsule" ? $"{{\"Capsule\":{{\"radius\":{Num(r)},\"height\":{Num(h)}}}}}" : null;
                        if (n < 0 || shape == null)
                        {
                            Skip($"contact {shapeType}");
                            continue;
                        }
                        var tags = P(so, "collisionTags");
                        var tagList = Enumerable.Range(0, tags.arraySize).Select(i => Str(tags.GetArrayElementAtIndex(i).stringValue));
                        var head = $"{{\"volume\":{{\"node\":{n},\"shape\":{shape},\"position\":{Vec(P(so, "position").vector3Value)}," +
                            $"\"rotation\":{Quat(P(so, "rotation").quaternionValue)}}},\"tags\":[{string.Join(",", tagList)}]," +
                            $"\"local_only\":{Bool(P(so, "localOnly").boolValue)}";
                        if (!receiver)
                        {
                            senders.Add(head + "}");
                            continue;
                        }
                        receivers.Add(head + $",\"parameter\":{Str(P(so, "parameter").stringValue)}," +
                            $"\"kind\":\"{EnumName(P(so, "receiverType"))}\",\"allow_self\":{Bool(P(so, "allowSelf").boolValue)}," +
                            $"\"allow_others\":{Bool(P(so, "allowOthers").boolValue)},\"min_velocity\":{Num(P(so, "minVelocity").floatValue)}}}");
                    }
                    catch (MissingFieldException)
                    {
                        Skip("contact (unknown fields)");
                    }
                }
            }
            notes.Add($"{senders.Count} contact senders, {receivers.Count} receivers");
            return senders.Count + receivers.Count > 0
                ? $"{{\"senders\":[{string.Join(",", senders)}],\"receivers\":[{string.Join(",", receivers)}]}}" : null;
        }

        /// VRChat constraints as format `Constraint` JSON with their options (ADR 0040).
        public List<string> Constraints()
        {
            var list = new List<string>();
            foreach (var kind in new[] { "Position", "Rotation", "Scale", "Parent" })
            {
                foreach (var c in Components(DynamicsNs + $"Constraint.Components.VRC{kind}Constraint"))
                {
                    try
                    {
                        list.Add(Constraint(kind, c));
                    }
                    catch (MissingFieldException)
                    {
                        Skip($"VRC{kind}Constraint (unknown fields)");
                    }
                }
            }
            foreach (var kind in new[] { "Aim", "LookAt" })
            {
                foreach (var _ in Components(DynamicsNs + $"Constraint.Components.VRC{kind}Constraint"))
                {
                    Skip($"VRC{kind}Constraint");
                }
            }
            int rebake = 0;
            foreach (var c in root.GetComponentsInChildren<Component>(true).Where(c => c != null && c.GetType().FullName.StartsWith(DynamicsNs + "Constraint.")))
            {
                var p = new SerializedObject(c).FindProperty("RebakeOffsetsWhenUnfrozen");
                rebake += p != null && p.boolValue ? 1 : 0;
            }
            notes.Add($"{list.Count} VRChat constraints ({rebake} rebake offsets when unfrozen, #73)");
            return list;
        }

        string Constraint(string kind, Component c)
        {
            var so = new SerializedObject(c);
            int target = node(Or(P(so, "TargetTransform"), c));
            if (target < 0)
            {
                throw new MissingFieldException(c.name, "TargetTransform outside the avatar");
            }
            string Axes(string what) => $"[{Bool(P(so, $"Affects{what}X").boolValue)},{Bool(P(so, $"Affects{what}Y").boolValue)}," +
                $"{Bool(P(so, $"Affects{what}Z").boolValue)}]";
            Vector3 V(string f) => P(so, f).vector3Value;
            string json, axes, rest;
            switch (kind)
            {
                case "Position":
                    json = $"{{\"Position\":{{\"offset\":{Vec(V("PositionOffset"))}}}}}";
                    axes = Axes("Position");
                    rest = Transform(V("PositionAtRest"), Quaternion.identity, Vector3.one);
                    break;
                case "Rotation":
                    json = $"{{\"Rotation\":{{\"offset\":{Euler(V("RotationOffset"))}}}}}";
                    axes = Axes("Rotation");
                    rest = Transform(Vector3.zero, Quaternion.Euler(V("RotationAtRest")), Vector3.one);
                    break;
                case "Scale":
                    var s = V("ScaleOffset");
                    json = $"{{\"Scale\":{{\"offset\":{Nums(s.x, s.y, s.z)}}}}}";
                    axes = Axes("Scale");
                    rest = Transform(Vector3.zero, Quaternion.identity, V("ScaleAtRest"));
                    break;
                default:
                    json = $"{{\"Parent\":{{\"rotation_axes\":{Axes("Rotation")}}}}}";
                    axes = Axes("Position");
                    rest = Transform(V("PositionAtRest"), Quaternion.Euler(V("RotationAtRest")), Vector3.one);
                    break;
            }
            var sources = P(so, "Sources");
            int count = P(sources, "totalLength").intValue;
            var overflow = P(sources, "overflowList");
            var parts = new List<string>();
            for (int i = 0; i < count; i++)
            {
                var src = i < 16 ? P(sources, $"source{i}") : overflow.GetArrayElementAtIndex(i - 16);
                int n = node(P(src, "SourceTransform").objectReferenceValue as Transform);
                if (n < 0)
                {
                    Skip("constraint source outside the avatar");
                    continue;
                }
                var offset = kind == "Parent"
                    ? Transform(P(src, "ParentPositionOffset").vector3Value, Quaternion.Euler(P(src, "ParentRotationOffset").vector3Value), Vector3.one)
                    : Transform(Vector3.zero, Quaternion.identity, Vector3.one);
                parts.Add($"{{\"node\":{n},\"weight\":{Num(P(src, "Weight").floatValue)},\"offset\":{offset}}}");
            }
            return $"{{\"kind\":{json},\"target\":{target},\"sources\":[{string.Join(",", parts)}]," +
                $"\"weight\":{Num(P(so, "GlobalWeight").floatValue)},\"active\":{Bool(P(so, "IsActive").boolValue)},\"axes\":{axes}," +
                $"\"rest\":{rest},\"options\":{{\"local_space\":{Bool(P(so, "SolveInLocalSpace").boolValue)}," +
                $"\"freeze\":{Bool(P(so, "FreezeToWorld").boolValue)}}}}}";
        }

        /// Counts VRChat components this exporter does not carry (head chops, audio, station, ...).
        public void CountOthers()
        {
            var known = new HashSet<string> { "VRCAvatarDescriptor", "VRCPhysBone", "VRCPhysBoneCollider", "VRCContactSender",
                "VRCContactReceiver", "VRCPositionConstraint", "VRCRotationConstraint", "VRCScaleConstraint",
                "VRCParentConstraint", "VRCAimConstraint", "VRCLookAtConstraint", "PipelineManager" };
            foreach (var c in root.GetComponentsInChildren<Component>(true))
            {
                if (c != null && c.GetType().FullName.StartsWith("VRC.") && !known.Contains(c.GetType().Name))
                {
                    Skip(c.GetType().Name);
                }
            }
        }
    }
}
