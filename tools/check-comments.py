#!/usr/bin/env python3
"""Reports comments in src/ that do not appear in the V8 source.

A comment in the Rust source is kept only where V8 has one, copied as
written. This finds the ones that are not V8's.

Judging is per contiguous comment block, not per line, because the same
comment reflows to different line breaks in Rust: a block of seven words or
more matches on word shingles, so reflow is invisible, and a shorter block
has to appear verbatim as one of V8's own comment blocks or lines. Matching a
short comment against all of V8's comment text instead would pass anything
whose words occur somewhere, which is how section banners survived.

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

def collect(run):
    """One contiguous run of V8 comment lines.

    The run goes in whole, so a vx comment that reflowed it matches on
    shingles, and every line of it goes in on its own, so a vx comment that
    copied one line of it matches exactly.
    """
    text = normalise(" ".join(run))
    haystack.update(shingles(text.split()))
    exact.add(text)
    for line in run:
        exact.add(normalise(line))

haystack = set()
exact = set()
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
                # A trailing comment on a line of code counts too.
                trailing = line.partition("//")
                if trailing[1] and "\"" not in trailing[0]:
                    run.append(trailing[2].strip())
                    continue
                # A one-line block comment counts too: that is how V8
                # writes the section headers inside its macro lists.
                block = re.match(r".*?/\*(.*?)\*/", line)
                if block:
                    run.append(block.group(1).strip())
                    continue
                if run:
                    collect(run)
                    run = []
        if run:
            collect(run)

# A block of one or two words carries no evidence of having been copied: the
# word is bound to occur in V8 somewhere. Those are reported whatever V8 has.
MIN_WORDS = 3

def is_v8s(key):
    words = key.split()
    if len(words) < MIN_WORDS:
        return False
    if len(words) < SHINGLE:
        return key in exact
    return all(shingle in haystack for shingle in shingles(words))

ASSERT_MACRO = re.compile(r"\b(?:debug_)?assert(?:_eq|_ne)?!\(")

def assert_messages(path):
    """Production assertion macros carrying a message string.

    V8's DCHECK takes no message: the macro prints the expression and the
    values. A message is authored text, so it is held to the same rule as a
    comment. The three guards in heap.rs are deliberate FATAL-class
    diagnostics and are allowed to print the address they caught.
    """
    body = open(path).read().partition("#[cfg(test)]")[0]
    found = []
    position = 0
    while True:
        match = ASSERT_MACRO.search(body, position)
        if not match:
            return found
        depth = 0
        in_string = escaped = False
        commas = []
        index = match.end()
        while index < len(body):
            char = body[index]
            if in_string:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    in_string = False
            elif char == '"':
                in_string = True
            elif char in "([{":
                depth += 1
            elif char in ")]}":
                if depth == 0:
                    break
                depth -= 1
            elif char == "," and depth == 0:
                commas.append(index)
            index += 1
        keep = 1 if match.group(0).startswith(("debug_assert_eq", "debug_assert_ne",
                                               "assert_eq", "assert_ne")) else 0
        if len(commas) > keep and body[commas[keep] + 1 :].lstrip().startswith('"'):
            found.append(body[: match.start()].count("\n") + 1)
        position = index + 1

total = flagged = 0
for path in sorted(subprocess.run(["find", "src", "-name", "*.rs"],
                                  capture_output=True, text=True).stdout.split()):
    report = []
    block = []

    def judge(block):
        global flagged
        if not block:
            return
        key = normalise(" ".join(body for _, _, body in block))
        if is_v8s(key):
            return
        for number, stripped, _ in block:
            report.append((number, stripped))
            flagged += 1

    for number, line in enumerate(open(path), 1):
        stripped = line.strip()
        if not stripped.startswith("//"):
            judge(block)
            block = []
            continue
        body = stripped.lstrip("/!").strip()
        if not body:
            judge(block)
            block = []
            continue
        total += 1
        block.append((number, stripped, body))
    judge(block)
    messages = [] if path.endswith("heap/heap.rs") else assert_messages(path)
    for number in messages:
        report.append((number, "<assertion message>"))
        flagged += 1
    if report:
        report.sort()
        print(f"\n{path}  ({len(report)} not in V8)")
        for number, text in report[:200]:
            print(f"  {number:5d}  {text}")

print(f"\n{flagged} of {total} comment lines are not V8's", file=sys.stderr)
