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
