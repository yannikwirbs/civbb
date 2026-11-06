// Writes a prefab's hierarchy as GLB (ADR 0038): nodes, one mesh per renderer with a primitive per submesh, skins,
// morph targets, materials with their main texture, the humanoid map as X_avatar, and with ADR 0041 the animator
// (X_animator) and Unity constraints (X_constraints).
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using UnityEditor;
using UnityEngine;
using static Civbb.Exporter.Json;

namespace Civbb.Exporter
{
    public static class Glb
    {
        /// The GLB bytes and a one-line summary of what went in. `layers` names the controllers that fill the
        /// animator's layer groups, in order (`Base=<asset path>`); empty writes no animator.
        public static (byte[] glb, string report) Write(GameObject root, IList<(string group, string path)> layers)
        {
            var w = new Writer();
            w.Run(root.transform);
            var animator = AnimatorExport.Write(root.transform, layers);
            if (animator.json != null)
            {
                w.extensions["X_animator"] = animator.json;
            }
            var constraints = ConstraintExport.Write(root.transform, w.NodeIndex);
            if (constraints.json != null)
            {
                w.extensions["X_constraints"] = constraints.json;
            }
            return (w.Finish(), $"{w.Report()}; {animator.report}; {constraints.report}");
        }

        sealed class Writer
        {
            const int Float = 5126, UnsignedShort = 5123, UnsignedInt = 5125;
            const int ArrayBuffer = 34962, ElementArrayBuffer = 34963;

            readonly MemoryStream bin = new MemoryStream();
            readonly List<string> views = new List<string>(), accessors = new List<string>(), meshes = new List<string>(),
                skins = new List<string>(), materials = new List<string>(), textures = new List<string>(),
                images = new List<string>(), nodes = new List<string>();
            readonly Dictionary<Transform, int> nodeIndex = new Dictionary<Transform, int>();
            readonly Dictionary<Material, int> materialIndex = new Dictionary<Material, int>();
            readonly Dictionary<string, int> textureIndex = new Dictionary<string, int>();
            public readonly SortedDictionary<string, string> extensions = new SortedDictionary<string, string>();
            int primitives, vertices, triangles, shapes, multiFrameShapes, skippedTextures, skippedSubmeshes,
                extraMaterials, missingBones, humanBones;

            public void Run(Transform root)
            {
                var order = new List<Transform>();
                Collect(root, order);
                for (int i = 0; i < order.Count; i++)
                {
                    nodeIndex[order[i]] = i;
                }
                foreach (var t in order)
                {
                    int mesh = -1, skin = -1;
                    var smr = t.GetComponent<SkinnedMeshRenderer>();
                    var filter = t.GetComponent<MeshFilter>();
                    var renderer = t.GetComponent<MeshRenderer>();
                    if (smr != null && smr.sharedMesh != null)
                    {
                        mesh = Mesh(smr.sharedMesh, smr.sharedMaterials, smr);
                        if (smr.sharedMesh.bindposes.Length > 0 && smr.bones.Length > 0)
                        {
                            skin = Skin(smr, root);
                        }
                    }
                    else if (filter != null && renderer != null && filter.sharedMesh != null)
                    {
                        mesh = Mesh(filter.sharedMesh, renderer.sharedMaterials, null);
                    }
                    nodes.Add(Node(t, t == root, mesh, skin));
                }
                Humanoid(root);
            }

            static void Collect(Transform t, List<Transform> order)
            {
                order.Add(t);
                foreach (Transform c in t)
                {
                    Collect(c, order);
                }
            }

            string Node(Transform t, bool root, int mesh, int skin)
            {
                var sb = new StringBuilder("{\"name\":").Append(Str(t.name));
                if (!root)
                {
                    var p = t.localPosition;
                    var q = t.localRotation.normalized;
                    var s = t.localScale;
                    sb.Append(",\"translation\":").Append(Nums(-p.x, p.y, p.z));
                    sb.Append(",\"rotation\":").Append(Nums(q.x, -q.y, -q.z, q.w));
                    sb.Append(",\"scale\":").Append(Nums(s.x, s.y, s.z));
                }
                var children = new List<int>();
                foreach (Transform c in t)
                {
                    children.Add(nodeIndex[c]);
                }
                if (children.Count > 0)
                {
                    sb.Append(",\"children\":[").Append(string.Join(",", children)).Append(']');
                }
                if (mesh >= 0)
                {
                    sb.Append(",\"mesh\":").Append(mesh);
                }
                if (skin >= 0)
                {
                    sb.Append(",\"skin\":").Append(skin);
                }
                if (!t.gameObject.activeSelf)
                {
                    sb.Append(",\"extras\":{\"active\":false}");
                }
                return sb.Append('}').ToString();
            }

            int Mesh(Mesh mesh, Material[] mats, SkinnedMeshRenderer smr)
            {
                int n = mesh.vertexCount;
                vertices += n;
                var attrs = new StringBuilder();
                attrs.Append("\"POSITION\":").Append(Vec3(mesh.vertices, true));
                var normals = mesh.normals;
                if (normals.Length == n)
                {
                    attrs.Append(",\"NORMAL\":").Append(Vec3(normals, false));
                }
                var tangents = mesh.tangents;
                if (tangents.Length == n)
                {
                    var f = new float[n * 4];
                    for (int i = 0; i < n; i++)
                    {
                        var v = tangents[i];
                        f[i * 4] = -v.x;
                        f[i * 4 + 1] = v.y;
                        f[i * 4 + 2] = v.z;
                        f[i * 4 + 3] = v.w < 0 ? 1 : -1;
                    }
                    attrs.Append(",\"TANGENT\":").Append(Accessor(View(Bytes(f), ArrayBuffer), Float, n, "VEC4"));
                }
                var uvs = new[] { mesh.uv, mesh.uv2 };
                for (int set = 0; set < uvs.Length; set++)
                {
                    if (uvs[set].Length != n)
                    {
                        continue;
                    }
                    var f = new float[n * 2];
                    for (int i = 0; i < n; i++)
                    {
                        f[i * 2] = uvs[set][i].x;
                        f[i * 2 + 1] = 1 - uvs[set][i].y;
                    }
                    attrs.Append($",\"TEXCOORD_{set}\":").Append(Accessor(View(Bytes(f), ArrayBuffer), Float, n, "VEC2"));
                }
                var colors = mesh.colors;
                if (colors.Length == n)
                {
                    var f = new float[n * 4];
                    for (int i = 0; i < n; i++)
                    {
                        var c = colors[i].linear;
                        f[i * 4] = c.r;
                        f[i * 4 + 1] = c.g;
                        f[i * 4 + 2] = c.b;
                        f[i * 4 + 3] = colors[i].a;
                    }
                    attrs.Append(",\"COLOR_0\":").Append(Accessor(View(Bytes(f), ArrayBuffer), Float, n, "VEC4"));
                }
                var weights = mesh.boneWeights;
                if (smr != null && weights.Length == n && smr.bones.Length > 0)
                {
                    var j = new ushort[n * 4];
                    var w = new float[n * 4];
                    for (int i = 0; i < n; i++)
                    {
                        var b = weights[i];
                        j[i * 4] = (ushort)b.boneIndex0;
                        j[i * 4 + 1] = (ushort)b.boneIndex1;
                        j[i * 4 + 2] = (ushort)b.boneIndex2;
                        j[i * 4 + 3] = (ushort)b.boneIndex3;
                        w[i * 4] = b.weight0;
                        w[i * 4 + 1] = b.weight1;
                        w[i * 4 + 2] = b.weight2;
                        w[i * 4 + 3] = b.weight3;
                    }
                    var jb = new byte[j.Length * 2];
                    Buffer.BlockCopy(j, 0, jb, 0, jb.Length);
                    attrs.Append(",\"JOINTS_0\":").Append(Accessor(View(jb, ArrayBuffer), UnsignedShort, n, "VEC4"));
                    attrs.Append(",\"WEIGHTS_0\":").Append(Accessor(View(Bytes(w), ArrayBuffer), Float, n, "VEC4"));
                }

                var targets = new List<string>();
                var names = new List<string>();
                var defaults = new List<float>();
                var dv = new Vector3[n];
                var dn = new Vector3[n];
                var dt = new Vector3[n];
                for (int s = 0; s < mesh.blendShapeCount; s++)
                {
                    int frames = mesh.GetBlendShapeFrameCount(s);
                    if (frames > 1)
                    {
                        multiFrameShapes++;
                    }
                    int last = frames - 1;
                    mesh.GetBlendShapeFrameVertices(s, last, dv, dn, dt);
                    targets.Add($"{{\"POSITION\":{Vec3(dv, true)},\"NORMAL\":{Vec3(dn, false)}}}");
                    names.Add(Str(mesh.GetBlendShapeName(s)));
                    float frameWeight = mesh.GetBlendShapeFrameWeight(s, last);
                    defaults.Add(smr != null && frameWeight != 0 ? smr.GetBlendShapeWeight(s) / frameWeight : 0);
                    shapes++;
                }

                var prims = new List<string>();
                extraMaterials += Math.Max(0, mats.Length - mesh.subMeshCount);
                for (int sub = 0; sub < mesh.subMeshCount; sub++)
                {
                    if (mesh.GetTopology(sub) != MeshTopology.Triangles || sub >= mats.Length)
                    {
                        skippedSubmeshes++;
                        continue;
                    }
                    var idx = mesh.GetIndices(sub);
                    var flipped = new uint[idx.Length];
                    for (int i = 0; i + 2 < idx.Length; i += 3)
                    {
                        flipped[i] = (uint)idx[i];
                        flipped[i + 1] = (uint)idx[i + 2];
                        flipped[i + 2] = (uint)idx[i + 1];
                    }
                    triangles += idx.Length / 3;
                    var ib = new byte[flipped.Length * 4];
                    Buffer.BlockCopy(flipped, 0, ib, 0, ib.Length);
                    int indices = Accessor(View(ib, ElementArrayBuffer), UnsignedInt, flipped.Length, "SCALAR");
                    var p = new StringBuilder($"{{\"attributes\":{{{attrs}}},\"indices\":{indices}");
                    int material = MaterialFor(mats[sub]);
                    if (material >= 0)
                    {
                        p.Append(",\"material\":").Append(material);
                    }
                    if (targets.Count > 0)
                    {
                        p.Append(",\"targets\":[").Append(string.Join(",", targets)).Append(']');
                    }
                    prims.Add(p.Append('}').ToString());
                    primitives++;
                }
                var m = new StringBuilder($"{{\"name\":{Str(mesh.name)},\"primitives\":[{string.Join(",", prims)}]");
                if (targets.Count > 0)
                {
                    m.Append(",\"weights\":[").Append(string.Join(",", defaults.Select(Num))).Append(']');
                    m.Append(",\"extras\":{\"targetNames\":[").Append(string.Join(",", names)).Append("]}");
                }
                meshes.Add(m.Append('}').ToString());
                return meshes.Count - 1;
            }

            int Skin(SkinnedMeshRenderer smr, Transform root)
            {
                var bones = smr.bones;
                var binds = smr.sharedMesh.bindposes;
                if (bones.Length != binds.Length)
                {
                    throw new InvalidDataException($"{smr.name}: {bones.Length} bones but {binds.Length} bind poses");
                }
                var joints = new List<int>();
                foreach (var b in bones)
                {
                    if (b != null && nodeIndex.TryGetValue(b, out int i))
                    {
                        joints.Add(i);
                    }
                    else
                    {
                        missingBones++;
                        joints.Add(nodeIndex[root]);
                    }
                }
                var sign = new[] { -1f, 1f, 1f, 1f };
                var f = new float[binds.Length * 16];
                for (int k = 0; k < binds.Length; k++)
                {
                    for (int c = 0; c < 4; c++)
                    {
                        for (int r = 0; r < 4; r++)
                        {
                            f[k * 16 + c * 4 + r] = sign[r] * sign[c] * binds[k][r, c];
                        }
                    }
                }
                int ibm = Accessor(View(Bytes(f), null), Float, binds.Length, "MAT4");
                skins.Add($"{{\"inverseBindMatrices\":{ibm},\"joints\":[{string.Join(",", joints)}]}}");
                return skins.Count - 1;
            }

            int MaterialFor(Material mat)
            {
                if (mat == null)
                {
                    return -1;
                }
                if (materialIndex.TryGetValue(mat, out int found))
                {
                    return found;
                }
                var so = new SerializedObject(mat);
                Texture main = null;
                var color = Color.white;
                var envs = so.FindProperty("m_SavedProperties.m_TexEnvs");
                for (int i = 0; envs != null && i < envs.arraySize; i++)
                {
                    var e = envs.GetArrayElementAtIndex(i);
                    if (e.FindPropertyRelative("first").stringValue == "_MainTex")
                    {
                        main = e.FindPropertyRelative("second.m_Texture").objectReferenceValue as Texture;
                    }
                }
                var cols = so.FindProperty("m_SavedProperties.m_Colors");
                for (int i = 0; cols != null && i < cols.arraySize; i++)
                {
                    var e = cols.GetArrayElementAtIndex(i);
                    if (e.FindPropertyRelative("first").stringValue == "_Color")
                    {
                        color = e.FindPropertyRelative("second").colorValue;
                    }
                }
                var lin = color.linear;
                var pbr = new StringBuilder("{\"baseColorFactor\":").Append(Nums(lin.r, lin.g, lin.b, color.a));
                pbr.Append(",\"metallicFactor\":0,\"roughnessFactor\":1");
                int tex = TextureFor(main);
                if (tex >= 0)
                {
                    pbr.Append(",\"baseColorTexture\":{\"index\":").Append(tex).Append('}');
                }
                pbr.Append('}');
                var shader = mat.shader != null ? mat.shader.name : "";
                materials.Add($"{{\"name\":{Str(mat.name)},\"pbrMetallicRoughness\":{pbr},\"extras\":{{\"unityShader\":{Str(shader)}}}}}");
                materialIndex[mat] = materials.Count - 1;
                return materials.Count - 1;
            }

            int TextureFor(Texture tex)
            {
                if (tex == null)
                {
                    return -1;
                }
                var path = AssetDatabase.GetAssetPath(tex);
                if (textureIndex.TryGetValue(path, out int found))
                {
                    return found;
                }
                var ext = Path.GetExtension(path).ToLowerInvariant();
                string mime = ext == ".png" ? "image/png" : ext == ".jpg" || ext == ".jpeg" ? "image/jpeg" : null;
                if (mime == null || !File.Exists(path))
                {
                    skippedTextures++;
                    textureIndex[path] = -1;
                    return -1;
                }
                int view = View(File.ReadAllBytes(path), null);
                images.Add($"{{\"name\":{Str(Path.GetFileNameWithoutExtension(path))},\"bufferView\":{view},\"mimeType\":\"{mime}\"}}");
                textures.Add($"{{\"source\":{images.Count - 1}}}");
                textureIndex[path] = textures.Count - 1;
                return textures.Count - 1;
            }

            void Humanoid(Transform root)
            {
                var animator = root.GetComponent<Animator>();
                if (animator == null || animator.avatar == null || !animator.avatar.isHuman)
                {
                    return;
                }
                var names = new Dictionary<string, string>();
                for (int i = 0; i < HumanTrait.BoneName.Length; i++)
                {
                    var bone = ((HumanBodyBones)i).ToString();
                    if (HumanTrait.BoneName[i].Replace(" ", "") != bone)
                    {
                        throw new InvalidOperationException($"HumanTrait {HumanTrait.BoneName[i]} is not {bone}");
                    }
                    names[HumanTrait.BoneName[i]] = bone;
                }
                var byName = new Dictionary<string, Transform>();
                foreach (var t in nodeIndex.Keys)
                {
                    if (!byName.ContainsKey(t.name))
                    {
                        byName[t.name] = t;
                    }
                }
                var entries = new List<string>();
                foreach (var h in animator.avatar.humanDescription.human)
                {
                    if (names.TryGetValue(h.humanName, out var bone) && byName.TryGetValue(h.boneName, out var t))
                    {
                        entries.Add($"{Str(bone)}:{nodeIndex[t]}");
                    }
                }
                humanBones = entries.Count;
                extensions["X_avatar"] = $"{{\"humanoid\":{{{string.Join(",", entries)}}}}}";
            }

            /// The glTF node of a transform in the prefab, or -1.
            public int NodeIndex(Transform t) => t != null && nodeIndex.TryGetValue(t, out int i) ? i : -1;

            string Vec3(Vector3[] a, bool bounds)
            {
                var f = new float[a.Length * 3];
                var min = new[] { float.MaxValue, float.MaxValue, float.MaxValue };
                var max = new[] { float.MinValue, float.MinValue, float.MinValue };
                for (int i = 0; i < a.Length; i++)
                {
                    f[i * 3] = -a[i].x;
                    f[i * 3 + 1] = a[i].y;
                    f[i * 3 + 2] = a[i].z;
                    for (int k = 0; k < 3; k++)
                    {
                        min[k] = Math.Min(min[k], f[i * 3 + k]);
                        max[k] = Math.Max(max[k], f[i * 3 + k]);
                    }
                }
                var extra = bounds && a.Length > 0 ? $",\"min\":{Nums(min)},\"max\":{Nums(max)}" : "";
                return Accessor(View(Bytes(f), ArrayBuffer), Float, a.Length, "VEC3", extra).ToString();
            }

            int View(byte[] data, int? target)
            {
                while (bin.Length % 4 != 0)
                {
                    bin.WriteByte(0);
                }
                long offset = bin.Length;
                bin.Write(data, 0, data.Length);
                var t = target.HasValue ? $",\"target\":{target.Value}" : "";
                views.Add($"{{\"buffer\":0,\"byteOffset\":{offset},\"byteLength\":{data.Length}{t}}}");
                return views.Count - 1;
            }

            int Accessor(int view, int componentType, int count, string type, string extra = "")
            {
                accessors.Add($"{{\"bufferView\":{view},\"componentType\":{componentType},\"count\":{count},\"type\":\"{type}\"{extra}}}");
                return accessors.Count - 1;
            }

            public byte[] Finish()
            {
                while (bin.Length % 4 != 0)
                {
                    bin.WriteByte(0);
                }
                var json = new StringBuilder("{\"asset\":{\"version\":\"2.0\",\"generator\":\"civbb-unity-exporter\"}");
                if (extensions.Count > 0)
                {
                    json.Append(",\"extensionsUsed\":[").Append(string.Join(",", extensions.Keys.Select(Str))).Append(']');
                }
                json.Append(",\"scene\":0,\"scenes\":[{\"nodes\":[0]}]");
                Array(json, "nodes", nodes);
                Array(json, "meshes", meshes);
                Array(json, "skins", skins);
                Array(json, "materials", materials);
                Array(json, "textures", textures);
                Array(json, "images", images);
                Array(json, "accessors", accessors);
                Array(json, "bufferViews", views);
                if (bin.Length > 0)
                {
                    json.Append(",\"buffers\":[{\"byteLength\":").Append(bin.Length).Append("}]");
                }
                if (extensions.Count > 0)
                {
                    json.Append(",\"extensions\":{")
                        .Append(string.Join(",", extensions.Select(e => $"{Str(e.Key)}:{e.Value}"))).Append('}');
                }
                json.Append('}');
                var jsonBytes = Encoding.UTF8.GetBytes(json.ToString());
                int jsonLen = (jsonBytes.Length + 3) / 4 * 4;
                int binLen = (int)bin.Length;
                using var outStream = new MemoryStream();
                using var w = new BinaryWriter(outStream);
                w.Write(0x46546C67u);
                w.Write(2u);
                w.Write((uint)(12 + 8 + jsonLen + (binLen > 0 ? 8 + binLen : 0)));
                w.Write((uint)jsonLen);
                w.Write(0x4E4F534Au);
                w.Write(jsonBytes);
                for (int i = jsonBytes.Length; i < jsonLen; i++)
                {
                    w.Write((byte)' ');
                }
                if (binLen > 0)
                {
                    w.Write((uint)binLen);
                    w.Write(0x004E4942u);
                    w.Write(bin.GetBuffer(), 0, binLen);
                }
                w.Flush();
                return outStream.ToArray();
            }

            public string Report()
            {
                return $"{nodes.Count} nodes, {meshes.Count} meshes, {primitives} primitives, {vertices} vertices, " +
                    $"{triangles} triangles, {shapes} shapes ({multiFrameShapes} multi-frame), {skins.Count} skins " +
                    $"({missingBones} missing bones), {materials.Count} materials ({extraMaterials} extra), " +
                    $"{images.Count} images ({skippedTextures} textures skipped), {skippedSubmeshes} submeshes skipped, " +
                    $"humanoid {humanBones} bones";
            }

            static void Array(StringBuilder json, string key, List<string> items)
            {
                if (items.Count > 0)
                {
                    json.Append(",\"").Append(key).Append("\":[").Append(string.Join(",", items)).Append(']');
                }
            }

            static byte[] Bytes(float[] f)
            {
                var b = new byte[f.Length * 4];
                Buffer.BlockCopy(f, 0, b, 0, b.Length);
                return b;
            }
        }
    }
}
