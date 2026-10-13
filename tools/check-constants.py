#!/usr/bin/env python3
"""Checks that every constant vx declares for arm64 is spelled exactly as V8
spells it.

The arm64 constant names carry information that a mechanical case conversion
destroys: `ImmAddSub_offset` and `LSOpc_mask` cannot be recovered from
`IMM_ADD_SUB_OFFSET` and `LS_OPC_MASK`, because the acronym boundaries are
gone. So vx keeps V8's spelling verbatim and this tool enforces it, which
turns the `#![allow(non_upper_case_globals)]` in the constants module into
the counterpart of an invariant rather than a hole.

Run it against a V8 checkout:

  tools/check-constants.py ~/devel/v8
"""
import os
import re
import sys

ARM64_DIRS = ('codegen/arm64', 'execution/arm64', 'diagnostics/arm64')
# vx's codegen/{assembler,label}.rs and utils/utils.rs map onto these, and the
# arm64 modules use the constants they declare.
SHARED_FILES = ('codegen/register-base.h', 'codegen/assembler.h',
                'codegen/label.h', 'codegen/reloc-info.h', 'codegen/cpu-features.h',
                'common/globals.h', 'utils/utils.h', 'base/vlq.h')


def v8_names(v8_root):
    """Every constant-ish name V8 declares for arm64, macro lists expanded."""
    names = set()
    sources = []
    for directory in ARM64_DIRS:
        path = os.path.join(v8_root, 'src', directory)
        if not os.path.isdir(path):
            continue
        for entry in sorted(os.listdir(path)):
            if entry.endswith(('.h', '.cc')):
                sources.append(open(os.path.join(path, entry),
                                    encoding='utf8', errors='ignore').read())
    for shared in SHARED_FILES:
        path = os.path.join(v8_root, 'src', shared)
        if os.path.isfile(path):
            sources.append(open(path, encoding='utf8', errors='ignore').read())
    for src in sources:
        names |= set(re.findall(
            r'constexpr\s+(?:V8_EXPORT_PRIVATE\s+)?[A-Za-z_][\w:<>]*\s+(\w+)\s*=', src))
        names |= set(re.findall(
            r'^\s*(?:V8_EXPORT_PRIVATE\s+)?const\s+[A-Za-z_][\w:<>]*\s+(?:\w+::)?(\w+)\s*=',
            src, re.M))
        names |= set(re.findall(r'static\s+const(?:expr)?\s+[\w:<>]+\s+(\w+)\s*=', src))
        for start in re.finditer(r'enum\s+(?:class\s+)?\w*[^{;]*\{', src):
            depth, i = 0, start.end() - 1
            while i < len(src):
                if src[i] == '{':
                    depth += 1
                elif src[i] == '}':
                    depth -= 1
                    if depth == 0:
                        break
                i += 1
            members = re.sub(r'//[^\n]*', '', src[start.end():i])
            members = re.sub(r'^\s*#[^\n]*', '', members, flags=re.M)
            for member in members.split(','):
                match = re.match(r'\s*(\w+)', member)
                if match:
                    names.add(match.group(1))
        names |= set(re.findall(
            r'(?:DEFINE_REGISTER|ALIAS_REGISTER)\(\s*\w+\s*,\s*(\w+)', src))

    constants = open(os.path.join(v8_root, 'src/codegen/arm64/constants-arm64.h'),
                     encoding='utf8').read()

    for list_name in ('INSTRUCTION_FIELDS_LIST', 'SYSTEM_REGISTER_FIELDS_LIST'):
        block = re.search(r'#define ' + list_name + r'\([^)]*\)(.*?)\n\n',
                          constants, re.S)
        if not block:
            continue
        for field in re.findall(r'V_\((\w+),\s*\d+,\s*\d+', block.group(1)):
            for suffix in ('_offset', '_width', '_mask'):
                names.add(field + suffix)

    load_store = re.search(r'#define LOAD_STORE_OP_LIST\(V\)(.*?)\n\n',
                           constants, re.S)
    if load_store:
        for a, b, c in re.findall(r'V\((\w+),\s*(\w+),\s*(\w+),', load_store.group(1)):
            names.add(f'{a}U{b}_{c}')
            names.add(f'{a}{b}_{c}')
            for suffix in ('_post', '_pre', '_unsigned', '_reg'):
                names.add(f'{a}{b}_{c}{suffix}')

    pair = re.search(r'#define LOAD_STORE_PAIR_OP_LIST\(V\)(.*?)\n\n',
                     constants, re.S)
    if pair:
        for a, b in re.findall(r'V\((\w+),\s*(\w+),', pair.group(1)):
            names.add(f'{a}_{b}')
            for suffix in ('_post', '_pre', '_off'):
                names.add(f'{a}_{b}{suffix}')

    add_sub = re.search(r'#define ADD_SUB_OP_LIST\(V\)(.*?)\n\n', constants, re.S)
    if add_sub:
        for op in re.findall(r'V\((\w+)\)', add_sub.group(1)):
            for size in ('w', 'x'):
                for form in ('imm', 'shift', 'ext'):
                    names.add(f'{op}_{size}_{form}')

    atomic = re.search(r'#define ATOMIC_MEMORY_SIMPLE_OPC_LIST\(V\)(.*?)\n\n',
                       constants, re.S)
    if atomic:
        modes = ('Op', 'B', 'AB', 'LB', 'ALB', 'H', 'AH', 'LH', 'ALH',
                 '_w', 'A_w', 'L_w', 'AL_w', '_x', 'A_x', 'L_x', 'AL_x')
        for op in re.findall(r'V\((\w+),', atomic.group(1)):
            for mode in modes:
                names.add(op + mode)

    for prefix in 'wxbhsdqvz':
        for number in range(32):
            names.add(f'{prefix}{number}')
    return names


def vx_constants(vx_root):
    """Every constant vx declares in its arm64 modules, with its file."""
    declared = []
    for directory in ('src/codegen/arm64', 'src/execution/arm64',
                      'src/diagnostics/arm64'):
        if not os.path.isdir(os.path.join(vx_root, directory)):
            continue
        for entry in sorted(os.listdir(os.path.join(vx_root, directory))):
            if not entry.endswith('.rs'):
                continue
            relative = os.path.join(directory, entry)
            src = open(os.path.join(vx_root, relative), encoding='utf8').read()
            for name in re.findall(r'pub const (\w+)\s*:', src):
                declared.append((relative, name))
            for macro in re.finditer(r'define_registers!\((.*?)\);', src, re.S):
                for pair in re.findall(r'\((\w+), (\w+), ', macro.group(1)):
                    declared += [(relative, pair[0]), (relative, pair[1])]
            for macro in re.finditer(r'define_v_registers!\((.*?)\);', src, re.S):
                for row in re.findall(
                        r'\((\w+), (\w+), (\w+), (\w+), (\w+), (\w+), ', macro.group(1)):
                    declared += [(relative, name) for name in row]
    for relative in ('src/codegen/reloc_info.rs', 'src/codegen/cpu_features.rs',
                     'src/base/vlq.rs'):
        path = os.path.join(vx_root, relative)
        if not os.path.isfile(path):
            continue
        src = open(path, encoding='utf8').read()
        for name in re.findall(r'pub const (\w+)\s*:', src):
            declared.append((relative[4:], name))
    return declared


# Constants vx declares that V8 has no name for, each with what it stands in
# for.
VX_ONLY = {
    'K_STACK_SIZE': "the simulator's stack is an arena; V8 runs on the host stack",
    'NO_LANE': 'a VOperand with no lane index; C++ picks the overload instead',
}



def value_mismatches(v8_root, vx_root):
    """Constants whose vx value differs from the value V8 computes."""
    text = open(os.path.join(v8_root, 'src/codegen/arm64/constants-arm64.h')).read()

    def collect_macros(t):
        macros = {}
        for m in re.finditer(r'#define (\w+)\(([^)]*)\)((?:[^\n]*\\\n)*[^\n]*)', t):
            macros[m.group(1)] = ([p.strip() for p in m.group(2).split(',') if p.strip()],
                                  m.group(3).replace('\\\n', '\n'))
        return macros

    macros = collect_macros(text)

    def split_args(s):
        args, depth, cur, q = [], 0, '', False
        for ch in s:
            if q:
                cur += ch
                if ch == '"': q = False
                continue
            if ch == '"': q = True; cur += ch; continue
            if ch == ',' and depth == 0: args.append(cur); cur = ''; continue
            if ch == '(': depth += 1
            elif ch == ')': depth -= 1
            cur += ch
        args.append(cur)
        return args

    pieces = []
    for m in re.finditer(r'\b([A-Z][A-Z_0-9]+)\((\w+)\)', text):
        list_name, macro = m.group(1), m.group(2)
        if list_name not in macros or macro not in macros:
            continue
        _, list_body = macros[list_name]
        params, body = macros[macro]
        for call in re.finditer(r'\bV\(([^()]*(?:\([^()]*\)[^()]*)*)\)', list_body):
            piece = body
            for p, a in zip(params, split_args(call.group(1))):
                piece = re.sub(r'\b'+p+r'\b', a.strip(), piece)
            piece = re.sub(r'\s*##\s*', '', piece).strip()
            if not piece.endswith(';'):
                piece += ';'
            pieces.append(piece)

    whole = re.sub(r'//[^\n]*', '', text + '\n' + '\n'.join(pieces))
    v8 = {}
    for name, expr in re.findall(r'constexpr\s+\w+\s+(\w+)\s*=\s*([^;]+);', whole, re.S):
        v8.setdefault(name, ' '.join(expr.split()))
    # enum members with explicit values
    for block in re.finditer(r'enum \w+ \{(.*?)\};', whole, re.S):
        for item in block.group(1).split(','):
            m = re.match(r'\s*(\w+)\s*=\s*(.+)', item, re.S)
            if m:
                v8.setdefault(m.group(1), ' '.join(m.group(2).split()))

    def evaluate(name, table, seen=None):
        seen = seen or set()
        if name in seen:
            return None
        expr = table.get(name)
        if expr is None:
            return None
        e = expr
        e = re.sub(r'\bstatic_cast<[^>]*>', '', e)
        e = re.sub(r'\b(0x[0-9a-fA-F]+|\d+)[uUlL]+\b', r'\1', e)
        names = set(re.findall(r'\b([A-Za-z_]\w*)\b', e))
        env = {}
        for n in names:
            if n in ('u', 'U', 'L'):
                continue
            value = evaluate(n, table, seen | {name})
            if value is None:
                return None
            env[n] = value
        try:
            return eval(e, {'__builtins__': {}}, env) & 0xFFFFFFFFFFFFFFFF
        except Exception:
            return None

    vx_text = open(os.path.join(vx_root, 'src/codegen/arm64/constants_arm64.rs'),
                   encoding='utf8').read()
    vx = {}
    for m in re.finditer(r'pub const (\w+)\s*:\s*\w+\s*=\s*([^;]+);', vx_text):
        vx[m.group(1)] = ' '.join(m.group(2).split())

    bad = []
    checked = 0
    for name, expr in sorted(vx.items()):
        if name not in v8:
            continue
        want = evaluate(name, v8)
        got = evaluate(name, vx)
        if want is None or got is None:
            continue
        checked += 1
        if want != got:
            bad.append((name, got, want))
    return bad, checked


def main():
    if len(sys.argv) < 2:
        print(f'usage: {sys.argv[0]} <path-to-v8-checkout>', file=sys.stderr)
        return 2
    v8_root = sys.argv[1]
    if not os.path.isdir(os.path.join(v8_root, 'src/codegen/arm64')):
        print(f'usage: {sys.argv[0]} <path-to-v8-checkout>', file=sys.stderr)
        return 2
    vx_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

    known = v8_names(v8_root)
    declared = vx_constants(vx_root)
    unknown = [(path, name) for path, name in declared
               if name not in known and name not in VX_ONLY]

    for path, name in unknown:
        print(f'{path:42}  {name}')
    print(f'{len(unknown)} of {len(declared)} arm64 constants are not spelled '
          f'the way V8 spells them')

    bad, checked = value_mismatches(v8_root, vx_root)
    for name, got, want in bad:
        print(f'{name:42}  vx=0x{got:X}  v8=0x{want:X}')
    print(f'{len(bad)} of {checked} arm64 constants have a different value from V8')
    return 1 if unknown or bad else 0


if __name__ == '__main__':
    sys.exit(main())
