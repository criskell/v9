//! Parser behaviour, checked against what JavaScript actually means.
//!
//! These are deliberately written as *semantic* claims -- "`a - b - c` groups
//! left", "a newline after `return` ends the statement" -- rather than as
//! structural comparisons against V8's output, because the parser is a
//! reconstruction of V8's grammar handling and not a transcription of its
//! code. A test that only checked the shape of the tree would pass while
//! getting the language wrong.

use vx::ast::ast::*;
use vx::ast::prettyprinter::AstPrinter;
use vx::parsing::parser::Parser;

fn parse(source: &str) -> Program {
    match Parser::parse_program(source) {
        Ok(program) => program,
        Err(error) => panic!("{}", error.display(source)),
    }
}

/// The names a scope declares, excluding parameters -- the scope's own
/// `var` bindings, which is what hoisting decides.
fn declared_vars(scope: &vx::ast::scopes::Scope) -> Vec<&str> {
    scope
        .variables()
        .iter()
        .filter(|variable| !variable.is_parameter)
        .map(|variable| variable.name.as_str())
        .collect()
}

fn parse_error(source: &str) -> String {
    match Parser::parse_program(source) {
        Ok(_) => panic!("expected {source:?} to fail to parse"),
        Err(error) => error.message,
    }
}

/// The single expression of a one-statement program.
fn expression_of(source: &str) -> Expression {
    let program = parse(source);
    assert_eq!(
        program.body.len(),
        1,
        "expected one statement in {source:?}"
    );
    match &program.body[0] {
        Statement::Expression { expression, .. } => expression.clone(),
        other => panic!("expected an expression statement, got {other:?}"),
    }
}

/// Renders an expression as a fully parenthesised string, which makes
/// associativity and precedence directly assertable.
fn shape(expression: &Expression) -> String {
    match expression {
        Expression::Literal { value, .. } => match value {
            Literal::Smi(v) => v.to_string(),
            Literal::Number(v) => v.to_string(),
            Literal::String(v) => format!("{v:?}"),
            Literal::Boolean(v) => v.to_string(),
            Literal::Null => "null".to_string(),
            Literal::Undefined => "undefined".to_string(),
        },
        Expression::VariableProxy { name, .. } => name.clone(),
        Expression::This { .. } => "this".to_string(),
        Expression::Binary {
            op, left, right, ..
        }
        | Expression::Compare {
            op, left, right, ..
        }
        | Expression::Logical {
            op, left, right, ..
        } => {
            format!("({} {} {})", shape(left), op, shape(right))
        }
        Expression::Assignment {
            op, target, value, ..
        } => format!("({} {} {})", shape(target), op, shape(value)),
        // `typeof a`, not `typeofa`: a word-shaped operator needs a space.
        Expression::Unary { op, expression, .. } => {
            let separator = if op
                .to_string()
                .chars()
                .next()
                .is_some_and(char::is_alphabetic)
            {
                " "
            } else {
                ""
            };
            format!("({op}{separator}{})", shape(expression))
        }
        Expression::Count {
            op,
            is_prefix,
            expression,
            ..
        } => {
            if *is_prefix {
                format!("({}{})", op, shape(expression))
            } else {
                format!("({}{})", shape(expression), op)
            }
        }
        Expression::Conditional {
            condition,
            then_expression,
            else_expression,
            ..
        } => format!(
            "({} ? {} : {})",
            shape(condition),
            shape(then_expression),
            shape(else_expression)
        ),
        Expression::Property { object, key, .. } => match key.as_ref() {
            PropertyAccess::Named(name) => format!("{}.{}", shape(object), name),
            PropertyAccess::Keyed(key) => format!("{}[{}]", shape(object), shape(key)),
        },
        Expression::CallNew {
            callee, arguments, ..
        } => format!(
            "new {}({})",
            shape(callee),
            arguments.iter().map(shape).collect::<Vec<_>>().join(", ")
        ),
        Expression::Call {
            callee, arguments, ..
        } => format!(
            "{}({})",
            shape(callee),
            arguments.iter().map(shape).collect::<Vec<_>>().join(", ")
        ),
        Expression::ObjectLiteral { properties, .. } => format!(
            "{{{}}}",
            properties
                .iter()
                .map(|p| match &p.key {
                    PropertyKey::Name(name) => format!("{name}: {}", shape(&p.value)),
                    PropertyKey::Computed(key) => format!("[{}]: {}", shape(key), shape(&p.value)),
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expression::ArrayLiteral { values, .. } => {
            format!(
                "[{}]",
                values.iter().map(shape).collect::<Vec<_>>().join(", ")
            )
        }
        Expression::FunctionLiteral { function, .. } => format!(
            "function {}({})",
            function.name.as_deref().unwrap_or(""),
            function.parameters.join(", ")
        ),
        Expression::SuperCallReference { .. } => "super".to_string(),
        Expression::SuperCallForwardArgs { .. } => "super(...arguments)".to_string(),
        Expression::SuperPropertyReference { .. } => "super".to_string(),
        Expression::TemplateLiteral { template, .. } => format!(
            "`{}`",
            template
                .string_parts
                .iter()
                .map(|part| part.as_str())
                .collect::<Vec<_>>()
                .join("${}")
        ),
        Expression::ClassLiteral { class, .. } => format!(
            "class {}({})",
            class.name.as_deref().unwrap_or(""),
            class
                .public_members
                .iter()
                .map(|property| match &property.key {
                    PropertyKey::Name(name) => name.clone(),
                    PropertyKey::Computed(key) => shape(key),
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expression::Sequence { expressions, .. } => format!(
            "({})",
            expressions
                .iter()
                .map(shape)
                .collect::<Vec<_>>()
                .join(" , ")
        ),
    }
}

fn shape_of(source: &str) -> String {
    shape(&expression_of(source))
}

#[test]
fn binary_operators_group_left() {
    assert_eq!(shape_of("a - b - c"), "((a - b) - c)");
    assert_eq!(shape_of("a / b / c"), "((a / b) / c)");
    assert_eq!(shape_of("a + b + c + d"), "(((a + b) + c) + d)");
}

#[test]
fn exponentiation_groups_right() {
    // The one right-associative binary operator in JavaScript.
    assert_eq!(shape_of("a ** b ** c"), "(a ** (b ** c))");
    assert_eq!(shape_of("2 ** 3 ** 2"), "(2 ** (3 ** 2))");
}

#[test]
fn assignment_groups_right() {
    assert_eq!(shape_of("a = b = c"), "(a = (b = c))");
    assert_eq!(shape_of("a += b -= c"), "(a += (b -= c))");
}

#[test]
fn precedence_matches_the_language() {
    assert_eq!(shape_of("a + b * c"), "(a + (b * c))");
    assert_eq!(shape_of("a * b + c"), "((a * b) + c)");
    assert_eq!(shape_of("a + b < c"), "((a + b) < c)");
    assert_eq!(shape_of("a < b == c"), "((a < b) == c)");
    assert_eq!(shape_of("a & b | c"), "((a & b) | c)");
    assert_eq!(shape_of("a | b ^ c"), "(a | (b ^ c))");
    assert_eq!(shape_of("a == b & c"), "((a == b) & c)");
    assert_eq!(shape_of("a && b || c"), "((a && b) || c)");
    assert_eq!(shape_of("a << b + c"), "(a << (b + c))");
    assert_eq!(shape_of("a + b ** c"), "(a + (b ** c))");
    // Parentheses override all of it.
    assert_eq!(shape_of("(a + b) * c"), "((a + b) * c)");
    assert_eq!(shape_of("a * (b + c)"), "(a * (b + c))");
}

#[test]
fn unary_binds_tighter_than_binary() {
    assert_eq!(shape_of("-a + b"), "((-a) + b)");
    assert_eq!(shape_of("!a && b"), "((!a) && b)");
    assert_eq!(shape_of("typeof a === 'x'"), "((typeof a) === \"x\")");
    assert_eq!(shape_of("-a * -b"), "((-a) * (-b))");
}

#[test]
fn a_unary_operator_before_exponentiation_is_rejected() {
    // `-2 ** 2` is a SyntaxError: the spec refuses to guess the grouping.
    assert!(parse_error("-2 ** 2").contains("requires parentheses"));
    // ...and both explicit groupings are fine.
    assert_eq!(shape_of("(-2) ** 2"), "((-2) ** 2)");
    assert_eq!(shape_of("-(2 ** 2)"), "(-(2 ** 2))");
}

#[test]
fn nullish_coalescing_cannot_be_mixed_with_logical_or_and() {
    assert_eq!(shape_of("a ?? b"), "(a ?? b)");
    assert_eq!(shape_of("a ?? b ?? c"), "((a ?? b) ?? c)");

    // Mixing is a syntax error, and the interesting part is *how*: nothing
    // checks for it. Both productions start at precedence 6, so the mixed
    // operator is simply never consumed and is left for the statement parser
    // to trip over.
    for source in ["a || b ?? c", "a ?? b || c", "a && b ?? c", "a ?? b && c"] {
        let message = parse_error(source);
        assert!(
            message.contains("??") || message.contains("||") || message.contains("&&"),
            "{source:?} should be rejected at the mixed operator, said: {message}"
        );
    }

    // Parentheses make either grouping legal.
    assert_eq!(shape_of("(a || b) ?? c"), "((a || b) ?? c)");
    assert_eq!(shape_of("a ?? (b || c)"), "(a ?? (b || c))");

    // And `??` still binds looser than everything bitwise, which is what
    // parsing its operands at precedence 6 buys.
    assert_eq!(shape_of("a | b ?? c"), "((a | b) ?? c)");
    assert_eq!(shape_of("a ?? b | c"), "(a ?? (b | c))");
}

#[test]
fn the_conditional_operator_groups_right() {
    assert_eq!(shape_of("a ? b : c ? d : e"), "(a ? b : (c ? d : e))");
    assert_eq!(shape_of("a ? b ? c : d : e"), "(a ? (b ? c : d) : e)");
    // It binds looser than any binary operator.
    assert_eq!(shape_of("a + b ? c : d"), "((a + b) ? c : d)");
}

#[test]
fn member_access_and_calls_chain_in_order() {
    assert_eq!(shape_of("a.b.c"), "a.b.c");
    assert_eq!(shape_of("a[b][c]"), "a[b][c]");
    assert_eq!(shape_of("f()"), "f()");
    assert_eq!(shape_of("f(1, 2)"), "f(1, 2)");
    assert_eq!(shape_of("a.b()"), "a.b()");
    assert_eq!(shape_of("f().g"), "f().g");
    assert_eq!(shape_of("f()()"), "f()()");
    assert_eq!(shape_of("a.b().c[d]()"), "a.b().c[d]()");
    // A call binds tighter than any operator.
    assert_eq!(shape_of("a + f(b)"), "(a + f(b))");
    assert_eq!(shape_of("-f(a)"), "(-f(a))");
}

#[test]
fn reserved_words_are_legal_property_names() {
    assert_eq!(shape_of("o.class"), "o.class");
    assert_eq!(shape_of("o.if"), "o.if");
    assert_eq!(shape_of("o.function"), "o.function");
    assert_eq!(shape_of("o.new"), "o.new");
}

#[test]
fn count_operators_distinguish_prefix_from_postfix() {
    assert_eq!(shape_of("++a"), "(++a)");
    assert_eq!(shape_of("a++"), "(a++)");
    assert_eq!(shape_of("--a.b"), "(--a.b)");
    assert_eq!(shape_of("a[0]++"), "(a[0]++)");
    // Only references can be counted.
    assert!(parse_error("1++").contains("invalid target"));
    assert!(parse_error("++f()").contains("invalid target"));
}

#[test]
fn only_references_can_be_assigned_to() {
    assert_eq!(shape_of("a = 1"), "(a = 1)");
    assert_eq!(shape_of("a.b = 1"), "(a.b = 1)");
    assert_eq!(shape_of("a[0] = 1"), "(a[0] = 1)");
    assert!(parse_error("1 = 2").contains("invalid assignment target"));
    assert!(parse_error("a + b = c").contains("invalid assignment target"));
    assert!(parse_error("f() = 1").contains("invalid assignment target"));
}

#[test]
fn automatic_semicolon_insertion_ends_statements_at_newlines() {
    // Two statements, no semicolons written.
    let program = parse("a = 1\nb = 2");
    assert_eq!(program.body.len(), 2);

    // ...but on one line it is an error, because `a = 1 b` is not an
    // expression.
    assert!(parse_error("a = 1 b = 2").contains("expected `;`"));

    // A closing brace also offers a semicolon.
    let program = parse("function f() { return 1 }");
    assert_eq!(program.body.len(), 1);

    // And so does end of input.
    assert_eq!(parse("a = 1").body.len(), 1);
}

#[test]
fn a_newline_after_return_ends_the_statement() {
    // The classic restricted production: this returns undefined, it does
    // not return `1`.
    let program = parse("function f() {\n  return\n  1\n}");
    let function = &program.hoisted_functions[0];
    assert_eq!(
        function.body.len(),
        2,
        "`return` and `1` are two statements"
    );
    match &function.body[0] {
        Statement::Return { value, .. } => {
            assert!(value.is_none(), "the newline ended the return statement")
        }
        other => panic!("{other:?}"),
    }

    // On the same line it does return the value.
    let program = parse("function f() { return 1 }");
    let function = &program.hoisted_functions[0];
    match &function.body[0] {
        Statement::Return { value, .. } => assert!(value.is_some()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_newline_before_a_postfix_operator_detaches_it() {
    // `a\n++\nb` is `a; ++b`, not `a++; b`.
    let program = parse("a\n++\nb");
    assert_eq!(program.body.len(), 2);
    assert_eq!(shape(&expression_of("a++")), "(a++)");
    match &program.body[1] {
        Statement::Expression { expression, .. } => {
            assert_eq!(shape(expression), "(++b)", "the ++ attached to b")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn in_loses_its_meaning_inside_a_for_header() {
    // `for (var a = b in c; ...)` would be a parse of `b in c` if the
    // grammar parameter were ignored. With it, `in` is not available, so
    // the parser stops and this is an error at `in`.
    assert!(Parser::parse_program("for (var a = b in c; a; a) {}").is_err());
    // Outside a for header `in` is an ordinary relational operator.
    assert_eq!(shape_of("a in b"), "(a in b)");
    // ...and inside the *body* of the for, it works again.
    let program = parse("for (var i = 0; i < 1; i = i + 1) { x = a in b; }");
    assert_eq!(program.body.len(), 1);
}

#[test]
fn var_declarations_hoist_to_the_enclosing_function() {
    let program = parse(
        "var a = 1;
         function f() {
           var b = 2;
           if (true) { var c = 3; }
           while (false) { var d = 4; }
         }",
    );
    assert_eq!(declared_vars(&program.scope), vec!["a", "f"]);

    let function = &program.hoisted_functions[0];
    assert_eq!(
        declared_vars(&function.scope),
        vec!["b", "c", "d"],
        "a var inside a block still belongs to the function"
    );
}

#[test]
fn a_function_declaration_is_not_a_statement() {
    // Calling before the definition is legal, because the binding is created
    // when the scope is entered. So the declaration belongs to the scope and
    // leaves an empty statement where it was written -- V8's
    // `Parser::DeclareFunction` returns `EmptyStatement`.
    let program = parse("f(); function f() { return 1; }");
    assert_eq!(program.hoisted_functions.len(), 1);
    assert_eq!(program.hoisted_functions[0].name.as_deref(), Some("f"));
    assert_eq!(program.body.len(), 2);
    assert!(
        matches!(program.body[1], Statement::Empty { .. }),
        "the declaration should leave an empty statement, not a second copy \
         of the function: {:?}",
        program.body[1]
    );
    // The name is still bound, so it is hoisted as a variable too.
    assert_eq!(declared_vars(&program.scope), vec!["f"]);
}

#[test]
fn a_function_literals_parameters_and_body_are_recorded() {
    let program = parse("var f = function add(a, b) { return a + b; };");
    let declaration = match &program.body[0] {
        Statement::Declaration { declarations, .. } => &declarations[0],
        other => panic!("{other:?}"),
    };
    let function = match declaration.initializer.as_ref().unwrap() {
        Expression::FunctionLiteral { function, .. } => function,
        other => panic!("{other:?}"),
    };
    assert_eq!(function.name.as_deref(), Some("add"));
    assert_eq!(function.parameters, vec!["a", "b"]);
    assert_eq!(function.body.len(), 1);
    // A function expression is not hoisted into the enclosing scope.
    assert_eq!(program.hoisted_functions.len(), 0);
    assert_eq!(declared_vars(&program.scope), vec!["f"]);
}

#[test]
fn object_literals() {
    assert_eq!(shape_of("({})"), "{}");
    assert_eq!(shape_of("({a: 1})"), "{a: 1}");
    assert_eq!(shape_of("({a: 1, b: 2})"), "{a: 1, b: 2}");
    // A trailing comma is allowed.
    assert_eq!(shape_of("({a: 1,})"), "{a: 1}");
    // Shorthand.
    assert_eq!(shape_of("({a})"), "{a: a}");
    // Reserved words and numbers are legal keys, and a numeric key is a
    // string key.
    assert_eq!(shape_of("({if: 1})"), "{if: 1}");
    assert_eq!(shape_of("({1: 2})"), "{1: 2}");
    assert_eq!(shape_of("({1.5: 2})"), "{1.5: 2}");
    assert_eq!(shape_of("({'a b': 1})"), "{a b: 1}");
    // Computed keys.
    assert_eq!(shape_of("({[a]: 1})"), "{[a]: 1}");
}

#[test]
fn array_literals_including_holes() {
    assert_eq!(shape_of("[]"), "[]");
    assert_eq!(shape_of("[1, 2, 3]"), "[1, 2, 3]");
    assert_eq!(shape_of("[1,]"), "[1]", "a trailing comma is not a hole");
    assert_eq!(shape_of("[1, , 3]"), "[1, undefined, 3]");
    assert_eq!(shape_of("[[1], [2]]"), "[[1], [2]]");
}

#[test]
fn statements_parse() {
    assert_eq!(parse(";").body.len(), 1);
    assert_eq!(parse("{ }").body.len(), 1);
    assert_eq!(parse("{ a = 1; b = 2; }").body.len(), 1);
    assert_eq!(parse("if (a) b = 1;").body.len(), 1);
    assert_eq!(parse("if (a) b = 1; else c = 2;").body.len(), 1);
    assert_eq!(parse("while (a) { b = 1; }").body.len(), 1);
    assert_eq!(parse("do { a = 1; } while (b)").body.len(), 1);
    assert_eq!(parse("for (;;) { break; }").body.len(), 1);
    assert_eq!(
        parse("for (var i = 0; i < 10; i = i + 1) { continue; }")
            .body
            .len(),
        1
    );
    assert_eq!(parse("var a, b = 1, c;").body.len(), 1);
    assert_eq!(parse("let a = 1;").body.len(), 1);
    assert_eq!(parse("const a = 1;").body.len(), 1);
}

#[test]
fn dangling_else_binds_to_the_nearest_if() {
    let program = parse("if (a) if (b) x = 1; else y = 2;");
    let outer = match &program.body[0] {
        Statement::If {
            then_statement,
            else_statement,
            ..
        } => {
            assert!(else_statement.is_none(), "the else belongs to the inner if");
            then_statement
        }
        other => panic!("{other:?}"),
    };
    match outer.as_ref() {
        Statement::If { else_statement, .. } => assert!(else_statement.is_some()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn let_is_a_keyword_only_where_a_binding_follows() {
    // `let x = 1` declares.
    match &parse("let x = 1;").body[0] {
        Statement::Declaration { mode, .. } => assert_eq!(*mode, VariableMode::Let),
        other => panic!("{other:?}"),
    }
    // `let = 1` assigns to a variable called `let`.
    assert_eq!(shape_of("let = 1"), "(let = 1)");
    assert_eq!(shape_of("let + 1"), "(let + 1)");
}

#[test]
fn const_must_be_initialized() {
    assert!(parse_error("const a;").contains("must be initialized"));
    assert!(Parser::parse_program("var a;").is_ok());
    assert!(Parser::parse_program("let a;").is_ok());
}

#[test]
fn the_comma_operator_is_a_sequence() {
    assert_eq!(shape_of("a, b, c"), "(a , b , c)");
    // It binds looser than assignment.
    assert_eq!(shape_of("a = 1, b = 2"), "((a = 1) , (b = 2))");
    // ...and does not leak into an argument list.
    assert_eq!(shape_of("f(a, b)"), "f(a, b)");
}

#[test]
fn errors_point_at_the_problem() {
    let error = Parser::parse_program("var a = ;").unwrap_err();
    let rendered = error.display("var a = ;");
    assert!(rendered.starts_with("SyntaxError:"), "{rendered}");
    assert!(rendered.contains("line 1:"), "{rendered}");
    assert!(rendered.contains('^'), "{rendered}");

    let error = Parser::parse_program("a = 1;\nb = ;").unwrap_err();
    let rendered = error.display("a = 1;\nb = ;");
    assert!(rendered.contains("line 2:"), "{rendered}");
}

#[test]
fn lexical_errors_are_reported_and_not_swallowed() {
    // An unterminated string must not parse as whatever follows it.
    assert!(parse_error("var a = 'oops;").contains("unterminated string"));
    assert!(parse_error("var a = 1 /* oops").contains("unterminated comment"));
    assert!(parse_error("var a = `x;").contains("unterminated template"));
}

#[test]
fn unimplemented_syntax_fails_loudly_rather_than_silently() {
    // Each of these would be a wrong parse if it were accepted.
    for (source, expected) in [
        ("class A { [k]() {} }", "computed class members"),
        ("class A { get x; }", "an accessor needs a body"),
        ("for (var x of y) {}", "for...of"),
        ("for (var a, b in y) {}", "single variable"),
        ("with (a) {}", "not implemented"),
        ("function* f() {}", "generators"),
        ("f(...a)", "spread"),
        ("a?.b", "optional chaining"),
        ("tag`a`", "tagged templates"),
        ("function f(a = 1) {}", "default parameters"),
        ("x\n=> x", "no newline before `=>`"),
        ("(a, ...b) => a", "rest parameters"),
        ("function f(...a) {}", "rest parameters"),
        ("({m() {}})", "method shorthand"),
        ("outer: while (a) { break outer; }", "not implemented"),
    ] {
        let message = parse_error(source);
        assert!(
            message.contains(expected),
            "{source:?} should mention {expected:?}, said: {message}"
        );
    }
}

#[test]
fn deep_nesting_fails_with_an_error_not_a_stack_overflow() {
    let source = format!("{}1{}", "(".repeat(5000), ")".repeat(5000));
    let error = Parser::parse_program(&source).unwrap_err();
    assert!(error.message.contains("too much recursion"), "{error:?}");
}

#[test]
fn a_realistic_program_parses() {
    let source = r#"
        function factorial(n) {
          if (n <= 1) {
            return 1;
          }
          return n * factorial(n - 1);
        }

        function sum(values) {
          var total = 0;
          for (var i = 0; i < values.length; i = i + 1) {
            total += values[i];
          }
          return total;
        }

        var results = [];
        var i = 0;
        while (i < 10) {
          results[i] = factorial(i);
          i++;
        }

        var report = {
          count: results.length,
          total: sum(results),
          label: "factorials",
          nested: { a: 1, b: [1, 2, 3] }
        };

        var verdict = report.total > 100 ? "big" : "small";
    "#;
    let program = parse(source);
    assert_eq!(program.hoisted_functions.len(), 2);
    assert_eq!(
        declared_vars(&program.scope),
        vec!["factorial", "sum", "results", "i", "report", "verdict"]
    );
    // The printer runs over the whole tree without panicking, which is a
    // cheap exhaustiveness check on the node kinds.
    let printed = AstPrinter::print_program(&program);
    assert!(printed.contains("FUNC factorial(n)"));
    assert!(printed.contains("OBJECT LITERAL"));
    assert!(printed.contains("CONDITIONAL"));
    assert!(printed.contains("FOR"));
    assert!(printed.contains("WHILE"));
}
