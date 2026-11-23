//! Writes the skeleton each export's humanoid `Avatar` was built from (`humanDescription.skeleton`, #95) to
//! `<out>/<name>.skeleton.json`: Unity's frames follow it, and its thumbs and fingers often differ from the prefab's rest.
//! Copy into the exporter host project (the avatars are third-party): `-executeMethod BuildSkeleton.Record
//! -civbbList <abs path>/corpus/local/unity-glb/exports.txt -civbbOut <abs path>/corpus/local/unity-truth`.
using System;
using System.Globalization;
using System.IO;
using System.Linq;
using UnityEditor;
using UnityEngine;

public static class BuildSkeleton
{
    public static void Record()
    {
        try
        {
            var args = Environment.GetCommandLineArgs();
            string Arg(string n) => args[Array.IndexOf(args, n) + 1];
            string F(float x) => x.ToString("R", CultureInfo.InvariantCulture);
            string Str(string s) => "\"" + s.Replace("\\", "\\\\").Replace("\"", "\\\"") + "\"";
            foreach (var line in File.ReadAllLines(Arg("-civbbList")))
            {
                var parts = line.Split('|');
                var prefab = AssetDatabase.LoadAssetAtPath<GameObject>(parts[1]);
                var avatar = prefab.GetComponentInChildren<Animator>(true)?.avatar;
                if (avatar == null || !avatar.isHuman)
                    continue;
                // The first entry is the model's root, which the export does not place.
                var bones = avatar.humanDescription.skeleton.Skip(1).Select(b => "{\"name\": " + Str(b.name) + ", \"position\": [" +
                    F(b.position.x) + ", " + F(b.position.y) + ", " + F(b.position.z) + "], \"rotation\": [" + F(b.rotation.x) + ", " +
                    F(b.rotation.y) + ", " + F(b.rotation.z) + ", " + F(b.rotation.w) + "]}");
                File.WriteAllText(Path.Combine(Arg("-civbbOut"), parts[0] + ".skeleton.json"), "[\n" + string.Join(",\n", bones) + "\n]\n");
            }
            Debug.Log("CIVBB build skeletons recorded");
        }
        catch (Exception e)
        {
            Debug.LogError($"CIVBB failed: {e}");
        }
        EditorApplication.Exit(0);
    }
}
