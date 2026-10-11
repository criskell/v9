#!/usr/bin/env python3
"""Reports comments in src/ that do not appear in the V8 source.

A comment in the Rust source is kept only where V8 has one at the same
construct, and then it is copied as written. This finds the ones that are not
V8's.

Two things are checked per contiguous comment block, not per line, because the
same comment reflows to different line breaks in Rust:

  * the block, normalised and rejoined, has to be a contiguous slice of one of
    V8's own comment blocks -- not merely a bag of words that occur in V8, and
    not spliced from two different comments;

  * that V8 comment has to live in a file this module maps onto, so a comment
    cannot be matched against something unrelated elsewhere in V8. Matching
    anywhere in V8 is how `>>>, logical shift right` passed by colliding with
    a comment about the ARM LSR instruction.

  tools/check-comments.py [v8-checkout]
"""
import os, re, subprocess, sys

V8 = os.path.expanduser(sys.argv[1] if len(sys.argv) > 1 else "~/devel/v8")

def normalise(text):
    text = re.sub(r"`([^`]*)`", r"\1", text)
    return re.sub(r"[^a-z0-9]+", " ", text.lower()).strip()

def comment_blocks(path):
    """Every contiguous run of comment lines in a V8 file, each kept whole."""
    blocks, run = [], []
    with open(path, errors="replace") as handle:
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
            # A one-line block comment counts too: that is how V8 writes the
            # section headers inside its macro lists.
            block = re.match(r".*?/\*(.*?)\*/", line)
            if block:
                run.append(block.group(1).strip())
                continue
            if run:
                blocks.append(normalise(" ".join(run)))
                run = []
    if run:
        blocks.append(normalise(" ".join(run)))
    return [block for block in blocks if block]

DIRECTORY_CACHE = {}

def mapped_comments(rel):
    """The comments of the V8 directory `src/<rel>` maps onto.

    Not just `<stem>.h`: V8 scatters one header's implementations across
    neighbours (`HashTable::FindEntry` is in hash-table.h, but
    `UpdateMaxNumberKey` and `HasSufficientCapacityToAdd` are in objects.cc),
    while vx groups by header. The directory is the unit that still rules out
    matching a comment against an unrelated part of V8.
    """
    directory = os.path.dirname(rel)
    if directory in DIRECTORY_CACHE:
        return DIRECTORY_CACHE[directory]
    root = os.path.join(V8, "src", directory)
    blocks = []
    if os.path.isdir(root):
        for name in sorted(os.listdir(root)):
            if name.endswith((".h", ".cc", ".tq")):
                blocks += comment_blocks(os.path.join(root, name))
    DIRECTORY_CACHE[directory] = blocks
    return blocks

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
    pool = mapped_comments(path[len("src/"):])
    report = []
    block = []

    def judge(block):
        global flagged
        if not block:
            return
        key = normalise(" ".join(body for _, _, body in block))
        if key and any(key in v8s for v8s in pool):
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
