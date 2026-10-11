use crate::parsing::token::Token;

pub type Position = usize;
pub const NO_SOURCE_POSITION: Position = usize::MAX;

#[derive(Clone, PartialEq, Debug)]
pub enum Literal {
    Smi(i32),
    Number(f64),
    String(String),
    Boolean(bool),
    Null,
    Undefined,
}

impl Literal {
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

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum VariableMode {
    Var,
    Let,
    Const,
}

#[derive(Clone, Debug)]
pub struct VariableDeclaration {
    pub name: String,
    pub initializer: Option<Expression>,
    pub position: Position,
}

#[derive(Clone, Debug)]
pub struct ObjectLiteralProperty {
    pub key: PropertyKey,
    pub value: Expression,
    pub kind: ObjectLiteralPropertyKind,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ObjectLiteralPropertyKind {
    Computed,
    Getter,
    Setter,
}

#[derive(Clone, Debug)]
pub enum PropertyKey {
    Name(String),
    Computed(Expression),
}

#[derive(Clone, Debug)]
pub struct CaseClause {
    pub label: Option<Expression>,
    pub statements: Vec<Statement>,
}

#[derive(Clone, Debug)]
pub struct ClassLiteralProperty {
    pub key: PropertyKey,
    pub value: FunctionLiteral,
    pub kind: ClassLiteralPropertyKind,
    pub is_static: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ClassLiteralPropertyKind {
    Method,
    Getter,
    Setter,
}

#[derive(Clone, Debug)]
pub struct ClassLiteral {
    pub name: Option<String>,
    pub extends: Option<Expression>,
    pub constructor: FunctionLiteral,
    pub public_members: Vec<ClassLiteralProperty>,
    pub scope: crate::ast::scopes::Scope,
    pub position: Position,
}

#[derive(Clone, Debug)]
pub struct FunctionLiteral {
    pub name: Option<String>,
    pub parameters: Vec<String>,
    pub body: Vec<Statement>,
    pub scope: crate::ast::scopes::Scope,
    pub hoisted_functions: Vec<FunctionLiteral>,
    pub position: Position,
}

#[derive(Clone, Debug)]
pub enum Expression {
    Literal {
        value: Literal,
        position: Position,
    },

    VariableProxy {
        name: String,
        position: Position,
    },

    This {
        position: Position,
    },

    Assignment {
        op: Token,
        target: Box<Expression>,
        value: Box<Expression>,
        position: Position,
    },

    Binary {
        op: Token,
        left: Box<Expression>,
        right: Box<Expression>,
        position: Position,
    },

    Compare {
        op: Token,
        left: Box<Expression>,
        right: Box<Expression>,
        position: Position,
    },

    Logical {
        op: Token,
        left: Box<Expression>,
        right: Box<Expression>,
        position: Position,
    },

    Unary {
        op: Token,
        expression: Box<Expression>,
        position: Position,
    },

    Count {
        op: Token,
        is_prefix: bool,
        expression: Box<Expression>,
        position: Position,
    },

    Conditional {
        condition: Box<Expression>,
        then_expression: Box<Expression>,
        else_expression: Box<Expression>,
        position: Position,
    },

    Property {
        object: Box<Expression>,
        key: Box<PropertyAccess>,
        position: Position,
    },

    CallNew {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
        position: Position,
    },

    Call {
        callee: Box<Expression>,
        arguments: Vec<Expression>,
        position: Position,
    },

    ObjectLiteral {
        properties: Vec<ObjectLiteralProperty>,
        position: Position,
    },

    ArrayLiteral {
        values: Vec<Expression>,
        position: Position,
    },

    FunctionLiteral {
        function: Box<FunctionLiteral>,
        position: Position,
    },

    ClassLiteral {
        class: Box<ClassLiteral>,
        position: Position,
    },

    SuperCallReference {
        position: Position,
    },

    SuperCallForwardArgs {
        position: Position,
    },

    SuperPropertyReference {
        is_static: bool,
        position: Position,
    },

    Sequence {
        expressions: Vec<Expression>,
        position: Position,
    },
}

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
            | Expression::CallNew { position, .. }
            | Expression::Call { position, .. }
            | Expression::ObjectLiteral { position, .. }
            | Expression::ArrayLiteral { position, .. }
            | Expression::FunctionLiteral { position, .. }
            | Expression::ClassLiteral { position, .. }
            | Expression::SuperCallReference { position }
            | Expression::SuperCallForwardArgs { position }
            | Expression::SuperPropertyReference { position, .. }
            | Expression::Sequence { position, .. } => *position,
        }
    }

    pub fn is_valid_reference_expression(&self) -> bool {
        matches!(
            self,
            Expression::VariableProxy { .. } | Expression::Property { .. }
        )
    }

    pub fn to_boolean_is_true(&self) -> Option<bool> {
        match self {
            Expression::Literal { value, .. } => Some(value.to_boolean_is_true()),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Statement {
    Block {
        statements: Vec<Statement>,
        position: Position,
    },

    ForIn {
        each: String,
        subject: Expression,
        body: Box<Statement>,
        position: Position,
    },

    TryCatch {
        try_block: Vec<Statement>,
        catch_variable: Option<String>,
        catch_block: Vec<Statement>,
        position: Position,
    },

    TryFinally {
        try_block: Vec<Statement>,
        finally_block: Vec<Statement>,
        position: Position,
    },

    Throw {
        exception: Expression,
        position: Position,
    },

    Expression {
        expression: Expression,
        position: Position,
    },

    Empty {
        position: Position,
    },

    Declaration {
        mode: VariableMode,
        declarations: Vec<VariableDeclaration>,
        position: Position,
    },

    If {
        condition: Expression,
        then_statement: Box<Statement>,
        else_statement: Option<Box<Statement>>,
        position: Position,
    },

    While {
        condition: Expression,
        body: Box<Statement>,
        position: Position,
    },

    DoWhile {
        body: Box<Statement>,
        condition: Expression,
        position: Position,
    },

    For {
        init: Option<Box<Statement>>,
        condition: Option<Expression>,
        next: Option<Expression>,
        body: Box<Statement>,
        position: Position,
    },

    Switch {
        tag: Expression,
        cases: Vec<CaseClause>,
        position: Position,
    },

    Return {
        value: Option<Expression>,
        position: Position,
    },

    Break {
        position: Position,
    },

    Continue {
        position: Position,
    },
}

impl Statement {
    pub fn position(&self) -> Position {
        match self {
            Statement::Block { position, .. }
            | Statement::Expression { position, .. }
            | Statement::Empty { position }
            | Statement::Declaration { position, .. }
            | Statement::If { position, .. }
            | Statement::While { position, .. }
            | Statement::DoWhile { position, .. }
            | Statement::For { position, .. }
            | Statement::Switch { position, .. }
            | Statement::Return { position, .. }
            | Statement::ForIn { position, .. }
            | Statement::TryFinally { position, .. }
            | Statement::TryCatch { position, .. }
            | Statement::Throw { position, .. }
            | Statement::Break { position }
            | Statement::Continue { position } => *position,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Program {
    pub body: Vec<Statement>,
    pub scope: crate::ast::scopes::Scope,
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
