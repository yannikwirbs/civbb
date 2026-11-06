// JSON text helpers shared by the GLB writer and the X_* extension writers (ADR 0041: serde_json's shapes).
using System.Globalization;
using System.Linq;
using System.Text;
using UnityEngine;

namespace Civbb.Exporter
{
    static class Json
    {
        /// A finite float; non-finite values become 0 (glTF numbers must be finite).
        public static string Num(float f) => float.IsFinite(f) ? f.ToString("R", CultureInfo.InvariantCulture) : "0";

        /// A key tangent: infinite (a step) becomes `null`, which the importer reads as +infinity.
        public static string Tangent(float f) => float.IsFinite(f) ? Num(f) : "null";

        public static string Nums(params float[] f) => "[" + string.Join(",", f.Select(Num)) + "]";

        public static string Bool(bool b) => b ? "true" : "false";

        /// A Unity position or direction in glTF space (ADR 0038's X mirror).
        public static string Vec(Vector3 v) => Nums(-v.x, v.y, v.z);

        /// A Unity rotation in glTF space.
        public static string Quat(Quaternion q) => Nums(q.x, -q.y, -q.z, q.w);

        /// Unity Euler degrees in glTF space (ADR 0039: y and z negate).
        public static string Euler(Vector3 e) => Nums(e.x, -e.y, -e.z);

        public static string Transform(Vector3 t, Quaternion r, Vector3 s) =>
            $"{{\"translation\":{Vec(t)},\"rotation\":{Quat(r)},\"scale\":{Nums(s.x, s.y, s.z)}}}";

        public static string Str(string s)
        {
            var sb = new StringBuilder("\"");
            foreach (var c in s ?? "")
            {
                if (c == '"' || c == '\\')
                {
                    sb.Append('\\').Append(c);
                }
                else if (c < 0x20)
                {
                    sb.Append("\\u").Append(((int)c).ToString("x4"));
                }
                else
                {
                    sb.Append(c);
                }
            }
            return sb.Append('"').ToString();
        }
    }
}
