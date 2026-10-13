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
                'codegen/label.h', 'common/globals.h', 'utils/utils.h')


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
        for body in re.finditer(r'enum\s+(?:class\s+)?\w*[^{;]*\{([^}]*)\}', src, re.S):
            members = re.sub(r'//[^\n]*', '', body.group(1))
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
    for directory in ('src/codegen/arm64', 'src/execution/arm64'):
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
    return declared


# Constants vx declares that V8 has no name for, each with what it stands in
# for.
VX_ONLY = {
    'K_STACK_SIZE': "the simulator's stack is an arena; V8 runs on the host stack",
    'NO_LANE': 'a VOperand with no lane index; C++ picks the overload instead',
}


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
    print(f'\n{len(unknown)} of {len(declared)} arm64 constants are not spelled '
          f'the way V8 spells them')
    return 1 if unknown else 0


if __name__ == '__main__':
    sys.exit(main())
