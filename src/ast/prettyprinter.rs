//! Ported from `src/ast/prettyprinter.h` / `prettyprinter.cc`.
//!
//! V8's `--print-ast` output: the tree, indented. Cheap to write and worth
//! far more than it costs, because the alternative when the parser and the
//! bytecode generator disagree is guessing which one is wrong.

use crate::ast::ast::*;

/// `AstPrinter`
pub struct AstPrinter;

impl AstPrinter {
    /// `AstPrinter::PrintProgram`
    pub fn print_program(program: &Program) -> String {
        let mut out = String::new();
        out.push_str("PROGRAM\n");
        if !program.hoisted_vars.is_empty() {
            out.push_str(&format!(". VARS {}\n", program.hoisted_vars.join(", ")));
        }
        for function in &program.hoisted_functions {
            print_function(&mut out, function, 1);
        }
        for statement in &program.body {
            print_statement(&mut out, statement, 1);
        }
        out
    }

    /// `AstPrinter::PrintStatement`
    pub fn print_statement(statement: &Statement) -> String {
        let mut out = String::new();
        print_statement(&mut out, statement, 0);
        out
    }

    /// `AstPrinter::PrintExpression`
    pub fn print_expression(expression: &Expression) -> String {
        let mut out = String::new();
        print_expression(&mut out, expression, 0);
        out
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str(". ");
    }
}

fn print_function(out: &mut String, function: &FunctionLiteral, depth: usize) {
    indent(out, depth);
    out.push_str(&format!(
        "FUNC {}({})\n",
        function.name.as_deref().unwrap_or("<anonymous>"),
        function.parameters.join(", ")
    ));
    if !function.hoisted_vars.is_empty() {
        indent(out, depth + 1);
        out.push_str(&format!("VARS {}\n", function.hoisted_vars.join(", ")));
    }
    for inner in &function.hoisted_functions {
        print_function(out, inner, depth + 1);
    }
    for statement in &function.body {
        print_statement(out, statement, depth + 1);
    }
}

fn print_statement(out: &mut String, statement: &Statement, depth: usize) {
    indent(out, depth);
    match statement {
        Statement::Block { statements, .. } => {
            out.push_str("BLOCK\n");
            for statement in statements {
                print_statement(out, statement, depth + 1);
            }
        }
        Statement::Expression { expression, .. } => {
            out.push_str("EXPRESSION STATEMENT\n");
            print_expression(out, expression, depth + 1);
        }
        Statement::Empty { .. } => out.push_str("EMPTY\n"),
        Statement::Declaration {
            mode, declarations, ..
        } => {
            out.push_str(&format!("DECLS {mode:?}\n"));
            for declaration in declarations {
                indent(out, depth + 1);
                out.push_str(&format!("VAR {}\n", declaration.name));
                if let Some(initializer) = &declaration.initializer {
                    print_expression(out, initializer, depth + 2);
                }
            }
        }
        Statement::If {
            condition,
            then_statement,
            else_statement,
            ..
        } => {
            out.push_str("IF\n");
            print_expression(out, condition, depth + 1);
            indent(out, depth + 1);
            out.push_str("THEN\n");
            print_statement(out, then_statement, depth + 2);
            if let Some(else_statement) = else_statement {
                indent(out, depth + 1);
                out.push_str("ELSE\n");
                print_statement(out, else_statement, depth + 2);
            }
        }
        Statement::While {
            condition, body, ..
        } => {
            out.push_str("WHILE\n");
            print_expression(out, condition, depth + 1);
            print_statement(out, body, depth + 1);
        }
        Statement::DoWhile {
            body, condition, ..
        } => {
            out.push_str("DO WHILE\n");
            print_statement(out, body, depth + 1);
            print_expression(out, condition, depth + 1);
        }
        Statement::For {
            init,
            condition,
            next,
            body,
            ..
        } => {
            out.push_str("FOR\n");
            if let Some(init) = init {
                indent(out, depth + 1);
                out.push_str("INIT\n");
                print_statement(out, init, depth + 2);
            }
            if let Some(condition) = condition {
                indent(out, depth + 1);
                out.push_str("COND\n");
                print_expression(out, condition, depth + 2);
            }
            if let Some(next) = next {
                indent(out, depth + 1);
                out.push_str("NEXT\n");
                print_expression(out, next, depth + 2);
            }
            indent(out, depth + 1);
            out.push_str("BODY\n");
            print_statement(out, body, depth + 2);
        }
        Statement::Return { value, .. } => {
            out.push_str("RETURN\n");
            if let Some(value) = value {
                print_expression(out, value, depth + 1);
            }
        }
        Statement::Break { .. } => out.push_str("BREAK\n"),
        Statement::Continue { .. } => out.push_str("CONTINUE\n"),
    }
}

fn print_expression(out: &mut String, expression: &Expression, depth: usize) {
    indent(out, depth);
    match expression {
        Expression::Literal { value, .. } => out.push_str(&format!("LITERAL {value:?}\n")),
        Expression::VariableProxy { name, .. } => out.push_str(&format!("VAR PROXY {name}\n")),
        Expression::This { .. } => out.push_str("THIS\n"),
        Expression::Assignment {
            op, target, value, ..
        } => {
            out.push_str(&format!("ASSIGN {op}\n"));
            print_expression(out, target, depth + 1);
            print_expression(out, value, depth + 1);
        }
        Expression::Binary {
            op, left, right, ..
        }
        | Expression::Compare {
            op, left, right, ..
        }
        | Expression::Logical {
            op, left, right, ..
        } => {
            out.push_str(&format!("{op}\n"));
            print_expression(out, left, depth + 1);
            print_expression(out, right, depth + 1);
        }
        Expression::Unary { op, expression, .. } => {
            out.push_str(&format!("UNARY {op}\n"));
            print_expression(out, expression, depth + 1);
        }
        Expression::Count {
            op,
            is_prefix,
            expression,
            ..
        } => {
            out.push_str(&format!(
                "COUNT {}{op}\n",
                if *is_prefix { "pre " } else { "post " }
            ));
            print_expression(out, expression, depth + 1);
        }
        Expression::Conditional {
            condition,
            then_expression,
            else_expression,
            ..
        } => {
            out.push_str("CONDITIONAL\n");
            print_expression(out, condition, depth + 1);
            print_expression(out, then_expression, depth + 1);
            print_expression(out, else_expression, depth + 1);
        }
        Expression::Property { object, key, .. } => {
            match key.as_ref() {
                PropertyAccess::Named(name) => out.push_str(&format!("PROPERTY .{name}\n")),
                PropertyAccess::Keyed(_) => out.push_str("PROPERTY []\n"),
            }
            print_expression(out, object, depth + 1);
            if let PropertyAccess::Keyed(key) = key.as_ref() {
                print_expression(out, key, depth + 1);
            }
        }
        Expression::Call {
            callee, arguments, ..
        } => {
            out.push_str("CALL\n");
            print_expression(out, callee, depth + 1);
            for argument in arguments {
                print_expression(out, argument, depth + 1);
            }
        }
        Expression::ObjectLiteral { properties, .. } => {
            out.push_str("OBJECT LITERAL\n");
            for property in properties {
                indent(out, depth + 1);
                match &property.key {
                    PropertyKey::Name(name) => out.push_str(&format!("PROPERTY {name}\n")),
                    PropertyKey::Computed(_) => out.push_str("PROPERTY [computed]\n"),
                }
                if let PropertyKey::Computed(key) = &property.key {
                    print_expression(out, key, depth + 2);
                }
                print_expression(out, &property.value, depth + 2);
            }
        }
        Expression::ArrayLiteral { values, .. } => {
            out.push_str("ARRAY LITERAL\n");
            for value in values {
                print_expression(out, value, depth + 1);
            }
        }
        Expression::FunctionLiteral { function, .. } => print_function(out, function, depth),
        Expression::Sequence { expressions, .. } => {
            out.push_str("SEQUENCE\n");
            for expression in expressions {
                print_expression(out, expression, depth + 1);
            }
        }
    }
}
