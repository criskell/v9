use crate::ast::ast::*;
use crate::ast::scopes::{ScopeChain, ScopeKind};
use crate::parsing::scanner::{Location, Scanner};
use crate::objects::function_kind::FunctionKind;
use crate::parsing::token::Token;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParserError {
    pub message: String,
    pub location: Location,
}

impl ParserError {
    pub fn display(&self, source: &str) -> String {
        let chars: Vec<char> = source.chars().collect();
        let line_start = chars[..self.location.beg_pos.min(chars.len())]
            .iter()
            .rposition(|c| *c == '\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        let line_number = chars[..line_start].iter().filter(|c| **c == '\n').count() + 1;
        let line_end = chars[line_start..]
            .iter()
            .position(|c| *c == '\n')
            .map(|offset| line_start + offset)
            .unwrap_or(chars.len());
        let line: String = chars[line_start..line_end].iter().collect();
        let column = self.location.beg_pos.saturating_sub(line_start);
        format!(
            "SyntaxError: {}\n  at line {}:{}\n  {}\n  {}^",
            self.message,
            line_number,
            column + 1,
            line,
            " ".repeat(column)
        )
    }
}

type Result<T> = std::result::Result<T, ParserError>;

struct ClassInfo {
    instance_members_scope: Option<crate::ast::scopes::Scope>,
    static_members_scope: Option<crate::ast::scopes::Scope>,
    computed_field_count: usize,
}

enum ClassMember {
    Constructor(FunctionLiteral),
    Property(ClassLiteralProperty),
    Field(ClassLiteralField),
    ComputedField(ClassLiteralProperty, ClassLiteralField),
}

#[derive(Default)]
struct FunctionState {
    hoisted_functions: Vec<FunctionLiteral>,
    in_static_method: bool,
}

pub struct Parser {
    scanner: Scanner,
    scope_chain: ScopeChain,
    accept_in: bool,
    function_state: Vec<FunctionState>,
    recursion_depth: usize,
}

const MAX_RECURSION_DEPTH: usize = 256;

impl Parser {
    pub fn new(source: &str) -> Parser {
        let mut scope_chain = ScopeChain::new();
        scope_chain.push(ScopeKind::Script);
        Parser {
            scanner: Scanner::new(source),
            scope_chain,
            accept_in: true,
            function_state: vec![FunctionState::default()],
            recursion_depth: 0,
        }
    }

    pub fn parse_program(source: &str) -> Result<Program> {
        let mut parser = Parser::new(source);
        let body = parser.parse_statement_list(Token::Eos)?;
        parser.expect(Token::Eos)?;
        parser.check_scanner_error()?;
        let state = parser.function_state.pop().expect("the top-level state");
        let scope = parser.scope_chain.pop_root();
        Ok(Program {
            body,
            scope,
            hoisted_functions: state.hoisted_functions,
        })
    }

    #[inline]
    fn peek(&self) -> Token {
        self.scanner.peek()
    }

    #[inline]
    fn peek_ahead(&mut self) -> Token {
        self.scanner.peek_ahead()
    }

    #[inline]
    fn peek_position(&self) -> Position {
        self.scanner.peek_location().beg_pos
    }

    #[inline]
    fn position(&self) -> Position {
        self.scanner.location().beg_pos
    }

    #[inline]
    fn next(&mut self) -> Token {
        self.scanner.next()
    }

    #[inline]
    fn check(&mut self, token: Token) -> bool {
        if self.peek() == token {
            self.next();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, token: Token) -> Result<()> {
        if self.peek() != token {
            return Err(self.unexpected_token_with_hint(&format!("expected `{token}`")));
        }
        self.next();
        Ok(())
    }

    #[inline]
    fn consume(&mut self, token: Token) {
        debug_assert_eq!(self.peek(), token);
        self.next();
    }

    fn unexpected_token(&self) -> ParserError {
        let token = self.peek();
        let location = self.scanner.peek_location();
        let found = match token {
            Token::Eos => "end of input".to_string(),
            Token::Identifier => format!("identifier `{}`", self.scanner.next_literal()),
            other => format!("`{other}`"),
        };
        ParserError {
            message: format!("unexpected {found}"),
            location,
        }
    }

    fn unexpected_token_with_hint(&self, hint: &str) -> ParserError {
        let mut error = self.unexpected_token();
        error.message = format!("{}, {hint}", error.message);
        error
    }

    fn check_scanner_error(&self) -> Result<()> {
        match self.scanner.error() {
            Some(error) => Err(ParserError {
                message: error.message.clone(),
                location: error.location,
            }),
            None => Ok(()),
        }
    }

    fn expect_semicolon(&mut self) -> Result<()> {
        let token = self.peek();
        if token == Token::Semicolon {
            self.next();
            return Ok(());
        }
        if self.scanner.has_line_terminator_before_next() || token.is_auto_semicolon() {
            return Ok(());
        }
        self.check_scanner_error()?;
        Err(self.unexpected_token_with_hint("expected `;`"))
    }

    fn enter_recursion(&mut self) -> Result<()> {
        self.recursion_depth += 1;
        if self.recursion_depth > MAX_RECURSION_DEPTH {
            return Err(ParserError {
                message: "too much recursion in the parser".to_string(),
                location: self.scanner.peek_location(),
            });
        }
        Ok(())
    }

    #[inline]
    fn leave_recursion(&mut self) {
        self.recursion_depth -= 1;
    }

    fn state(&mut self) -> &mut FunctionState {
        self.function_state.last_mut().expect("a function state")
    }

    fn declare_variable(&mut self, name: &str, mode: VariableMode) {
        self.scope_chain.current().declare(name, mode);
    }

    fn record_reference(&mut self, name: &str) {
        self.scope_chain.current().record_reference(name);
    }


    fn parse_statement_list(&mut self, end: Token) -> Result<Vec<Statement>> {
        let mut statements = Vec::new();
        while self.peek() != end {
            if self.peek() == Token::Eos {
                self.check_scanner_error()?;
                return Err(self.unexpected_token_with_hint(&format!("expected `{end}`")));
            }
            statements.push(self.parse_statement_list_item()?);
        }
        Ok(statements)
    }

    fn parse_statement_list_item(&mut self) -> Result<Statement> {
        match self.peek() {
            Token::Function => self.parse_hoistable_declaration(),
            Token::Const => self.parse_variable_statement(VariableMode::Const),
            Token::Let if self.peek_ahead_starts_a_binding() => {
                self.parse_variable_statement(VariableMode::Let)
            }
            _ => self.parse_statement(),
        }
    }

    fn peek_ahead_starts_a_binding(&mut self) -> bool {
        let ahead = self.peek_ahead();
        ahead.is_any_identifier() || ahead == Token::LeftBracket || ahead == Token::LeftBrace
    }

    fn parse_statement(&mut self) -> Result<Statement> {
        self.enter_recursion()?;
        let result = self.parse_statement_inner();
        self.leave_recursion();
        result
    }

    fn parse_statement_inner(&mut self) -> Result<Statement> {
        match self.peek() {
            Token::LeftBrace => self.parse_block(),
            Token::Semicolon => {
                let position = self.peek_position();
                self.next();
                Ok(Statement::Empty { position })
            }
            Token::Var => self.parse_variable_statement(VariableMode::Var),
            Token::If => self.parse_if_statement(),
            Token::While => self.parse_while_statement(),
            Token::Do => self.parse_do_while_statement(),
            Token::For => self.parse_for_statement(),
            Token::Return => self.parse_return_statement(),
            Token::Break => {
                let position = self.peek_position();
                self.consume(Token::Break);
                if self.peek().is_any_identifier()
                    && !self.scanner.has_line_terminator_before_next()
                {
                    return Err(self.unexpected_token_with_hint("labels are not implemented"));
                }
                self.expect_semicolon()?;
                Ok(Statement::Break { position })
            }
            Token::Continue => {
                let position = self.peek_position();
                self.consume(Token::Continue);
                if self.peek().is_any_identifier()
                    && !self.scanner.has_line_terminator_before_next()
                {
                    return Err(self.unexpected_token_with_hint("labels are not implemented"));
                }
                self.expect_semicolon()?;
                Ok(Statement::Continue { position })
            }
            Token::Function => self.parse_hoistable_declaration(),
            Token::Try => self.parse_try_statement(),
            Token::Throw => self.parse_throw_statement(),
            Token::Switch => self.parse_switch_statement(),
            Token::With | Token::Debugger => {
                Err(self.unexpected_token_with_hint("this statement is not implemented"))
            }
            Token::Class => self.parse_class_declaration(),
            _ => self.parse_expression_statement(),
        }
    }

    fn parse_throw_statement(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        self.consume(Token::Throw);
        if self.scanner.has_line_terminator_before_next() {
            return Err(self.unexpected_token_with_hint("no line break is allowed here"));
        }
        let exception = self.parse_expression()?;
        self.expect_semicolon()?;
        Ok(Statement::Throw {
            exception,
            position,
        })
    }

    fn parse_try_statement(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        self.consume(Token::Try);
        let try_block = self.parse_block_body()?;

        if self.peek() == Token::Finally {
            self.consume(Token::Finally);
            let finally_block = self.parse_block_body()?;
            return Ok(Statement::TryFinally {
                try_block,
                finally_block,
                position,
            });
        }
        self.consume(Token::Catch);

        let mut catch_variable = None;
        if self.peek() == Token::LeftParen {
            self.consume(Token::LeftParen);
            let name = self.parse_identifier()?;
            self.scope_chain.current().declare(&name, VariableMode::Let);
            catch_variable = Some(name);
            self.expect(Token::RightParen)?;
        }
        let catch_block = self.parse_block_body()?;

        let inner = Statement::TryCatch {
            try_block,
            catch_variable,
            catch_block,
            position,
        };

        if self.peek() == Token::Finally {
            self.consume(Token::Finally);
            let finally_block = self.parse_block_body()?;
            return Ok(Statement::TryFinally {
                try_block: vec![inner],
                finally_block,
                position,
            });
        }

        Ok(inner)
    }

    fn parse_block_body(&mut self) -> Result<Vec<Statement>> {
        self.expect(Token::LeftBrace)?;
        let statements = self.parse_statement_list(Token::RightBrace)?;
        self.expect(Token::RightBrace)?;
        Ok(statements)
    }

    fn parse_block(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        let statements = self.parse_block_body()?;
        Ok(Statement::Block {
            statements,
            position,
        })
    }

    fn parse_variable_statement(&mut self, mode: VariableMode) -> Result<Statement> {
        let position = self.peek_position();
        self.next(); // var / let / const
        let declarations = self.parse_variable_declarations(mode)?;
        self.expect_semicolon()?;
        Ok(Statement::Declaration {
            mode,
            declarations,
            position,
        })
    }

    fn parse_variable_declarations(
        &mut self,
        mode: VariableMode,
    ) -> Result<Vec<VariableDeclaration>> {
        let mut declarations = Vec::new();
        loop {
            let position = self.peek_position();
            let name = self.parse_identifier()?;
            self.declare_variable(&name, mode);

            let initializer = if self.check(Token::Assign) {
                Some(self.parse_assignment_expression()?)
            } else {
                if mode == VariableMode::Const {
                    return Err(ParserError {
                        message: format!("`const {name}` must be initialized"),
                        location: self.scanner.location(),
                    });
                }
                None
            };

            declarations.push(VariableDeclaration {
                name,
                initializer,
                position,
            });

            if !self.check(Token::Comma) {
                break;
            }
        }
        Ok(declarations)
    }

    fn parse_hoistable_declaration(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        self.consume(Token::Function);
        if self.peek() == Token::Mul {
            return Err(self.unexpected_token_with_hint("generators are not implemented"));
        }
        let name = self.parse_identifier()?;
        let function = self.parse_function_literal(Some(name.clone()), position)?;
        self.state().hoisted_functions.push(function);
        self.declare_variable(&name, VariableMode::Var);
        Ok(Statement::Empty { position })
    }

    fn parse_switch_statement(&mut self) -> Result<Statement> {
        // SwitchStatement ::
        //   'switch' '(' Expression ')' '{' CaseClause* '}'
        // CaseClause ::
        //   'case' Expression ':' StatementList
        //   'default' ':' StatementList
        let position = self.peek_position();

        self.consume(Token::Switch);
        self.expect(Token::LeftParen)?;
        let tag = self.parse_expression()?;
        self.expect(Token::RightParen)?;

        let mut default_seen = false;
        let mut cases = Vec::new();
        self.expect(Token::LeftBrace)?;
        while self.peek() != Token::RightBrace {
            let label = if self.check(Token::Case) {
                Some(self.parse_expression()?)
            } else {
                self.expect(Token::Default)?;
                if default_seen {
                    return Err(ParserError {
                        message: "More than one default clause in switch statement".to_string(),
                        location: self.scanner.location(),
                    });
                }
                default_seen = true;
                None
            };
            self.expect(Token::Colon)?;
            let mut statements = Vec::new();
            while !matches!(
                self.peek(),
                Token::Case | Token::Default | Token::RightBrace
            ) {
                statements.push(self.parse_statement_list_item()?);
            }
            cases.push(CaseClause { label, statements });
        }
        self.expect(Token::RightBrace)?;

        Ok(Statement::Switch {
            tag,
            cases,
            position,
        })
    }

    fn parse_if_statement(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        self.consume(Token::If);
        self.expect(Token::LeftParen)?;
        let condition = self.parse_expression()?;
        self.expect(Token::RightParen)?;
        let then_statement = Box::new(self.parse_statement()?);
        let else_statement = if self.check(Token::Else) {
            Some(Box::new(self.parse_statement()?))
        } else {
            None
        };
        Ok(Statement::If {
            condition,
            then_statement,
            else_statement,
            position,
        })
    }

    fn parse_while_statement(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        self.consume(Token::While);
        self.expect(Token::LeftParen)?;
        let condition = self.parse_expression()?;
        self.expect(Token::RightParen)?;
        let body = Box::new(self.parse_statement()?);
        Ok(Statement::While {
            condition,
            body,
            position,
        })
    }

    fn parse_do_while_statement(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        self.consume(Token::Do);
        let body = Box::new(self.parse_statement()?);
        self.expect(Token::While)?;
        self.expect(Token::LeftParen)?;
        let condition = self.parse_expression()?;
        self.expect(Token::RightParen)?;
        self.check(Token::Semicolon);
        Ok(Statement::DoWhile {
            body,
            condition,
            position,
        })
    }

    fn for_in_each(&mut self, init: &Statement) -> Option<String> {
        match init {
            Statement::Declaration { declarations, .. } if declarations.len() == 1 => {
                let declaration = &declarations[0];
                if declaration.initializer.is_some() {
                    return None;
                }
                Some(declaration.name.clone())
            }
            Statement::Expression {
                expression: Expression::VariableProxy { name, .. },
                ..
            } => Some(name.clone()),
            _ => None,
        }
    }

    fn parse_for_statement(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        self.consume(Token::For);
        self.expect(Token::LeftParen)?;

        let init = if self.peek() == Token::Semicolon {
            None
        } else {
            let saved = self.accept_in;
            self.accept_in = false;
            let init = self.parse_for_init();
            self.accept_in = saved;
            Some(Box::new(init?))
        };

        if self.peek() == Token::Of {
            return Err(self.unexpected_token_with_hint("for...of is not implemented"));
        }
        if self.peek() == Token::In {
            let Some(init) = init else {
                return Err(self.unexpected_token_with_hint("for...in needs a binding"));
            };
            let each = match self.for_in_each(&init) {
                Some(name) => name,
                None => {
                    return Err(self.unexpected_token_with_hint("for...in takes a single variable"))
                }
            };
            self.consume(Token::In);
            let subject = self.parse_expression()?;
            self.expect(Token::RightParen)?;
            let body = Box::new(self.parse_statement()?);
            return Ok(Statement::ForIn {
                each,
                subject,
                body,
                position,
            });
        }

        self.expect(Token::Semicolon)?;

        let condition = if self.peek() == Token::Semicolon {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect(Token::Semicolon)?;

        let next = if self.peek() == Token::RightParen {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect(Token::RightParen)?;

        let body = Box::new(self.parse_statement()?);
        Ok(Statement::For {
            init,
            condition,
            next,
            body,
            position,
        })
    }

    fn parse_for_init(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        let mode = match self.peek() {
            Token::Var => Some(VariableMode::Var),
            Token::Const => Some(VariableMode::Const),
            Token::Let if self.peek_ahead_starts_a_binding() => Some(VariableMode::Let),
            _ => None,
        };
        match mode {
            Some(mode) => {
                self.next();
                let declarations = self.parse_variable_declarations(mode)?;
                Ok(Statement::Declaration {
                    mode,
                    declarations,
                    position,
                })
            }
            None => {
                let expression = self.parse_expression()?;
                Ok(Statement::Expression {
                    expression,
                    position,
                })
            }
        }
    }

    fn parse_return_statement(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        self.consume(Token::Return);
        let value = if self.peek() == Token::Semicolon
            || self.peek().is_auto_semicolon()
            || self.scanner.has_line_terminator_before_next()
        {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect_semicolon()?;
        Ok(Statement::Return { value, position })
    }

    fn parse_expression_statement(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        if self.peek().is_any_identifier() && self.peek_ahead() == Token::Colon {
            return Err(self.unexpected_token_with_hint("labels are not implemented"));
        }
        let expression = self.parse_expression()?;
        self.expect_semicolon()?;
        Ok(Statement::Expression {
            expression,
            position,
        })
    }


    fn parse_expression(&mut self) -> Result<Expression> {
        let position = self.peek_position();
        let first = self.parse_assignment_expression()?;
        if self.peek() != Token::Comma {
            return Ok(first);
        }
        let mut expressions = vec![first];
        while self.check(Token::Comma) {
            expressions.push(self.parse_assignment_expression()?);
        }
        Ok(Expression::Sequence {
            expressions,
            position,
        })
    }

    fn parse_assignment_expression(&mut self) -> Result<Expression> {
        self.enter_recursion()?;
        let result = self.parse_assignment_expression_inner();
        self.leave_recursion();
        result
    }

    fn parse_assignment_expression_inner(&mut self) -> Result<Expression> {
        let position = self.peek_position();
        let expression = self.parse_conditional_expression()?;

        let op = self.peek();
        if !op.is_assignment_op() {
            return Ok(expression);
        }
        if !expression.is_valid_reference_expression() {
            return Err(ParserError {
                message: "invalid assignment target".to_string(),
                location: Location::new(position, self.peek_position()),
            });
        }
        self.next();
        let value = self.parse_assignment_expression()?;
        Ok(Expression::Assignment {
            op,
            target: Box::new(expression),
            value: Box::new(value),
            position,
        })
    }

    fn parse_conditional_expression(&mut self) -> Result<Expression> {
        let position = self.peek_position();
        let expression = self.parse_logical_expression()?;
        if self.peek() != Token::Conditional {
            return Ok(expression);
        }
        self.consume(Token::Conditional);
        let saved = self.accept_in;
        self.accept_in = true;
        let then_expression = self.parse_assignment_expression();
        self.accept_in = saved;
        let then_expression = then_expression?;
        self.expect(Token::Colon)?;
        let else_expression = self.parse_assignment_expression()?;
        Ok(Expression::Conditional {
            condition: Box::new(expression),
            then_expression: Box::new(then_expression),
            else_expression: Box::new(else_expression),
            position,
        })
    }

    fn parse_logical_expression(&mut self) -> Result<Expression> {
        let expression = self.parse_binary_expression(6)?;
        if matches!(self.peek(), Token::And | Token::Or) {
            return self.parse_binary_continuation(expression, 4);
        }
        if self.peek() == Token::Nullish {
            return self.parse_coalesce_expression(expression);
        }
        Ok(expression)
    }

    fn parse_coalesce_expression(&mut self, first: Expression) -> Result<Expression> {
        //
        let mut expression = first;
        while self.peek() == Token::Nullish {
            let position = self.peek_position();
            self.consume(Token::Nullish);
            let right = self.parse_binary_expression(6)?;
            expression = Expression::Logical {
                op: Token::Nullish,
                left: Box::new(expression),
                right: Box::new(right),
                position,
            };
        }
        Ok(expression)
    }

    fn parse_binary_expression(&mut self, precedence: i32) -> Result<Expression> {
        self.enter_recursion()?;
        let result = self.parse_binary_expression_inner(precedence);
        self.leave_recursion();
        result
    }

    fn parse_binary_expression_inner(&mut self, precedence: i32) -> Result<Expression> {
        let left = self.parse_unary_or_prefix_expression()?;
        self.parse_binary_continuation(left, precedence)
    }

    fn parse_binary_continuation(
        &mut self,
        mut left: Expression,
        precedence: i32,
    ) -> Result<Expression> {
        loop {
            let op = self.peek();
            let op_precedence = op.precedence(self.accept_in);
            if op_precedence < precedence || op_precedence == 0 {
                break;
            }
            let position = self.peek_position();
            self.next();

            let right_precedence = if op == Token::Exp {
                op_precedence
            } else {
                op_precedence + 1
            };
            let right = self.parse_binary_expression(right_precedence)?;

            left = if op.is_compare_op() {
                Expression::Compare {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                    position,
                }
            } else if matches!(op, Token::Or | Token::And) {
                Expression::Logical {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                    position,
                }
            } else {
                Expression::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                    position,
                }
            };
        }
        Ok(left)
    }

    fn parse_unary_or_prefix_expression(&mut self) -> Result<Expression> {
        let op = self.peek();
        if op.is_unary_op() {
            let position = self.peek_position();
            self.next();
            let expression = self.parse_unary_or_prefix_expression()?;
            if self.peek() == Token::Exp {
                return Err(ParserError {
                    message: "unary operator before `**` requires parentheses".to_string(),
                    location: self.scanner.peek_location(),
                });
            }
            return Ok(Expression::Unary {
                op,
                expression: Box::new(expression),
                position,
            });
        }
        if op.is_count_op() {
            let position = self.peek_position();
            self.next();
            let expression = self.parse_unary_or_prefix_expression()?;
            if !expression.is_valid_reference_expression() {
                return Err(ParserError {
                    message: format!("invalid target for `{op}`"),
                    location: Location::new(position, self.peek_position()),
                });
            }
            return Ok(Expression::Count {
                op,
                is_prefix: true,
                expression: Box::new(expression),
                position,
            });
        }
        if op == Token::Await {
            return Err(self.unexpected_token_with_hint("`await` is not implemented"));
        }
        self.parse_postfix_expression()
    }

    fn parse_postfix_expression(&mut self) -> Result<Expression> {
        let position = self.peek_position();
        let expression = self.parse_left_hand_side_expression()?;
        let op = self.peek();
        if !op.is_count_op() || self.scanner.has_line_terminator_before_next() {
            return Ok(expression);
        }
        if !expression.is_valid_reference_expression() {
            return Err(ParserError {
                message: format!("invalid target for `{op}`"),
                location: Location::new(position, self.peek_position()),
            });
        }
        self.next();
        Ok(Expression::Count {
            op,
            is_prefix: false,
            expression: Box::new(expression),
            position,
        })
    }

    fn parse_left_hand_side_expression(&mut self) -> Result<Expression> {
        let mut expression = self.parse_member_expression()?;
        loop {
            match self.peek() {
                Token::LeftParen => {
                    let position = self.peek_position();
                    let arguments = self.parse_arguments()?;
                    expression = Expression::Call {
                        callee: Box::new(expression),
                        arguments,
                        position,
                    };
                }
                Token::TemplateSpan | Token::TemplateTail => {
                    return Err(
                        self.unexpected_token_with_hint("tagged templates are not implemented")
                    );
                }
                token if token.is_property() => {
                    expression = self.parse_property_access(expression)?;
                }
                Token::QuestionPeriod => {
                    return Err(
                        self.unexpected_token_with_hint("optional chaining is not implemented")
                    );
                }
                _ => return Ok(expression),
            }
        }
    }

    fn parse_member_expression(&mut self) -> Result<Expression> {
        if self.peek() == Token::New {
            let position = self.peek_position();
            self.consume(Token::New);
            let callee = Box::new(self.parse_member_expression()?);
            let arguments = if self.peek() == Token::LeftParen {
                self.parse_arguments()?
            } else {
                Vec::new()
            };
            let mut expression = Expression::CallNew {
                callee,
                arguments,
                position,
            };
            while self.peek().is_property() {
                expression = self.parse_property_access(expression)?;
            }
            return Ok(expression);
        }
        let mut expression = self.parse_primary_expression()?;
        while self.peek().is_property() {
            expression = self.parse_property_access(expression)?;
        }
        Ok(expression)
    }

    fn parse_property_access(&mut self, object: Expression) -> Result<Expression> {
        let position = self.peek_position();
        match self.peek() {
            Token::Period => {
                self.consume(Token::Period);
                let token = self.peek();
                if !token.is_property_name() {
                    return Err(self.unexpected_token_with_hint("expected a property name"));
                }
                self.next();
                let name = self.property_name_text(token);
                Ok(Expression::Property {
                    object: Box::new(object),
                    key: Box::new(PropertyAccess::Named(name)),
                    position,
                })
            }
            Token::LeftBracket => {
                self.consume(Token::LeftBracket);
                let saved = self.accept_in;
                self.accept_in = true;
                let key = self.parse_expression();
                self.accept_in = saved;
                let key = key?;
                self.expect(Token::RightBracket)?;
                Ok(Expression::Property {
                    object: Box::new(object),
                    key: Box::new(PropertyAccess::Keyed(key)),
                    position,
                })
            }
            _ => Err(self.unexpected_token()),
        }
    }

    fn property_name_text(&self, token: Token) -> String {
        if token.is_any_identifier() {
            self.scanner.current_literal().to_string()
        } else {
            token.string().unwrap_or_default().to_string()
        }
    }

    fn parse_arguments(&mut self) -> Result<Vec<Expression>> {
        self.expect(Token::LeftParen)?;
        let mut arguments = Vec::new();
        let saved = self.accept_in;
        self.accept_in = true;
        let result = (|| -> Result<()> {
            while self.peek() != Token::RightParen {
                if self.peek() == Token::Ellipsis {
                    return Err(self.unexpected_token_with_hint("spread is not implemented"));
                }
                arguments.push(self.parse_assignment_expression()?);
                if !self.check(Token::Comma) {
                    break;
                }
            }
            Ok(())
        })();
        self.accept_in = saved;
        result?;
        self.expect(Token::RightParen)?;
        Ok(arguments)
    }

    fn parse_primary_expression(&mut self) -> Result<Expression> {
        let position = self.peek_position();
        match self.peek() {
            Token::Smi => {
                self.next();
                Ok(Expression::Literal {
                    value: Literal::Smi(self.scanner.smi_value()),
                    position,
                })
            }
            Token::Number => {
                self.next();
                Ok(Expression::Literal {
                    value: Literal::Number(self.scanner.number_value()),
                    position,
                })
            }
            Token::String => {
                self.next();
                Ok(Expression::Literal {
                    value: Literal::String(self.scanner.current_literal().to_string()),
                    position,
                })
            }
            Token::TrueLiteral => {
                self.next();
                Ok(Expression::Literal {
                    value: Literal::Boolean(true),
                    position,
                })
            }
            Token::FalseLiteral => {
                self.next();
                Ok(Expression::Literal {
                    value: Literal::Boolean(false),
                    position,
                })
            }
            Token::NullLiteral => {
                self.next();
                Ok(Expression::Literal {
                    value: Literal::Null,
                    position,
                })
            }
            Token::This => {
                self.next();
                self.record_reference("this");
                Ok(Expression::This { position })
            }
            Token::LeftParen => {
                if self.parenthesis_starts_an_arrow_parameter_list() {
                    return self.parse_parenthesised_arrow_function(position);
                }
                self.consume(Token::LeftParen);
                let saved = self.accept_in;
                self.accept_in = true;
                let expression = self.parse_expression();
                self.accept_in = saved;
                let expression = expression?;
                self.expect(Token::RightParen)?;
                Ok(expression)
            }
            Token::TemplateSpan | Token::TemplateTail => self.parse_template_literal(position),
            Token::LeftBracket => self.parse_array_literal(),
            Token::LeftBrace => self.parse_object_literal(),
            Token::Function => {
                self.consume(Token::Function);
                if self.peek() == Token::Mul {
                    return Err(self.unexpected_token_with_hint("generators are not implemented"));
                }
                let name = if self.peek().is_any_identifier() {
                    Some(self.parse_identifier()?)
                } else {
                    None
                };
                self.parse_function_expression(name, position)
            }
            Token::Super => {
                self.consume(Token::Super);
                if self.peek() == Token::LeftParen {
                    return Ok(Expression::SuperCallReference { position });
                }
                if !matches!(self.peek(), Token::Period | Token::LeftBracket) {
                    return Err(self.unexpected_token_with_hint(
                        "`super` must be followed by a call or a property access",
                    ));
                }
                let is_static = self.state().in_static_method;
                let name = if is_static {
                    crate::ast::scopes::K_STATIC_HOME_OBJECT
                } else {
                    crate::ast::scopes::K_HOME_OBJECT
                };
                self.record_reference(name);
                Ok(Expression::SuperPropertyReference {
                    is_static,
                    position,
                })
            }
            Token::Class => self.parse_class_expression(position),
            Token::Mod if crate::flags::flags::V8_FLAGS.allow_natives_syntax() => {
                self.parse_v8_intrinsic()
            }
            Token::Div | Token::AssignDiv => {
                Err(self
                    .unexpected_token_with_hint("regular expression literals are not implemented"))
            }
            token if token.is_any_identifier() => {
                let name = self.parse_identifier()?;
                if self.peek() == Token::Arrow {
                    return self.parse_arrow_function(vec![name], position);
                }
                self.record_reference(&name);
                Ok(Expression::VariableProxy { name, position })
            }
            _ => {
                self.check_scanner_error()?;
                Err(self.unexpected_token_with_hint("expected an expression"))
            }
        }
    }

    fn parse_v8_intrinsic(&mut self) -> Result<Expression> {
        let position = self.peek_position();
        self.consume(Token::Mod);
        let name = self.parse_identifier()?;
        if self.peek() != Token::LeftParen {
            return Err(self.unexpected_token_with_hint("expected an argument list"));
        }
        let arguments = self.parse_arguments()?;
        self.new_v8_intrinsic(&name, arguments, position)
    }

    fn new_v8_intrinsic(
        &mut self,
        name: &str,
        arguments: Vec<Expression>,
        position: crate::ast::ast::Position,
    ) -> Result<Expression> {
        let Some(function) = crate::runtime::runtime::FunctionId::function_for_name(name) else {
            return Err(self.unexpected_token_with_hint("not defined"));
        };

        if function.nargs != -1 && function.nargs != arguments.len() as i32 {
            return Err(
                self.unexpected_token_with_hint("runtime function given the wrong number of arguments")
            );
        }

        Ok(Expression::CallRuntime {
            function_id: function.function_id,
            arguments,
            position,
        })
    }

    fn parse_identifier(&mut self) -> Result<String> {
        let token = self.peek();
        if !token.is_valid_identifier(false, false, false) {
            self.check_scanner_error()?;
            return Err(self.unexpected_token_with_hint("expected an identifier"));
        }
        self.next();
        if token == Token::Identifier {
            Ok(self.scanner.current_literal().to_string())
        } else {
            Ok(token.string().unwrap_or_default().to_string())
        }
    }

    fn parse_array_literal(&mut self) -> Result<Expression> {
        let position = self.peek_position();
        self.expect(Token::LeftBracket)?;
        let mut values = Vec::new();
        while self.peek() != Token::RightBracket {
            if self.peek() == Token::Comma {
                self.consume(Token::Comma);
                values.push(Expression::Literal {
                    value: Literal::Undefined,
                    position: self.position(),
                });
                continue;
            }
            if self.peek() == Token::Ellipsis {
                return Err(self.unexpected_token_with_hint("spread is not implemented"));
            }
            values.push(self.parse_assignment_expression()?);
            if !self.check(Token::Comma) {
                break;
            }
        }
        self.expect(Token::RightBracket)?;
        Ok(Expression::ArrayLiteral { values, position })
    }

    fn parse_object_literal(&mut self) -> Result<Expression> {
        let position = self.peek_position();
        self.expect(Token::LeftBrace)?;
        let mut properties = Vec::new();
        while self.peek() != Token::RightBrace {
            properties.push(self.parse_object_property()?);
            if !self.check(Token::Comma) {
                break;
            }
        }
        self.expect(Token::RightBrace)?;
        Ok(Expression::ObjectLiteral {
            properties,
            position,
        })
    }

    fn parse_class_declaration(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        let class = self.parse_class_literal()?;
        let name = class
            .name
            .clone()
            .ok_or_else(|| self.unexpected_token_with_hint("a class declaration needs a name"))?;
        self.declare_variable(&name, VariableMode::Let);
        Ok(Statement::Expression {
            expression: Expression::Assignment {
                op: Token::Assign,
                target: Box::new(Expression::VariableProxy {
                    name,
                    position,
                }),
                value: Box::new(Expression::ClassLiteral {
                    class: Box::new(class),
                    position,
                }),
                position,
            },
            position,
        })
    }

    fn parse_class_literal(&mut self) -> Result<ClassLiteral> {
        let position = self.peek_position();
        self.expect(Token::Class)?;

        let name = if self.peek().is_any_identifier() && self.peek() != Token::Extends {
            Some(self.parse_identifier()?)
        } else {
            None
        };

        let extends = if self.check(Token::Extends) {
            Some(self.parse_left_hand_side_expression()?)
        } else {
            None
        };

        self.scope_chain.push(ScopeKind::Class);
        self.scope_chain
            .current()
            .declare(crate::ast::scopes::K_HOME_OBJECT, VariableMode::Const);
        self.scope_chain
            .current()
            .declare(crate::ast::scopes::K_STATIC_HOME_OBJECT, VariableMode::Const);

        let mut class_info = ClassInfo {
            instance_members_scope: self.new_field_initializer_scope(),
            static_members_scope: self.new_field_initializer_scope(),
            computed_field_count: 0,
        };

        self.expect(Token::LeftBrace)?;
        let mut public_members = Vec::new();
        let mut instance_fields: Vec<ClassLiteralField> = Vec::new();
        let mut static_fields: Vec<ClassLiteralField> = Vec::new();
        let mut constructor = None;
        while self.peek() != Token::RightBrace {
            if self.check(Token::Semicolon) {
                continue;
            }
            match self.parse_class_property_definition(&mut class_info)? {
                ClassMember::Constructor(value) => {
                    if constructor.is_some() {
                        return Err(
                            self.unexpected_token_with_hint("a class may have one constructor")
                        );
                    }
                    constructor = Some(value);
                }
                ClassMember::Property(property) => public_members.push(property),
                ClassMember::Field(field) => {
                    if field.is_static {
                        static_fields.push(field);
                    } else {
                        instance_fields.push(field);
                    }
                }
                ClassMember::ComputedField(property, field) => {
                    public_members.push(property);
                    if field.is_static {
                        static_fields.push(field);
                    } else {
                        instance_fields.push(field);
                    }
                }
            }
        }
        self.expect(Token::RightBrace)?;

        let mut constructor = match constructor {
            Some(constructor) => constructor,
            None => {
                self.default_constructor(name.clone(), extends.is_some(), position)?
            }
        };
        constructor.kind = if extends.is_some() {
            FunctionKind::DerivedConstructor
        } else {
            FunctionKind::BaseConstructor
        };
        constructor.requires_instance_members_initializer = !instance_fields.is_empty();

        let instance_members_initializer_function = if instance_fields.is_empty() {
            None
        } else {
            Some(self.field_initializer_function(
                &instance_fields,
                class_info.instance_members_scope.take().expect("the field scope"),
                position,
            ))
        };
        let static_initializer = if static_fields.is_empty() {
            None
        } else {
            Some(self.field_initializer_function(
                &static_fields,
                class_info.static_members_scope.take().expect("the field scope"),
                position,
            ))
        };
        let scope = self.scope_chain.pop();

        Ok(ClassLiteral {
            name,
            extends,
            constructor,
            public_members,
            instance_members_initializer_function,
            static_initializer,
            scope,
            position,
        })
    }

    fn parse_class_property_definition(&mut self, class_info: &mut ClassInfo) -> Result<ClassMember> {
        let is_static = self.peek() == Token::Static
            && !matches!(
                self.peek_ahead(),
                Token::LeftParen | Token::Assign | Token::Semicolon | Token::RightBrace
            );
        if is_static {
            self.consume(Token::Static);
        }

        let mut kind = ClassLiteralPropertyKind::Method;
        if matches!(self.peek(), Token::Get | Token::Set)
            && !matches!(
                self.peek_ahead(),
                Token::LeftParen | Token::Assign | Token::Semicolon | Token::RightBrace
            )
        {
            kind = if self.next() == Token::Get {
                ClassLiteralPropertyKind::Getter
            } else {
                ClassLiteralPropertyKind::Setter
            };
        }

        let position = self.peek_position();
        if self.peek() == Token::LeftBracket {
            self.consume(Token::LeftBracket);
            let saved = self.accept_in;
            self.accept_in = true;
            let key = self.parse_assignment_expression();
            self.accept_in = saved;
            let key = key?;
            self.expect(Token::RightBracket)?;
            if self.peek() != Token::LeftParen {
                if kind != ClassLiteralPropertyKind::Method {
                    return Err(self.unexpected_token_with_hint("an accessor needs a body"));
                }
                return self.parse_computed_class_field_initializer(
                    class_info, key, is_static, position,
                );
            }
            let value = self.parse_method_literal(None, is_static, position)?;
            return Ok(ClassMember::Property(ClassLiteralProperty {
                key: PropertyKey::Computed(key),
                value: Some(value),
                kind,
                is_static,
                computed_name_var: None,
            }));
        }
        let name = self.parse_property_name()?;
        if is_static && name == "prototype" {
            return Err(self.unexpected_token_with_hint(
                "Classes may not have a static property named 'prototype'",
            ));
        }
        if self.peek() != Token::LeftParen {
            if kind != ClassLiteralPropertyKind::Method {
                return Err(self.unexpected_token_with_hint("an accessor needs a body"));
            }
            return self.parse_class_field_initializer(class_info, name, is_static, position);
        }
        let value = self.parse_method_literal(Some(name.clone()), is_static, position)?;

        if !is_static && kind == ClassLiteralPropertyKind::Method && name == "constructor" {
            return Ok(ClassMember::Constructor(value));
        }
        Ok(ClassMember::Property(ClassLiteralProperty {
            key: PropertyKey::Name(name),
            value: Some(value),
            kind,
            is_static,
            computed_name_var: None,
        }))
    }

    fn class_field_variable_name(index: usize) -> String {
        format!(".class-field-{index}")
    }

    fn parse_computed_class_field_initializer(
        &mut self,
        class_info: &mut ClassInfo,
        key: Expression,
        is_static: bool,
        position: Position,
    ) -> Result<ClassMember> {
        let name = Self::class_field_variable_name(class_info.computed_field_count);
        class_info.computed_field_count += 1;
        self.scope_chain
            .current()
            .declare(&name, VariableMode::Const);

        let value = self.parse_field_initializer_value(class_info, is_static, position, &name)?;
        self.expect_semicolon()?;
        Ok(ClassMember::ComputedField(
            ClassLiteralProperty {
                key: PropertyKey::Computed(key),
                value: None,
                kind: ClassLiteralPropertyKind::Field,
                is_static,
                computed_name_var: Some(name.clone()),
            },
            ClassLiteralField {
                name,
                value,
                is_static,
                is_computed_name: true,
            },
        ))
    }

    fn parse_field_initializer_value(
        &mut self,
        class_info: &mut ClassInfo,
        is_static: bool,
        position: Position,
        computed_name_var: &str,
    ) -> Result<Expression> {
        let slot = if is_static {
            &mut class_info.static_members_scope
        } else {
            &mut class_info.instance_members_scope
        };
        self.scope_chain
            .push_scope(slot.take().expect("the field initializer scope"));
        if !computed_name_var.is_empty() {
            self.record_reference(computed_name_var);
        }

        let value = if self.check(Token::Assign) {
            self.parse_assignment_expression()
        } else {
            Ok(Expression::Literal {
                value: Literal::Undefined,
                position,
            })
        };

        let detached = self.scope_chain.detach();
        if is_static {
            class_info.static_members_scope = Some(detached);
        } else {
            class_info.instance_members_scope = Some(detached);
        }
        value
    }

    fn parse_class_field_initializer(
        &mut self,
        class_info: &mut ClassInfo,
        name: String,
        is_static: bool,
        position: Position,
    ) -> Result<ClassMember> {
        let value = self.parse_field_initializer_value(class_info, is_static, position, "")?;
        self.expect_semicolon()?;
        Ok(ClassMember::Field(ClassLiteralField {
            name,
            value,
            is_static,
            is_computed_name: false,
        }))
    }

    fn default_constructor(
        &mut self,
        name: Option<String>,
        call_super: bool,
        position: Position,
    ) -> Result<FunctionLiteral> {
        self.function_state.push(FunctionState::default());
        self.scope_chain.push(ScopeKind::Function);
        self.declare_receiver();
        let body = if call_super {
            vec![Statement::Return {
                value: Some(Expression::SuperCallForwardArgs { position }),
                position,
            }]
        } else {
            Vec::new()
        };
        let scope = self.scope_chain.pop();
        let state = self.function_state.pop().expect("the function state");
        Ok(FunctionLiteral {
            name,
            kind: FunctionKind::NormalFunction,
            requires_instance_members_initializer: false,
            parameters: Vec::new(),
            body,
            scope,
            hoisted_functions: state.hoisted_functions,
            position,
        })
    }

    fn parse_property_name(&mut self) -> Result<String> {
        let token = self.peek();
        Ok(match token {
            Token::String => {
                self.next();
                self.scanner.current_literal().to_string()
            }
            Token::Smi => {
                self.next();
                self.scanner.smi_value().to_string()
            }
            Token::Number => {
                self.next();
                crate::numbers::conversions::number_to_string(self.scanner.number_value())
            }
            token if token.is_property_name() => {
                self.next();
                self.property_name_text(token)
            }
            _ => return Err(self.unexpected_token_with_hint("expected a property name")),
        })
    }

    fn parse_object_property(&mut self) -> Result<ObjectLiteralProperty> {
        if self.peek() == Token::Ellipsis {
            return Err(self.unexpected_token_with_hint("spread is not implemented"));
        }

        if self.peek() == Token::LeftBracket {
            self.consume(Token::LeftBracket);
            let key = self.parse_assignment_expression()?;
            self.expect(Token::RightBracket)?;
            self.expect(Token::Colon)?;
            let value = self.parse_assignment_expression()?;
            return Ok(ObjectLiteralProperty {
                key: PropertyKey::Computed(key),
                value,
                kind: ObjectLiteralPropertyKind::Computed,
            });
        }

        if matches!(self.peek(), Token::Get | Token::Set)
            && !matches!(
                self.peek_ahead(),
                Token::Colon | Token::Comma | Token::RightBrace | Token::Assign | Token::LeftParen
            )
        {
            let token = self.next();
            let position = self.peek_position();
            let name = self.parse_property_name()?;
            let function = self.parse_function_literal(Some(name.clone()), position)?;
            return Ok(ObjectLiteralProperty {
                key: PropertyKey::Name(name),
                value: Expression::FunctionLiteral {
                    function: Box::new(function),
                    position,
                },
                kind: if token == Token::Get {
                    ObjectLiteralPropertyKind::Getter
                } else {
                    ObjectLiteralPropertyKind::Setter
                },
            });
        }

        let token = self.peek();
        let position = self.peek_position();
        let name = self.parse_property_name()?;

        if self.peek() == Token::LeftParen {
            return Err(self.unexpected_token_with_hint("method shorthand is not implemented"));
        }

        if matches!(self.peek(), Token::Comma | Token::RightBrace) {
            if !token.is_any_identifier() {
                return Err(ParserError {
                    message: format!("`{name}` cannot be used as a shorthand property"),
                    location: self.scanner.location(),
                });
            }
            self.record_reference(&name);
            return Ok(ObjectLiteralProperty {
                key: PropertyKey::Name(name.clone()),
                value: Expression::VariableProxy { name, position },
                kind: ObjectLiteralPropertyKind::Computed,
            });
        }

        self.expect(Token::Colon)?;
        let value = self.parse_assignment_expression()?;
        Ok(ObjectLiteralProperty {
            key: PropertyKey::Name(name),
            value,
            kind: ObjectLiteralPropertyKind::Computed,
        })
    }

    fn parse_method_literal(
        &mut self,
        name: Option<String>,
        is_static: bool,
        position: Position,
    ) -> Result<FunctionLiteral> {
        let parameters = self.parse_formal_parameter_list()?;
        self.function_state.push(FunctionState {
            hoisted_functions: Vec::new(),
            in_static_method: is_static,
        });
        self.scope_chain.push(ScopeKind::Function);
        self.declare_receiver();
        self.parse_function_body(name, FunctionKind::NormalFunction, parameters, position)
    }

    fn new_field_initializer_scope(&mut self) -> Option<crate::ast::scopes::Scope> {
        self.scope_chain.push(ScopeKind::Function);
        self.declare_receiver();
        Some(self.scope_chain.detach())
    }

    fn field_initializer_function(
        &mut self,
        fields: &[ClassLiteralField],
        scope: crate::ast::scopes::Scope,
        position: Position,
    ) -> FunctionLiteral {
        let body = vec![Statement::InitializeClassMembers {
            fields: fields.to_vec(),
            position,
        }];

        self.scope_chain.push_scope(scope);
        let scope = self.scope_chain.pop();
        FunctionLiteral {
            name: None,
            kind: FunctionKind::NormalFunction,
            requires_instance_members_initializer: false,
            parameters: Vec::new(),
            body,
            scope,
            hoisted_functions: Vec::new(),
            position,
        }
    }



    fn declare_receiver(&mut self) {
        self.scope_chain.current().declare_parameter("this", -1);
    }

    fn parse_function_literal(
        &mut self,
        name: Option<String>,
        position: Position,
    ) -> Result<FunctionLiteral> {
        let parameters = self.parse_formal_parameter_list()?;

        self.function_state.push(FunctionState::default());
        self.scope_chain.push(ScopeKind::Function);
        self.declare_receiver();
        self.parse_function_body(name, FunctionKind::NormalFunction, parameters, position)
    }

    fn parse_function_body(
        &mut self,
        name: Option<String>,
        kind: FunctionKind,
        parameters: Vec<String>,
        position: Position,
    ) -> Result<FunctionLiteral> {
        for (index, parameter) in parameters.iter().enumerate() {
            self.scope_chain
                .current()
                .declare_parameter(parameter, index as i32);
        }

        let body = (|| -> Result<Vec<Statement>> {
            self.expect(Token::LeftBrace)?;
            let saved = self.accept_in;
            self.accept_in = true;
            let body = self.parse_statement_list(Token::RightBrace);
            self.accept_in = saved;
            let body = body?;
            self.expect(Token::RightBrace)?;
            Ok(body)
        })();

        let scope = self.scope_chain.pop();
        let state = self.function_state.pop().expect("the function state");
        let body = body?;

        Ok(FunctionLiteral {
            name,
            kind,
            requires_instance_members_initializer: false,
            parameters,
            body,
            scope,
            hoisted_functions: state.hoisted_functions,
            position,
        })
    }

    #[inline(never)]
    fn parse_template_literal(&mut self, position: Position) -> Result<Expression> {
        // A TemplateLiteral is made up of 0 or more kTemplateSpan tokens
        // (literal text followed by a substitution expression), finalized by a
        // single kTemplateTail.
        debug_assert!(matches!(
            self.peek(),
            Token::TemplateSpan | Token::TemplateTail
        ));

        // If we reach a kTemplateTail first, we are parsing a
        // NoSubstitutionTemplate.
        if self.peek() == Token::TemplateTail {
            self.next();
            let value = self.scanner.current_literal().to_string();
            return Ok(Expression::Literal {
                value: Literal::String(value),
                position,
            });
        }

        self.next();
        let mut string_parts = vec![self.scanner.current_literal().to_string()];
        let mut substitutions = Vec::new();
        loop {
            let saved = self.accept_in;
            self.accept_in = true;
            let expression = self.parse_expression();
            self.accept_in = saved;
            substitutions.push(expression?);

            if self.peek() != Token::RightBrace {
                return Err(self.unexpected_token_with_hint("unterminated template expression"));
            }
            let next = self.scanner.scan_template_continuation();
            self.next();
            string_parts.push(self.scanner.current_literal().to_string());
            if next != Token::TemplateSpan {
                break;
            }
        }

        Ok(Expression::TemplateLiteral {
            template: Box::new(TemplateLiteral {
                string_parts,
                substitutions,
            }),
            position,
        })
    }

    #[inline(never)]
    fn parse_class_expression(&mut self, position: Position) -> Result<Expression> {
        let class = self.parse_class_literal()?;
        Ok(Expression::ClassLiteral {
            class: Box::new(class),
            position,
        })
    }

    #[inline(never)]
    fn parse_function_expression(
        &mut self,
        name: Option<String>,
        position: Position,
    ) -> Result<Expression> {
        let function = self.parse_function_literal(name, position)?;
        Ok(Expression::FunctionLiteral {
            function: Box::new(function),
            position,
        })
    }

    #[inline(never)]
    fn parse_parenthesised_arrow_function(&mut self, position: Position) -> Result<Expression> {
        let parameters = self.parse_formal_parameter_list()?;
        self.parse_arrow_function(parameters, position)
    }

    #[inline(never)]
    fn parse_arrow_function(
        &mut self,
        parameters: Vec<String>,
        position: Position,
    ) -> Result<Expression> {
        let function = self.parse_arrow_function_literal(parameters, position)?;
        Ok(Expression::FunctionLiteral {
            function: Box::new(function),
            position,
        })
    }

    fn parenthesis_starts_an_arrow_parameter_list(&mut self) -> bool {
        debug_assert_eq!(self.peek(), Token::LeftParen);
        let bookmark = self.scanner.peek_location().beg_pos;
        let mut depth = 0;
        let is_arrow = loop {
            match self.scanner.next() {
                Token::LeftParen | Token::LeftBracket | Token::LeftBrace => depth += 1,
                Token::RightParen | Token::RightBracket | Token::RightBrace => {
                    depth -= 1;
                    if depth == 0 {
                        break self.scanner.peek() == Token::Arrow;
                    }
                }
                Token::Eos | Token::Illegal => break false,
                _ => {}
            }
        };
        self.scanner.seek_next(bookmark);
        is_arrow
    }

    fn parse_arrow_function_literal(
        &mut self,
        parameters: Vec<String>,
        position: Position,
    ) -> Result<FunctionLiteral> {
        debug_assert_eq!(self.peek(), Token::Arrow);
        if self.scanner.has_line_terminator_before_next() {
            // No line terminator allowed between the parameters and the arrow:
            // ArrowFunction[In, Yield, Await] :
            //   ArrowParameters[?Yield, ?Await] [no LineTerminator here] =>
            //   ConciseBody[?In]
            // If the next token is not `=>`, it's a syntax error anyway.
            return Err(self.unexpected_token_with_hint("no newline before `=>`"));
        }
        self.expect(Token::Arrow)?;
        self.function_state.push(FunctionState {
            hoisted_functions: Vec::new(),
            in_static_method: self
                .function_state
                .last()
                .is_some_and(|state| state.in_static_method),
        });
        self.scope_chain.push(ScopeKind::Function);

        if self.peek() == Token::LeftBrace {
            return self.parse_function_body(None, FunctionKind::ArrowFunction, parameters, position);
        }

        for (index, parameter) in parameters.iter().enumerate() {
            self.scope_chain
                .current()
                .declare_parameter(parameter, index as i32);
        }
        let saved = self.accept_in;
        self.accept_in = true;
        let value = self.parse_assignment_expression();
        self.accept_in = saved;
        let scope = self.scope_chain.pop();
        let state = self.function_state.pop().expect("the function state");
        let value = value?;

        Ok(FunctionLiteral {
            name: None,
            kind: FunctionKind::ArrowFunction,
            requires_instance_members_initializer: false,
            parameters,
            body: vec![Statement::Return {
                value: Some(value),
                position,
            }],
            scope,
            hoisted_functions: state.hoisted_functions,
            position,
        })
    }

    fn parse_formal_parameter_list(&mut self) -> Result<Vec<String>> {
        self.expect(Token::LeftParen)?;
        let mut parameters = Vec::new();
        while self.peek() != Token::RightParen {
            if self.peek() == Token::Ellipsis {
                return Err(self.unexpected_token_with_hint("rest parameters are not implemented"));
            }
            if matches!(self.peek(), Token::LeftBracket | Token::LeftBrace) {
                return Err(
                    self.unexpected_token_with_hint("destructuring parameters are not implemented")
                );
            }
            let name = self.parse_identifier()?;
            if self.peek() == Token::Assign {
                return Err(
                    self.unexpected_token_with_hint("default parameters are not implemented")
                );
            }
            parameters.push(name);
            if !self.check(Token::Comma) {
                break;
            }
        }
        self.expect(Token::RightParen)?;
        Ok(parameters)
    }
}
