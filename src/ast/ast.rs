//! Ported from `src/ast/ast.h`.
//!
//! V8's AST is a class hierarchy of zone-allocated nodes with a packed bit
//! field in the base class. `vx` uses Rust enums: the node *kinds* and their
//! fields follow V8's `AST_NODE_LIST`, but a sum type replaces the hierarchy
//! and `Box` replaces the zone. That is the one place where following V8's
//! shape would cost more than it buys -- a visitor over a sealed enum is
//! exhaustively checked, which a virtual `Visit` is not.
//!
//! Deviations, tracked in `PORTING.md`:
//!
//! * **Variables are not resolved here.** V8's parser builds a `Scope` tree
//!   and resolves every `VariableProxy` to a `Variable` as it goes. `vx`
//!   leaves a name in the proxy and resolves in the bytecode generator,
//!   which can do it because there are no closures yet. When contexts
//!   arrive, resolution has to move back into the parser.
//! * No classes, generators, async functions, destructuring, template
//!   literals, `try`/`catch`, `switch`, `with`, labels, or regular
//!   expressions.

use crate::parsing::token::Token;

/// Source position, as V8's `kNoSourcePosition`-bearing `int position`.
pub type Position = usize;
pub const NO_SOURCE_POSITION: Position = usize::MAX;

/// `Literal::Type`. Which kind of constant this is, so the bytecode
/// generator can pick between `LdaSmi`, `LdaConstant` and the oddball loads
/// without inspecting a value.
#[derive(Clone, PartialEq, Debug)]
pub enum Literal {
    /// An integer that fits in a Smi: compiles to `LdaSmi`, no allocation.
    Smi(i32),
    /// Any other number: lives in the constant pool as a `HeapNumber`.
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
    Undefined,
}

impl Literal {
    /// `Literal::ToBooleanIsTrue`: whether the constant is truthy, known at
    /// compile time. Lets the generator fold `if (0)` away.
    pub fn to_boolean_is_true(&self) -> bool {
        match self {
            Literal::Smi(value) => *value != 0,
            Literal::Number(value) => *value != 0.0 && !value.is_nan(),
            Literal::String(value) => !value.is_empty(),
            Literal::Boolean(value) => *value,
            Literal::Null | Literal::Undefined => false,
        }
    }
}

/// `VariableMode`: how a binding behaves.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum VariableMode {
    /// `var`: function scoped, hoisted, redeclarable.
    Var,
    /// `let`: block scoped.
    Let,
    /// `const`: block scoped, assign once.
    Const,
}

/// One binding in a declaration statement.
#[derive(Clone, Debug)]
pub struct VariableDeclaration {
    pub name: String,
    pub initializer: Option<Expression>,
    pub position: Position,
}

/// `ObjectLiteralProperty`
#[derive(Clone, Debug)]
pub struct ObjectLiteralProperty {
    /// Already a string: `{1: x}` and `{"1": x}` are the same property.
    pub key: PropertyKey,
    pub value: Expression,
}

#[derive(Clone, Debug)]
pub enum PropertyKey {
    /// A name known at parse time, which can be internalized once into the
    /// constant pool.
    Name(String),
    /// A computed key: `{[expr]: value}`.
    Computed(Expression),
}

/// `FunctionLiteral`
#[derive(Clone, Debug)]
pub struct FunctionLiteral {
    /// `None` for an anonymous function expression.
    pub name: Option<String>,
    pub parameters: Vec<String>,
    pub body: Vec<Statement>,
    /// Every `var` declared anywhere in the function, hoisted to the top as
    /// JavaScript requires. V8 collects these into the function scope; `vx`
    /// hands the list to the bytecode generator, which allocates a register
    /// for each.
    pub hoisted_vars: Vec<String>,
    /// Function declarations in the body, which are hoisted with their
    /// definitions.
    pub hoisted_functions: Vec<FunctionLiteral>,
    pub position: Position,
}

/// `Expression`
#[derive(Clone, Debug)]
pub enum Expression {
    Literal {
        value: Literal,
        position: Position,
    },

    /// `VariableProxy`: a reference to a name, not yet resolved to a slot.
    VariableProxy {
        name: String,
        position: Position,
    },

    /// `ThisExpression`
    This {
        position: Position,
    },

    /// `Assignment`. `op` is [`Token::Assign`] for a plain assignment, or the
    /// compound token (`+=`, `<<=`, ...) otherwise -- V8 keeps the operator
    /// on the node rather than desugaring, because the compound form has to
    /// evaluate its target exactly once.
    Assignment {
        op: Token,
        target: Box<Expression>,
        value: Box<Expression>,
        position: Position,
    },

    /// `BinaryOperation`
    Binary {
        op: Token,
        left: Box<Expression>,
        right: Box<Expression>,
        position: Position,
    },

    /// `CompareOperation`. Separate from `Binary` as in V8: comparisons
    /// produce a boolean and take different feedback.
    Compare {
        op: Token,
        left: Box<Expression>,
        right: Box<Expression>,
        position: Position,
    },

    /// `BinaryOperation` with `&&`, `||` or `??`. Separate because these
    /// short-circuit, so they compile to a branch and not to a call.
    Logical {
        op: Token,
        left: Box<Expression>,
        right: Box<Expression>,
        position: Position,
    },

    /// `UnaryOperation`
    Unary {
        op: Token,
        expression: Box<Expression>,
        position: Position,
    },

    /// `CountOperation`: `++x`, `x--`.
    Count {
        op: Token,
        is_prefix: bool,
        expression: Box<Expression>,
        position: Position,
    },

    /// `Conditional`: `test ? then : otherwise`.
    Conditional {
        condition: Box<Expression>,
        then_expression: Box<Expression>,
        else_expression: Box<Expression>,
        position: Position,
    },

    /// `Property`: `obj.name` or `obj[key]`.
    Property {
        object: Box<Expression>,
        key: Box<PropertyAccess>,
        position: Position,
    },

    /// `Call`
    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
        position: Position,
    },

    /// `ObjectLiteral`
    ObjectLiteral {
        properties: Vec<ObjectLiteralProperty>,
        position: Position,
    },

    /// `ArrayLiteral`
    ArrayLiteral {
        values: Vec<Expression>,
        position: Position,
    },

    /// `FunctionLiteral` used as an expression.
    FunctionLiteral {
        function: Box<FunctionLiteral>,
        position: Position,
    },

    /// The comma operator: evaluate both, yield the right.
    Sequence {
        expressions: Vec<Expression>,
        position: Position,
    },
}

/// How a property is named at the access site. Kept apart from
/// [`PropertyKey`] because a *named* access compiles to
/// `GetNamedProperty` with an inline cache, while a keyed one compiles to
/// `GetKeyedProperty` -- different bytecodes and different feedback.
#[derive(Clone, Debug)]
pub enum PropertyAccess {
    Named(String),
    Keyed(Expression),
}

impl Expression {
    pub fn position(&self) -> Position {
        match self {
            Expression::Literal { position, .. }
            | Expression::VariableProxy { position, .. }
            | Expression::This { position }
            | Expression::Assignment { position, .. }
            | Expression::Binary { position, .. }
            | Expression::Compare { position, .. }
            | Expression::Logical { position, .. }
            | Expression::Unary { position, .. }
            | Expression::Count { position, .. }
            | Expression::Conditional { position, .. }
            | Expression::Property { position, .. }
            | Expression::Call { position, .. }
            | Expression::ObjectLiteral { position, .. }
            | Expression::ArrayLiteral { position, .. }
            | Expression::FunctionLiteral { position, .. }
            | Expression::Sequence { position, .. } => *position,
        }
    }

    /// `Expression::IsValidReferenceExpression`: whether this can be
    /// assigned to. The parser checks it when it turns an expression into an
    /// assignment target, which is how `1 = 2` is rejected.
    pub fn is_valid_reference_expression(&self) -> bool {
        matches!(
            self,
            Expression::VariableProxy { .. } | Expression::Property { .. }
        )
    }

    /// `Expression::ToBooleanIsTrue`, for the constant-folding the bytecode
    /// generator does on `if` and loop conditions.
    pub fn to_boolean_is_true(&self) -> Option<bool> {
        match self {
            Expression::Literal { value, .. } => Some(value.to_boolean_is_true()),
            _ => None,
        }
    }
}

/// `Statement`
#[derive(Clone, Debug)]
pub enum Statement {
    /// `Block`
    Block {
        statements: Vec<Statement>,
        position: Position,
    },

    /// `ExpressionStatement`
    Expression {
        expression: Expression,
        position: Position,
    },

    /// `EmptyStatement`: a bare `;`.
    Empty { position: Position },

    /// A `var` / `let` / `const` statement, which may declare several names.
    Declaration {
        mode: VariableMode,
        declarations: Vec<VariableDeclaration>,
        position: Position,
    },

    /// `FunctionDeclaration`
    FunctionDeclaration {
        function: Box<FunctionLiteral>,
        position: Position,
    },

    /// `IfStatement`
    If {
        condition: Expression,
        then_statement: Box<Statement>,
        else_statement: Option<Box<Statement>>,
        position: Position,
    },

    /// `WhileStatement`
    While {
        condition: Expression,
        body: Box<Statement>,
        position: Position,
    },

    /// `DoWhileStatement`
    DoWhile {
        body: Box<Statement>,
        condition: Expression,
        position: Position,
    },

    /// `ForStatement`
    For {
        init: Option<Box<Statement>>,
        condition: Option<Expression>,
        next: Option<Expression>,
        body: Box<Statement>,
        position: Position,
    },

    /// `ReturnStatement`
    Return {
        value: Option<Expression>,
        position: Position,
    },

    /// `BreakStatement`
    Break { position: Position },

    /// `ContinueStatement`
    Continue { position: Position },
}

impl Statement {
    pub fn position(&self) -> Position {
        match self {
            Statement::Block { position, .. }
            | Statement::Expression { position, .. }
            | Statement::Empty { position }
            | Statement::Declaration { position, .. }
            | Statement::FunctionDeclaration { position, .. }
            | Statement::If { position, .. }
            | Statement::While { position, .. }
            | Statement::DoWhile { position, .. }
            | Statement::For { position, .. }
            | Statement::Return { position, .. }
            | Statement::Break { position }
            | Statement::Continue { position } => *position,
        }
    }
}

/// The result of parsing a whole script: V8's top-level `FunctionLiteral`,
/// whose body is the program.
#[derive(Clone, Debug)]
pub struct Program {
    pub body: Vec<Statement>,
    pub hoisted_vars: Vec<String>,
    pub hoisted_functions: Vec<FunctionLiteral>,
}

// ---------------------------------------------------------------------------
// Printing, which stands in for V8's `--print-ast` (`src/ast/prettyprinter.cc`)
// ---------------------------------------------------------------------------

impl Program {
    /// An indented tree, in the spirit of `AstPrinter`.
    pub fn print(&self) -> String {
        let mut out = String::new();
        out.push_str("PROGRAM\n");
        if !self.hoisted_vars.is_empty() {
            out.push_str(&format!(". VARS {}\n", self.hoisted_vars.join(", ")));
        }
        for function in &self.hoisted_functions {
            print_function(&mut out, function, 1);
        }
        for statement in &self.body {
            print_statement(&mut out, statement, 1);
        }
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
        Statement::FunctionDeclaration { function, .. } => print_function(out, function, depth),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_truthiness_matches_javascript() {
        assert!(!Literal::Smi(0).to_boolean_is_true());
        assert!(Literal::Smi(1).to_boolean_is_true());
        assert!(Literal::Smi(-1).to_boolean_is_true());
        assert!(!Literal::Number(0.0).to_boolean_is_true());
        assert!(
            !Literal::Number(f64::NAN).to_boolean_is_true(),
            "NaN is falsy"
        );
        assert!(Literal::Number(0.5).to_boolean_is_true());
        assert!(!Literal::String(String::new()).to_boolean_is_true());
        assert!(
            Literal::String("0".to_string()).to_boolean_is_true(),
            "\"0\" is truthy"
        );
        assert!(!Literal::Null.to_boolean_is_true());
        assert!(!Literal::Undefined.to_boolean_is_true());
        assert!(Literal::Boolean(true).to_boolean_is_true());
        assert!(!Literal::Boolean(false).to_boolean_is_true());
    }

    #[test]
    fn only_references_can_be_assignment_targets() {
        let variable = Expression::VariableProxy {
            name: "x".to_string(),
            position: 0,
        };
        let literal = Expression::Literal {
            value: Literal::Smi(1),
            position: 0,
        };
        let property = Expression::Property {
            object: Box::new(variable.clone()),
            key: Box::new(PropertyAccess::Named("y".to_string())),
            position: 0,
        };

        assert!(variable.is_valid_reference_expression());
        assert!(property.is_valid_reference_expression(), "o.y = 1 is legal");
        assert!(!literal.is_valid_reference_expression(), "1 = 2 is not");
    }
}
