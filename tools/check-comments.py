#!/usr/bin/env python3
"""Reports comments in src/ that do not appear in the V8 source.

Rule 4 in PORTING.md: a comment in the Rust source is kept only where V8 has
one. This finds the ones that are not V8's, by normalising each comment line
and looking for it in the V8 files the module maps to (and, failing that,
anywhere in V8's src/, since a comment may have moved).

  tools/check-comments.py [v8-checkout]
"""
import os, re, subprocess, sys

V8 = os.path.expanduser(sys.argv[1] if len(sys.argv) > 1 else "~/devel/v8")

def normalise(text):
    text = re.sub(r"`([^`]*)`", r"\1", text)
    text = re.sub(r"\b([a-z0-9]+)_([a-z0-9_]+)\b", lambda m: m.group(0), text)
    return re.sub(r"[^a-z0-9]+", " ", text.lower()).strip()

SHINGLE = 7

def shingles(words):
    if len(words) <= SHINGLE:
        return [" ".join(words)]
    return [" ".join(words[i:i + SHINGLE]) for i in range(len(words) - SHINGLE + 1)]

# Every comment in V8's src/, as word shingles. Shingles rather than whole
# lines because the same comment reflows to different line breaks in Rust,
# and a line-by-line match would call that a difference.
haystack = set()
blob = []
for root, _, files in os.walk(os.path.join(V8, "src")):
    for name in files:
        if not name.endswith((".h", ".cc", ".tq")):
            continue
        run = []
        with open(os.path.join(root, name), errors="replace") as handle:
            for line in handle:
                line = line.strip()
                if line.startswith("//"):
                    run.append(line.lstrip("/").strip())
                    continue
                if run:
                    text = normalise(" ".join(run))
                    haystack.update(shingles(text.split()))
                    blob.append(text)
                    run = []
        if run:
            text = normalise(" ".join(run))
            haystack.update(shingles(text.split()))
            blob.append(text)

# A comment too short to shingle is looked for in the text itself.
blob = " | ".join(blob)

def is_v8s(key):
    words = key.split()
    if len(words) < SHINGLE:
        return key in blob
    return all(shingle in haystack for shingle in shingles(words))

total = flagged = 0
for path in sorted(subprocess.run(["find", "src", "-name", "*.rs"],
                                  capture_output=True, text=True).stdout.split()):
    report = []
    for number, line in enumerate(open(path), 1):
        stripped = line.strip()
        if not stripped.startswith("//"):
            continue
        body = stripped.lstrip("/!").strip()
        if not body:
            continue
        total += 1
        key = normalise(body)
        # A doc line that is only a V8 symbol name is the allowed exception.
        if re.fullmatch(r"`[A-Za-z_][A-Za-z0-9_:<>]*(\(\))?`\.?", body):
            continue
        if len(key) <= 12 or is_v8s(key):
            continue
        report.append((number, stripped))
        flagged += 1
    if report:
        print(f"\n{path}  ({len(report)} not in V8)")
        for number, text in report[:200]:
            print(f"  {number:5d}  {text}")

print(f"\n{flagged} of {total} comment lines are not V8's", file=sys.stderr)
