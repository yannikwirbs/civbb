// Batch-mode entry points (ADR 0038): import a .unitypackage that carries no code, list the humanoid prefabs, and
// export one prefab as GLB. Each exits the editor itself: 0 on success, 1 on failure.
using System;
using System.IO;
using System.Linq;
using UnityEditor;
using UnityEngine;

namespace Civbb.Exporter
{
    public static class Batch
    {
        const string PendingKey = "civbb.import.package";
        const double ImportTimeout = 600;

        /// `-civbbPackage <file>`: refuses packages with code, imports the rest and makes imported models readable.
        /// `ImportPackage` runs asynchronously and may be followed by a domain reload (the VRChat SDK recompiles after
        /// changing settings), which drops static callbacks. So the editor polls until every asset path the package
        /// lists exists and nothing compiles or imports, and the pending import survives reloads in `SessionState`.
        public static void Import()
        {
            string path;
            try
            {
                path = Arg("-civbbPackage") ?? throw new ArgumentException("missing -civbbPackage");
                var code = UnityPackage.CodeEntries(path);
                if (code.Count > 0)
                {
                    throw new InvalidOperationException(
                        $"refused, the package carries code: {string.Join(", ", code.Take(5))}");
                }
            }
            catch (Exception e)
            {
                Debug.LogError($"CIVBB failed: {e}");
                EditorApplication.Exit(1);
                return;
            }
            SessionState.SetString(PendingKey, path);
            AssetDatabase.importPackageFailed += (_, error) => Run(() => throw new InvalidOperationException(error));
            AssetDatabase.importPackageCancelled += _ => Run(() => throw new InvalidOperationException("cancelled"));
            AssetDatabase.ImportPackage(path, false);
            Watch();
        }

        [InitializeOnLoadMethod]
        static void Resume()
        {
            if (!string.IsNullOrEmpty(SessionState.GetString(PendingKey, "")))
            {
                Watch();
            }
        }

        static void Watch()
        {
            var path = SessionState.GetString(PendingKey, "");
            var expected = UnityPackage.Entries(path);
            // An asset whose GUID the project already has keeps its existing path.
            bool Present((string guid, string path) e) => File.Exists(e.path) || Directory.Exists(e.path) ||
                !string.IsNullOrEmpty(AssetDatabase.GUIDToAssetPath(e.guid));
            double start = EditorApplication.timeSinceStartup;
            EditorApplication.CallbackFunction poll = null;
            poll = () =>
            {
                if (EditorApplication.isCompiling || EditorApplication.isUpdating)
                {
                    return;
                }
                bool done = expected.All(Present);
                if (!done && EditorApplication.timeSinceStartup - start < ImportTimeout)
                {
                    return;
                }
                EditorApplication.update -= poll;
                SessionState.EraseString(PendingKey);
                Run(() =>
                {
                    if (!done)
                    {
                        var missing = expected.Where(e => !Present(e)).Take(5).Select(e => e.path);
                        throw new InvalidOperationException($"import timed out, missing {string.Join(", ", missing)}");
                    }
                    AssetDatabase.Refresh();
                    foreach (var guid in AssetDatabase.FindAssets("t:Model"))
                    {
                        var importer = AssetImporter.GetAtPath(AssetDatabase.GUIDToAssetPath(guid)) as ModelImporter;
                        if (importer != null && !importer.isReadable)
                        {
                            importer.isReadable = true;
                            importer.SaveAndReimport();
                        }
                    }
                    Debug.Log($"CIVBB imported {path} ({expected.Count} assets)");
                });
            };
            EditorApplication.update += poll;
        }

        /// `-civbbAvatar <prefab asset path> -civbbOut <file.glb>` writes the prefab; without `-civbbAvatar` it lists
        /// prefabs whose root has a humanoid Animator.
        public static void Export()
        {
            Run(() =>
            {
                var avatar = Arg("-civbbAvatar");
                if (avatar == null)
                {
                    foreach (var guid in AssetDatabase.FindAssets("t:Prefab"))
                    {
                        var path = AssetDatabase.GUIDToAssetPath(guid);
                        var go = AssetDatabase.LoadAssetAtPath<GameObject>(path);
                        var animator = go != null ? go.GetComponent<Animator>() : null;
                        if (animator != null && animator.avatar != null && animator.avatar.isHuman)
                        {
                            Debug.Log($"CIVBB humanoid prefab: {path}");
                        }
                    }
                    return;
                }
                var output = Arg("-civbbOut") ?? throw new ArgumentException("missing -civbbOut");
                // `Base=<controller>;FX=<controller>` fills the animator's layer groups in order (ADR 0041) until
                // the avatar descriptor is read (#37).
                var layers = (Arg("-civbbLayers") ?? "").Split(new[] { ';' }, StringSplitOptions.RemoveEmptyEntries)
                    .Select(l => l.Split(new[] { '=' }, 2))
                    .Select(p => p.Length == 2 ? (p[0].Trim(), p[1].Trim()) : throw new ArgumentException($"bad -civbbLayers entry {p[0]}"))
                    .ToList();
                var root = PrefabUtility.LoadPrefabContents(avatar);
                try
                {
                    var (glb, report) = Glb.Write(root, layers);
                    File.WriteAllBytes(output, glb);
                    Debug.Log($"CIVBB exported {avatar} to {output}: {report}");
                }
                finally
                {
                    PrefabUtility.UnloadPrefabContents(root);
                }
            });
        }

        static string Arg(string name)
        {
            var args = Environment.GetCommandLineArgs();
            int i = Array.IndexOf(args, name);
            return i >= 0 && i + 1 < args.Length ? args[i + 1] : null;
        }

        static void Run(Action work)
        {
            try
            {
                work();
                EditorApplication.Exit(0);
            }
            catch (Exception e)
            {
                Debug.LogError($"CIVBB failed: {e}");
                EditorApplication.Exit(1);
            }
        }
    }
}
