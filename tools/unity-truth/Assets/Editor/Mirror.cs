//! Records how Unity mirrors a humanoid motion (#89, ADR 0060): one clip with a distinct value per muscle, played
//! plain, by a mirrored state, by a mirrored blend tree child, and by both, on the generated test skeleton.
using System;
using System.IO;
using System.Linq;
using System.Text.RegularExpressions;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using UnityEngine.Animations;
using UnityEngine.Playables;

public static class Mirror
{
    /// Entry point: `-executeMethod Mirror.Record -civbbOut <abs path>/crates/anim/tests/unity/mirror.json`.
    public static void Record()
    {
        try
        {
            var args = Environment.GetCommandLineArgs();
            var outPath = args[Array.IndexOf(args, "-civbbOut") + 1];
            var go = new GameObject("Avatar");
            var animator = go.AddComponent<Animator>();
            animator.avatar = Muscles.BuildAvatar(go, false, new[] { 0.5f, 0.5f, 0.5f, 0.5f }, out _, out _);
            animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
            var names = HumanTrait.MuscleName;
            var clip = new AnimationClip();
            for (int i = 0; i < names.Length; i++)
            {
                // Distinct, within every muscle's range, and alternating in sign.
                float v = (0.1f + 0.008f * i) * (i % 2 == 0 ? 1 : -1);
                clip.SetCurve("", typeof(Animator), ClipName(names[i]), AnimationCurve.Constant(0, 1, v));
            }
            string Row(float[] m) => "[" + string.Join(", ", m.Select(x => x.ToString("R"))) + "]";
            var json = "{\n" +
                $"  \"unity\": \"{Application.unityVersion}\",\n" +
                $"  \"muscles\": [{string.Join(", ", names.Select(n => $"\"{n}\""))}],\n" +
                $"  \"plain\": {Row(Play(animator, clip, false, false))},\n" +
                $"  \"mirrored_state\": {Row(Play(animator, clip, true, false))},\n" +
                $"  \"mirrored_child\": {Row(Play(animator, clip, false, true))},\n" +
                $"  \"mirrored_both\": {Row(Play(animator, clip, true, true))}\n" +
                "}\n";
            File.WriteAllText(outPath, json);
            Debug.Log($"CIVBB recorded {names.Length} muscles to {outPath}");
        }
        catch (Exception e)
        {
            Debug.LogError($"CIVBB failed: {e}");
        }
        finally
        {
            EditorApplication.Exit(0);
        }
    }

    /// Clip property names: fingers as clips name them (`LeftHand.Index.1 Stretched`, `LeftHand.Index.Spread`).
    static string ClipName(string m)
    {
        var f = Regex.Match(m, @"^(Left|Right) (Thumb|Index|Middle|Ring|Little) (\d) Stretched$");
        if (f.Success) return $"{f.Groups[1].Value}Hand.{f.Groups[2].Value}.{f.Groups[3].Value} Stretched";
        var s = Regex.Match(m, @"^(Left|Right) (Thumb|Index|Middle|Ring|Little) Spread$");
        if (s.Success) return $"{s.Groups[1].Value}Hand.{s.Groups[2].Value}.Spread";
        return m;
    }

    /// The muscles after playing `clip` in one state, mirrored or not, directly or as a mirrored blend tree child.
    static float[] Play(Animator animator, AnimationClip clip, bool mirrorState, bool mirrorChild)
    {
        var ctrl = new AnimatorController();
        ctrl.AddParameter("p", AnimatorControllerParameterType.Float);
        ctrl.AddLayer("L");
        var st = ctrl.layers[0].stateMachine.AddState("s");
        st.mirror = mirrorState;
        st.motion = clip;
        if (mirrorChild)
        {
            var tree = new BlendTree { blendType = BlendTreeType.Simple1D, blendParameter = "p" };
            tree.children = new[] { new ChildMotion { motion = clip, threshold = 0, timeScale = 1, mirror = true } };
            st.motion = tree;
        }
        var graph = PlayableGraph.Create();
        var output = AnimationPlayableOutput.Create(graph, "o", animator);
        output.SetSourcePlayable(AnimatorControllerPlayable.Create(graph, ctrl));
        graph.Evaluate(0.1f);
        graph.Evaluate(0.1f);
        var pose = new HumanPose();
        new HumanPoseHandler(animator.avatar, animator.transform).GetHumanPose(ref pose);
        graph.Destroy();
        return pose.muscles;
    }
}
