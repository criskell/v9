#!/usr/bin/env python3
"""Rewrites a V8 definition into a Rust draft, over the C++ syntax tree.

    tools/v8-draft.py LookupIterator::Delete
    tools/v8-draft.py MigrateSlowToFast

Needs tree-sitter:  python3 -m pip install --user tree-sitter tree-sitter-cpp

The output does not compile and is not committed. It preserves statement
order and applies the conversions that are always the same conversion, so
what is left is the part that needs judgement. A node the rules do not
recognise is emitted as its original C++ text, so it is visible rather than
silently wrong.
"""
import argparse
import os
import re
import sys

try:
    import tree_sitter_cpp
    from tree_sitter import Language, Parser
except ModuleNotFoundError:
    sys.exit(
        "needs tree-sitter: python3 -m pip install --user tree-sitter tree-sitter-cpp"
    )

DROPPED_CALLS = {
    "RCS_SCOPE",
    "DisallowGarbageCollection",
    "DisallowHeapAllocation",
    "HandleScope",
    "USE",
    "LOG",
}
ASSERTS = {
    "DCHECK": "debug_assert!",
    "DCHECK_EQ": "debug_assert_eq!",
    "DCHECK_NE": "debug_assert_ne!",
    "DCHECK_LE": "debug_assert!",
    "DCHECK_LT": "debug_assert!",
    "DCHECK_GE": "debug_assert!",
    "DCHECK_GT": "debug_assert!",
    "DCHECK_NOT_NULL": "debug_assert!",
    "DCHECK_IMPLIES": "debug_assert!",
    "CHECK": "assert!",
    "CHECK_EQ": "assert_eq!",
}
STRIPPED_CASTS = re.compile(r"^(Cast|UncheckedCast|TryCast|direct_handle|handle|Just|indirect_handle)$")


def snake(name):
    name = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", name)
    name = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", name)
    return name.lower()


def constant(name):
    return "K_" + snake(name[1:]).upper()


class Draft:
    def __init__(self, source):
        self.source = source

    def text(self, node):
        return self.source[node.start_byte : node.end_byte].decode("utf-8", "replace")

    def children(self, node, *types):
        return [child for child in node.named_children if child.type in types]

    def field(self, node, name):
        return node.child_by_field_name(name)

    # -- expressions ----------------------------------------------------

    def expression(self, node):
        if node is None:
            return ""
        kind = node.type
        if kind == "condition_clause":
            inner = [c for c in node.named_children if c.type != "comment"]
            return self.expression(inner[-1]) if inner else ""
        handler = getattr(self, "expression_" + kind, None)
        if handler:
            return handler(node)
        if kind == "identifier":
            return self.identifier(self.text(node))
        if kind in ("number_literal", "string_literal", "char_literal", "true", "false"):
            return self.text(node)
        if kind == "null":
            return "None"
        if kind == "parenthesized_expression":
            inner = node.named_children
            return "(" + self.expression(inner[0]) + ")" if inner else "()"
        if kind in ("qualified_identifier", "field_identifier", "type_identifier"):
            return self.qualified(self.text(node))
        return self.text(node)

    def identifier(self, name):
        if name.endswith("_") and not name.endswith("__"):
            return "self." + name[:-1]
        if name in ("isolate", "isolate_"):
            return "self"
        if re.match(r"^k[A-Z]", name):
            return constant(name)
        return name

    def qualified(self, text):
        if "::" in text:
            head, _, tail = text.rpartition("::")
            if re.match(r"^k[A-Z]", tail):
                return f"{head}::{constant(tail)}"
            if tail[:1].isupper() and head[:1].isupper():
                return snake(tail)
            return text
        return self.identifier(text)

    def expression_conditional_expression(self, node):
        condition = self.expression(self.field(node, "condition"))
        consequence = self.expression(self.field(node, "consequence"))
        alternative = self.expression(self.field(node, "alternative"))
        return f"if {condition} {{ {consequence} }} else {{ {alternative} }}"

    def expression_binary_expression(self, node):
        left = self.expression(self.field(node, "left"))
        right = self.expression(self.field(node, "right"))
        operator = self.text(node.child_by_field_name("operator"))
        return f"{left} {operator} {right}"

    def expression_unary_expression(self, node):
        operator = self.text(node.child_by_field_name("operator"))
        argument = self.expression(self.field(node, "argument"))
        return f"{operator}{argument}"

    def expression_pointer_expression(self, node):
        return self.expression(self.field(node, "argument"))

    def expression_assignment_expression(self, node):
        left = self.expression(self.field(node, "left"))
        right = self.expression(self.field(node, "right"))
        operator = self.text(node.child_by_field_name("operator"))
        return f"{left} {operator} {right}"

    def expression_field_expression(self, node):
        target = self.expression(self.field(node, "argument"))
        name = self.text(self.field(node, "field"))
        if name[:1].isupper():
            name = snake(name)
        return f"{target}.{name}"

    def expression_call_expression(self, node):
        function = self.field(node, "function")
        arguments = self.field(node, "arguments")
        given = [
            child for child in arguments.named_children if child.type != "comment"
        ] if arguments else []
        name = self.text(function)

        bare = name.rpartition("::")[2]
        if bare in DROPPED_CALLS:
            return ""
        if bare in ASSERTS:
            macro = ASSERTS[bare]
            if macro.endswith("_eq!") or macro.endswith("_ne!"):
                rendered = ", ".join(self.expression(a) for a in given[:2])
            else:
                rendered = ", ".join(self.expression(a) for a in given[:1])
            return f"{macro}({rendered})"
        if bare == "UNREACHABLE":
            return "unreachable!()"
        if STRIPPED_CASTS.match(bare) or re.match(r"^Cast<", name):
            return self.expression(given[0]) if given else ""

        rendered = [self.expression(a) for a in given]
        rendered = [a for a in rendered if a not in ("self", "self.isolate", "")]

        if function.type == "qualified_identifier":
            method = snake(name.rpartition("::")[2])
            return f"self.{method}({', '.join(rendered)})"
        if function.type == "field_expression":
            target = self.expression(self.field(function, "argument"))
            method = self.text(self.field(function, "field"))
            if target in ("self.factory", "self"):
                return f"self.{snake(method)}({', '.join(rendered)})"
            return f"{target}.{snake(method)}({', '.join(rendered)})"
        if function.type == "template_function":
            method = snake(self.text(self.field(function, "name")))
            return f"self.{method}({', '.join(rendered)})"
        if function.type == "identifier" and name[:1].islower():
            return f"self.{name}({', '.join(rendered)})"
        if function.type == "identifier" and name.startswith("Is"):
            return f"{self.expression(given[0]) if given else ''}.{snake(name)}()"
        return f"{self.expression(function)}({', '.join(rendered)})"

    # -- statements -----------------------------------------------------

    def statement(self, node, indent):
        pad = "    " * indent
        kind = node.type
        handler = getattr(self, "statement_" + kind, None)
        if handler:
            return handler(node, indent)
        if kind == "expression_statement":
            inner = node.named_children
            if not inner:
                return []
            rendered = self.expression(inner[0])
            return [f"{pad}{rendered};"] if rendered else []
        if kind == "comment":
            return []
        if kind.startswith("preproc"):
            return []
        return [f"{pad}{line}" for line in self.text(node).split("\n")]

    def statement_compound_statement(self, node, indent):
        out = []
        for child in node.named_children:
            out.extend(self.statement(child, indent))
        return out

    def statement_declaration(self, node, indent):
        pad = "    " * indent
        out = []
        for declarator in self.children(node, "init_declarator", "identifier", "pointer_declarator", "reference_declarator", "function_declarator"):
            if declarator.type == "init_declarator":
                name = self.text(self.field(declarator, "declarator"))
                value = self.expression(self.field(declarator, "value"))
                out.append(f"{pad}let {self.declared(name)} = {value};")
            else:
                name = self.text(declarator)
                if "(" in name:
                    out.append(f"{pad}let {self.declared(name.split('(')[0])} = {name};")
                else:
                    out.append(f"{pad}let {self.declared(name)};")
        return out or [f"{pad}{self.text(node)}"]

    def declared(self, name):
        return re.sub(r"[*&\s]", "", name)

    def statement_if_statement(self, node, indent):
        pad = "    " * indent
        condition = self.expression(self.field(node, "condition"))
        out = [f"{pad}if {condition} {{"]
        out.extend(self.block(self.field(node, "consequence"), indent))
        alternative = self.field(node, "alternative")
        if alternative is not None:
            target = alternative.named_children[0] if alternative.named_children else None
            if target is not None and target.type == "if_statement":
                nested = self.statement(target, indent)
                out.append(f"{pad}}} else {nested[0].strip()}")
                out.extend(nested[1:])
                return out
            out.append(f"{pad}}} else {{")
            out.extend(self.block(target, indent))
        out.append(f"{pad}}}")
        return out

    def statement_for_range_loop(self, node, indent):
        pad = "    " * indent
        name = self.text(self.field(node, "declarator"))
        subject = self.expression(self.field(node, "right"))
        subject = re.sub(r"^self\.range\((.*)\)$", r"0..\1", subject)
        out = [f"{pad}for {self.declared(name)} in {subject} {{"]
        out.extend(self.block(self.field(node, "body"), indent))
        out.append(f"{pad}}}")
        return out

    def statement_for_statement(self, node, indent):
        pad = "    " * indent
        initializer = self.field(node, "initializer")
        condition = self.expression(self.field(node, "condition"))
        name = ""
        if initializer is not None:
            declared = self.children(initializer, "init_declarator")
            if declared:
                name = self.declared(self.text(self.field(declared[0], "declarator")))
        limit = condition.partition("<")[2].strip() or condition
        out = [f"{pad}for {name or '_'} in 0..{limit} {{"]
        out.extend(self.block(self.field(node, "body"), indent))
        out.append(f"{pad}}}")
        return out

    def statement_while_statement(self, node, indent):
        pad = "    " * indent
        out = [f"{pad}while {self.expression(self.field(node, 'condition'))} {{"]
        out.extend(self.block(self.field(node, "body"), indent))
        out.append(f"{pad}}}")
        return out

    def statement_return_statement(self, node, indent):
        pad = "    " * indent
        inner = [child for child in node.named_children if child.type != "comment"]
        if not inner:
            return [f"{pad}return;"]
        return [f"{pad}return {self.expression(inner[0])};"]

    def statement_switch_statement(self, node, indent):
        pad = "    " * indent
        out = [f"{pad}match {self.expression(self.field(node, 'condition'))} {{"]
        body = self.field(node, "body")
        for child in body.named_children if body else []:
            if child.type != "case_statement":
                continue
            value = self.field(child, "value")
            label = self.expression(value) if value is not None else "_"
            out.append("    " * (indent + 1) + f"{label} => {{")
            for statement in child.named_children[1 if value is not None else 0 :]:
                if statement.type == "break_statement":
                    continue
                out.extend(self.statement(statement, indent + 2))
            out.append("    " * (indent + 1) + "}")
        out.append(f"{pad}}}")
        return out

    def block(self, node, indent):
        if node is None:
            return []
        if node.type == "compound_statement":
            return self.statement_compound_statement(node, indent + 1)
        return self.statement(node, indent + 1)

    # -- signature ------------------------------------------------------

    def signature(self, node):
        declarator = self.field(node, "declarator")
        while declarator is not None and declarator.type != "function_declarator":
            declarator = self.field(declarator, "declarator")
        if declarator is None:
            return "fn unknown(&mut self) {"
        name = self.text(self.field(declarator, "declarator")).rpartition("::")[2]
        parameters = self.field(declarator, "parameters")
        rendered = ["&mut self"]
        for parameter in parameters.named_children if parameters else []:
            if parameter.type != "parameter_declaration":
                continue
            text = self.text(parameter)
            if "isolate" in text.lower():
                continue
            inner = self.field(parameter, "declarator")
            if inner is None:
                continue
            kind = self.text(self.field(parameter, "type"))
            rendered.append(f"{self.declared(self.text(inner))}: {kind}")
        returns = self.text(self.field(node, "type"))
        tail = "" if returns == "void" else f" -> {returns}"
        return f"fn {snake(name)}({', '.join(rendered)}){tail} {{"


def definitions(tree, source, name):
    wanted = name.rpartition("::")[2]
    qualified = "::" in name
    stack = [tree.root_node]
    while stack:
        node = stack.pop()
        if node.type == "function_definition":
            declarator = node.child_by_field_name("declarator")
            while declarator is not None and declarator.type != "function_declarator":
                declarator = declarator.child_by_field_name("declarator")
            if declarator is not None:
                spelled = source[
                    declarator.child_by_field_name("declarator").start_byte : declarator.child_by_field_name(
                        "declarator"
                    ).end_byte
                ].decode("utf-8", "replace")
                if spelled == name or (not qualified and spelled.rpartition("::")[2] == wanted):
                    yield node
        stack.extend(node.children)


def main():
    parser = argparse.ArgumentParser(add_help=True)
    parser.add_argument("name")
    parser.add_argument(
        "--v8", default=os.environ.get("V8_ROOT", os.path.expanduser("~/devel/v8"))
    )
    arguments = parser.parse_args()
    if not os.path.isdir(os.path.join(arguments.v8, "src")):
        sys.exit(f"usage: {sys.argv[0]} <Name> [--v8 <path-to-v8-checkout>]")

    language = Language(tree_sitter_cpp.language())
    cpp = Parser(language)
    found = 0
    for directory, _, files in os.walk(os.path.join(arguments.v8, "src")):
        for base in sorted(files):
            if not base.endswith((".cc", ".h")):
                continue
            path = os.path.join(directory, base)
            source = open(path, "rb").read()
            if arguments.name.rpartition("::")[2].encode() not in source:
                continue
            tree = cpp.parse(source)
            draft = Draft(source)
            for node in definitions(tree, source, arguments.name):
                found += 1
                line = source[: node.start_byte].count(b"\n") + 1
                print(f"=== {os.path.relpath(path, arguments.v8)}:{line} ===")
                print(draft.signature(node))
                for rendered in draft.block(node.child_by_field_name("body"), 0):
                    print(rendered)
                print("}")
                print()
    if not found:
        sys.exit(f"no definition of {arguments.name} found in {arguments.v8}/src")


main()
