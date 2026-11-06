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
        /// `-civbbPackage <file>`: refuses packages with code, imports the rest and makes imported models readable.
        /// `ImportPackage` runs asynchronously, so the editor exits from its completion callbacks.
        public static void Import()
        {
            string path = null;
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
            AssetDatabase.importPackageCompleted += _ => Run(() =>
            {
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
                Debug.Log($"CIVBB imported {path}");
            });
            AssetDatabase.importPackageFailed += (_, error) => Run(() => throw new InvalidOperationException(error));
            AssetDatabase.importPackageCancelled += _ => Run(() => throw new InvalidOperationException("cancelled"));
            AssetDatabase.ImportPackage(path, false);
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
                var root = PrefabUtility.LoadPrefabContents(avatar);
                try
                {
                    var (glb, report) = Glb.Write(root);
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
