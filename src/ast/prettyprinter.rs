use crate::ast::ast::*;

pub struct AstPrinter;

impl AstPrinter {
    pub fn print_program(program: &Program) -> String {
        let mut out = String::new();
        out.push_str("PROGRAM\n");
        print_scope(&mut out, &program.scope, 1);
        for function in &program.hoisted_functions {
            print_function(&mut out, function, 1);
        }
        for statement in &program.body {
            print_statement(&mut out, statement, 1);
        }
        out
    }

    pub fn print_statement(statement: &Statement) -> String {
        let mut out = String::new();
        print_statement(&mut out, statement, 0);
        out
    }

    pub fn print_expression(expression: &Expression) -> String {
        let mut out = String::new();
        print_expression(&mut out, expression, 0);
        out
    }
}

fn print_scope(out: &mut String, scope: &crate::ast::scopes::Scope, depth: usize) {
    use crate::common::globals::VariableLocation;
    if scope.variables().is_empty() {
        return;
    }
    indent(out, depth);
    let described: Vec<String> = scope
        .variables()
        .iter()
        .map(|variable| {
            let where_ = match variable.location {
                VariableLocation::Local => format!("r{}", variable.index),
                VariableLocation::Parameter => format!("a{}", variable.index),
                VariableLocation::Context => format!("context[{}]", variable.index),
                VariableLocation::Unallocated => "global".to_string(),
            };
            format!("{}:{where_}", variable.name)
        })
        .collect();
    out.push_str(&format!("VARS {}\n", described.join(", ")));
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
    print_scope(out, &function.scope, depth + 1);
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
        Statement::TryCatch {
            try_block,
            catch_variable,
            catch_block,
            ..
        } => {
            out.push_str("TRY CATCH\n");
            indent(out, depth + 1);
            out.push_str("TRY\n");
            for statement in try_block {
                print_statement(out, statement, depth + 2);
            }
            indent(out, depth + 1);
            match catch_variable {
                Some(name) => out.push_str(&format!("CATCH ({name})\n")),
                None => out.push_str("CATCH\n"),
            }
            for statement in catch_block {
                print_statement(out, statement, depth + 2);
            }
        }
        Statement::ForIn {
            each,
            subject,
            body,
            ..
        } => {
            out.push_str(&format!("FOR IN {each}\n"));
            print_expression(out, subject, depth + 1);
            print_statement(out, body, depth + 1);
        }
        Statement::TryFinally {
            try_block,
            finally_block,
            ..
        } => {
            out.push_str("TRY FINALLY\n");
            indent(out, depth + 1);
            out.push_str("TRY\n");
            for statement in try_block {
                print_statement(out, statement, depth + 2);
            }
            indent(out, depth + 1);
            out.push_str("FINALLY\n");
            for statement in finally_block {
                print_statement(out, statement, depth + 2);
            }
        }
        Statement::Throw { exception, .. } => {
            out.push_str("THROW\n");
            print_expression(out, exception, depth + 1);
        }
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
        Expression::CallNew {
            callee, arguments, ..
        } => {
            out.push_str("CALL NEW\n");
            print_expression(out, callee, depth + 1);
            for argument in arguments {
                print_expression(out, argument, depth + 1);
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
        Expression::SuperCallReference { .. } => out.push_str("SUPER CALL REFERENCE\n"),
        Expression::SuperPropertyReference { is_static, .. } => {
            let kind = if *is_static { "STATIC " } else { "" };
            out.push_str(&format!("{kind}SUPER PROPERTY REFERENCE\n"))
        }
        Expression::SuperCallForwardArgs { .. } => {
            out.push_str("SUPER CALL FORWARD ARGS\n")
        }
        Expression::FunctionLiteral { function, .. } => print_function(out, function, depth),
        Expression::ClassLiteral { class, .. } => {
            out.push_str(&format!(
                "CLASS LITERAL {}\n",
                class.name.as_deref().unwrap_or("(anonymous)")
            ));
            if let Some(extends) = &class.extends {
                indent(out, depth + 1);
                out.push_str("EXTENDS\n");
                print_expression(out, extends, depth + 2);
            }
            indent(out, depth + 1);
            out.push_str("CONSTRUCTOR\n");
            print_function(out, &class.constructor, depth + 2);
            for property in &class.public_members {
                indent(out, depth + 1);
                let kind = match property.kind {
                    ClassLiteralPropertyKind::Method => "METHOD",
                    ClassLiteralPropertyKind::Getter => "GETTER",
                    ClassLiteralPropertyKind::Setter => "SETTER",
                };
                let is_static = if property.is_static { "STATIC " } else { "" };
                out.push_str(&format!("{is_static}{kind}\n"));
                print_function(out, &property.value, depth + 2);
            }
        }
        Expression::Sequence { expressions, .. } => {
            out.push_str("SEQUENCE\n");
            for expression in expressions {
                print_expression(out, expression, depth + 1);
            }
        }
    }
}
