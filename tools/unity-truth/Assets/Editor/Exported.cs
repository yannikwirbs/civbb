// Records Unity's own result for an avatar the exporter wrote (#36), so a local test can compare civbb-anim on the
// exported GLB: named clips sampled on the prefab, and the prefab's Unity constraints under random source poses.
// The avatars are third-party: run it in an exporter host project (copy this file into its Assets/Editor) and keep
// the output in corpus/local. Constraints only run in play mode, so run it without -quit; it exits by itself.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using UnityEditor;
using UnityEngine;
using UnityEngine.Animations;
using UnityEngine.Playables;
using Object = UnityEngine.Object;

public static class Exported
{
    static GameObject avatar;
    static List<Transform> nodes;
    static (Vector3 p, Quaternion r, Vector3 s)[] rest;
    static readonly StringBuilder json = new StringBuilder();
    static string outPath;
    static List<IConstraint> constraints;
    static List<Transform> inputs;
    static System.Random random;
    static int pose, frames;
    const int Poses = 8;

    /// `-executeMethod Exported.Record -civbbAvatar <prefab> -civbbClips "a;b" -civbbOut <abs json>`.
    public static void Record()
    {
        try
        {
            var args = Environment.GetCommandLineArgs();
            string Arg(string n) => args[Array.IndexOf(args, n) + 1];
            outPath = Arg("-civbbOut");
            var prefab = AssetDatabase.LoadAssetAtPath<GameObject>(Arg("-civbbAvatar"));
            var clipNames = Arg("-civbbClips").Split(';');
            avatar = (GameObject)Object.Instantiate(prefab);
            avatar.name = prefab.name;
            // Keep Unity's own components only: VRChat runtime components (PhysBones, VRChat constraints) would move
            // bones in play mode.
            foreach (var c in avatar.GetComponentsInChildren<Component>(true).Where(c => c is MonoBehaviour).ToList())
                Object.DestroyImmediate(c);
            nodes = new List<Transform>();
            Collect(avatar.transform);
            rest = nodes.Select(t => (t.localPosition, t.localRotation, t.localScale)).ToArray();
            json.Append("{\n  \"paths\": [").Append(string.Join(", ", nodes.Select(t => S(Path(t))))).Append("],\n");
            json.Append("  \"rest\": ").Append(Locals()).Append(",\n  \"clips\": [");
            var animator = avatar.GetComponent<Animator>();
            animator.runtimeAnimatorController = null;
            animator.applyRootMotion = false;
            // Batch mode has no camera: a culling animator would evaluate nothing.
            animator.enabled = true;
            animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
            Debug.Log($"CIVBB exported truth: animator culling was {prefab.GetComponent<Animator>().cullingMode}, " +
                $"enabled {prefab.GetComponent<Animator>().enabled}");
            bool first = true;
            foreach (var name in clipNames)
            {
                var clip = AssetDatabase.FindAssets($"t:AnimationClip {name}")
                    .Select(g => AssetDatabase.GUIDToAssetPath(g)).Where(p => p.StartsWith("Assets/"))
                    .SelectMany(AssetDatabase.LoadAllAssetsAtPath).OfType<AnimationClip>().FirstOrDefault(c => c.name == name)
                    ?? throw new ArgumentException($"no clip {name}");
                foreach (var f in new[] { 0f, 1 / 3f, 2 / 3f })
                {
                    float time = f * clip.length;
                    Reset();
                    var graph = PlayableGraph.Create();
                    var output = AnimationPlayableOutput.Create(graph, "civbb", animator);
                    var play = AnimationClipPlayable.Create(graph, clip);
                    play.SetApplyFootIK(false);
                    play.SetTime(time);
                    output.SetSourcePlayable(play);
                    graph.Evaluate();
                    json.Append(first ? "\n    " : ",\n    ").Append("{\"clip\": ").Append(S(name)).Append(", \"time\": ")
                        .Append(F(time)).Append(", \"locals\": ").Append(Locals()).Append(", \"shapes\": ").Append(Shapes()).Append('}');
                    first = false;
                    graph.Destroy();
                }
            }
            json.Append("\n  ],\n  \"constraints\": [");
            Reset();
            constraints = avatar.GetComponentsInChildren<Transform>(true).SelectMany(t => t.GetComponents<IConstraint>()).ToList();
            inputs = new List<Transform>();
            foreach (var c in constraints)
            {
                var sources = new List<ConstraintSource>();
                c.GetSources(sources);
                inputs.AddRange(sources.Select(s => s.sourceTransform).Where(t => t != null));
                var target = ((Component)c).transform;
                if (target.parent != null)
                    inputs.Add(target.parent);
            }
            inputs = inputs.Distinct().ToList();
            random = new System.Random(5);
            EditorSettings.enterPlayModeOptionsEnabled = true;
            EditorSettings.enterPlayModeOptions = EnterPlayModeOptions.DisableDomainReload | EnterPlayModeOptions.DisableSceneReload;
            EditorApplication.update += Tick;
            EditorApplication.EnterPlaymode();
        }
        catch (Exception e)
        {
            Debug.LogError("CIVBB exported truth failed: " + e);
            EditorApplication.Exit(1);
        }
    }

    // Each tick after the first: record what the constraints made of the last pose, then set the next one. Each pose
    // is held for three frames so the constraints settle.
    static void Tick()
    {
        if (!EditorApplication.isPlaying)
            return;
        try
        {
            frames++;
            if (frames % 3 == 0 && pose > 0)
            {
                json.Append(pose == 1 ? "\n    " : ",\n    ").Append("{\"locals\": ").Append(Locals()).Append('}');
            }
            if (frames % 3 == 0)
            {
                if (pose == Poses || constraints.Count == 0)
                {
                    json.Append("\n  ]\n}\n");
                    File.WriteAllText(outPath, json.ToString());
                    Debug.Log($"CIVBB exported truth recorded to {outPath}: {constraints.Count} constraints");
                    EditorApplication.update -= Tick;
                    EditorApplication.Exit(0);
                    return;
                }
                pose++;
            }
            // Every frame: rest pose, then this pose's random source rotations (an animator would rewrite them).
            Reset();
            var r = new System.Random(pose * 7919);
            foreach (var t in inputs)
                t.localRotation = Quaternion.Euler(R(r) * 60, R(r) * 60, R(r) * 60) * rest[nodes.IndexOf(t)].r;
        }
        catch (Exception e)
        {
            Debug.LogError("CIVBB exported truth failed: " + e);
            EditorApplication.Exit(1);
        }
    }

    static float R(System.Random r) => (float)(r.NextDouble() * 2 - 1);

    static void Collect(Transform t)
    {
        nodes.Add(t);
        foreach (Transform c in t)
            Collect(c);
    }

    static string Path(Transform t) => t == avatar.transform ? "" : t.parent == avatar.transform ? t.name : Path(t.parent) + "/" + t.name;

    static void Reset()
    {
        for (int i = 0; i < nodes.Count; i++)
        {
            nodes[i].localPosition = rest[i].p;
            nodes[i].localRotation = rest[i].r;
            nodes[i].localScale = rest[i].s;
        }
        foreach (var smr in avatar.GetComponentsInChildren<SkinnedMeshRenderer>(true))
            for (int s = 0; smr.sharedMesh != null && s < smr.sharedMesh.blendShapeCount; s++)
                smr.SetBlendShapeWeight(s, 0);
    }

    /// Every node's local transform, Unity space: [px, py, pz, qx, qy, qz, qw, sx, sy, sz].
    static string Locals() => "[" + string.Join(", ", nodes.Select(t =>
    {
        var p = t.localPosition;
        var q = t.localRotation;
        var s = t.localScale;
        return "[" + string.Join(",", new[] { p.x, p.y, p.z, q.x, q.y, q.z, q.w, s.x, s.y, s.z }.Select(F)) + "]";
    })) + "]";

    /// Non-zero blendshape weights, by renderer path: {path: {shape name: weight 0..100}}.
    static string Shapes()
    {
        var parts = new List<string>();
        foreach (var smr in avatar.GetComponentsInChildren<SkinnedMeshRenderer>(true))
        {
            var mesh = smr.sharedMesh;
            var w = new List<string>();
            for (int s = 0; mesh != null && s < mesh.blendShapeCount; s++)
                if (smr.GetBlendShapeWeight(s) != 0)
                    w.Add(S(mesh.GetBlendShapeName(s)) + ": " + F(smr.GetBlendShapeWeight(s)));
            if (w.Count > 0)
                parts.Add(S(Path(smr.transform)) + ": {" + string.Join(", ", w) + "}");
        }
        return "{" + string.Join(", ", parts) + "}";
    }

    static string F(float f) => f.ToString("R", CultureInfo.InvariantCulture);

    static string S(string s) => "\"" + s.Replace("\\", "\\\\").Replace("\"", "\\\"") + "\"";
}
