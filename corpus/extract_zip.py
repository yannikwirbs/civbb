"""Corpus tool. Extracts a zip into a new directory, refusing absolute or escaping paths. Decodes non-UTF-8 names as cp932."""
import os
import sys
import zipfile

src, dest = sys.argv[1], sys.argv[2]
os.makedirs(dest, exist_ok=False)
root = os.path.realpath(dest)
with zipfile.ZipFile(src) as z:
    for info in z.infolist():
        name = info.filename
        if not info.flag_bits & 0x800:
            try:
                name = name.encode("cp437").decode("cp932")
            except UnicodeError:
                pass
        target = os.path.realpath(os.path.join(root, name))
        if not target.startswith(root + os.sep) and target != root:
            raise SystemExit(f"refusing {name!r}")
        if name.endswith("/"):
            os.makedirs(target, exist_ok=True)
            continue
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with z.open(info) as f, open(target, "wb") as out:
            while chunk := f.read(1 << 20):
                out.write(chunk)
print("extracted", src, "->", dest)
