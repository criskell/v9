#!/usr/bin/env python3
"""Removes the comment lines that check-comments.py reports as not V8's.

  tools/strip-comments.py src/objects/string.rs [...]

Mechanical half of rule 4: it deletes, it never writes a comment. Leftover
empty comment markers and the blank lines they leave behind go too, so the
result is the file with only V8's comments in it.
"""
import re, subprocess, sys, os

V8 = os.path.expanduser("~/devel/v8")
report = subprocess.run([sys.executable, "-I", "tools/check-comments.py", V8],
                        capture_output=True, text=True).stdout

flagged = {}
current = None
for line in report.splitlines():
    header = re.match(r"^(src/\S+\.rs)\s+\(\d+ not in V8\)", line)
    if header:
        current = header.group(1)
        flagged[current] = set()
        continue
    body = re.match(r"^\s+(\d+)\s\s(.*)$", line)
    if body and current:
        flagged[current].add(int(body.group(1)))

for path in sys.argv[1:]:
    drop = flagged.get(path, set())
    if not drop:
        print(f"{path}: nothing to strip")
        continue
    lines = open(path).read().splitlines()

    # The unit of decision is a contiguous comment block, not a line. A
    # layout diagram is one comment whose lines are mostly punctuation, so
    # judging it line by line keeps the frame and deletes the content.
    blocks, number = [], 0
    while number < len(lines):
        if lines[number].strip().startswith("//"):
            start = number
            while number < len(lines) and lines[number].strip().startswith("//"):
                number += 1
            blocks.append((start, number))
        else:
            number += 1

    discard = set()
    for start, end in blocks:
        numbers = range(start + 1, end + 1)
        hit = [n for n in numbers if n in drop]
        if not hit:
            continue
        text = [n for n in numbers
                if lines[n - 1].strip().lstrip("/!").strip()]
        fenced = any("```" in lines[n - 1] for n in numbers)
        if fenced or len(hit) * 2 >= len(text):
            discard.update(numbers)
        else:
            discard.update(hit)

    out = []
    for number, line in enumerate(lines, 1):
        if number in discard:
            continue
        if line.strip() in ("//", "///", "//!"):
            continue
        out.append(line)
    # Collapse the runs of blank lines the deletions opened up.
    text = re.sub(r"\n\n\n+", "\n\n", "\n".join(out)) + "\n"
    # A doc comment that now sits directly above nothing but a blank line.
    open(path, "w").write(text)
    print(f"{path}: stripped {len(drop)}")
