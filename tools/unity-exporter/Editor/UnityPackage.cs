// Lists the asset paths in a .unitypackage (a tar.gz of <guid>/pathname, asset, asset.meta) without importing it,
// so the import step can refuse packages that carry code (ADR 0038).
using System;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Text;

namespace Civbb.Exporter
{
    public static class UnityPackage
    {
        static readonly string[] CodeExtensions = { ".cs", ".dll", ".asmdef", ".asmref", ".rsp" };

        /// Asset paths in the package that would compile or load code when imported.
        public static List<string> CodeEntries(string path)
        {
            var code = new List<string>();
            foreach (var asset in AssetPaths(path))
            {
                foreach (var ext in CodeExtensions)
                {
                    if (asset.EndsWith(ext, StringComparison.OrdinalIgnoreCase))
                    {
                        code.Add(asset);
                    }
                }
            }
            return code;
        }

        /// Every asset path the package declares (the first line of each `<guid>/pathname` entry).
        public static List<string> AssetPaths(string path) => Entries(path).ConvertAll(e => e.path);

        /// Every asset the package declares as (guid, asset path).
        public static List<(string guid, string path)> Entries(string path)
        {
            var paths = new List<(string, string)>();
            using var gz = new GZipStream(File.OpenRead(path), CompressionMode.Decompress);
            var header = new byte[512];
            while (ReadFull(gz, header, 512))
            {
                if (Array.TrueForAll(header, b => b == 0))
                {
                    break;
                }
                var name = Field(header, 0, 100);
                var size = Octal(header, 124, 12);
                var padded = (size + 511) / 512 * 512;
                var type = (char)header[156];
                if ((type == '0' || type == '\0') && name.EndsWith("/pathname") && size <= 4096)
                {
                    var body = new byte[padded];
                    if (!ReadFull(gz, body, (int)padded))
                    {
                        throw new InvalidDataException("truncated package");
                    }
                    var text = Encoding.UTF8.GetString(body, 0, (int)size);
                    var guid = name.StartsWith("./") ? name.Substring(2) : name;
                    paths.Add((guid.Split('/')[0], text.Split('\n')[0].Trim()));
                }
                else
                {
                    Skip(gz, padded);
                }
            }
            return paths;
        }

        static string Field(byte[] h, int at, int len)
        {
            int end = at;
            while (end < at + len && h[end] != 0)
            {
                end++;
            }
            return Encoding.UTF8.GetString(h, at, end - at);
        }

        static long Octal(byte[] h, int at, int len)
        {
            if ((h[at] & 0x80) != 0)
            {
                throw new InvalidDataException("tar entry too large");
            }
            var s = Field(h, at, len).Trim(' ', '\0');
            return s.Length == 0 ? 0 : Convert.ToInt64(s, 8);
        }

        static bool ReadFull(Stream s, byte[] buf, int count)
        {
            int read = 0;
            while (read < count)
            {
                int n = s.Read(buf, read, count - read);
                if (n == 0)
                {
                    return false;
                }
                read += n;
            }
            return true;
        }

        static void Skip(Stream s, long count)
        {
            var buf = new byte[81920];
            while (count > 0)
            {
                int n = s.Read(buf, 0, (int)Math.Min(buf.Length, count));
                if (n == 0)
                {
                    throw new InvalidDataException("truncated package");
                }
                count -= n;
            }
        }
    }
}
