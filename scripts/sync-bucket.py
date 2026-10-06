"""Copy the hand-maintained fields of packaging/scoop/cash.json (notes, suggest, hooks)
into a Scoop bucket's bucket/cash.json, keeping the bucket's version, url and hash, which
its Excavator action owns (RELEASING.md, step 7).

    python scripts/sync-bucket.py C:\\path\\to\\scoop-bucket\\bucket\\cash.json
"""

import json
import pathlib
import sys

SOURCE = pathlib.Path(__file__).resolve().parent.parent / "packaging" / "scoop" / "cash.json"
FIELDS = [
    "description", "homepage", "license", "notes", "suggest", "bin", "persist",
    "post_install", "pre_uninstall", "checkver", "autoupdate",
]


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    bucket_path = pathlib.Path(sys.argv[1])
    source = json.loads(SOURCE.read_text(encoding="utf-8"))
    bucket = json.loads(bucket_path.read_text(encoding="utf-8"))
    changed = [f for f in FIELDS if f in source and bucket.get(f) != source[f]]
    for field in changed:
        bucket[field] = source[field]
    # Scoop's bucket style: four-space indent, LF, a final newline.
    with bucket_path.open("w", encoding="utf-8", newline="\n") as out:
        json.dump(bucket, out, indent=4, ensure_ascii=False)
        out.write("\n")
    print("changed:", ", ".join(changed) or "nothing")
    return 0


if __name__ == "__main__":
    sys.exit(main())
