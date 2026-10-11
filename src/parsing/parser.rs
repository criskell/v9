//! Ported from `src/parsing/parser-base.h` and `src/parsing/parser.cc`.
//!
//! Recursive descent, with precedence climbing for the binary operators --
//! the same shape as V8's `ParserBase`, and the method names are V8's so the
//! two can be read side by side.
//!
//! The expression chain, from loosest binding to tightest, is V8's exactly:
//!
//! ```text
//!   ParseExpression              ,
//!   ParseAssignmentExpression    =  +=  ...
//!   ParseConditionalExpression   ?:
//!   ParseLogicalExpression       ||  &&      and, separately, ??
//!   ParseBinaryExpression(4)     |  ^  &  ==  <  <<  +  *  **
//!   ParseUnaryOrPrefixExpression !  ~  -  typeof  ++x
//!   ParsePostfixExpression       x++
//!   ParseLeftHandSideExpression  f()
//!   ParseMemberExpression        o.x  o[x]
//!   ParsePrimaryExpression       literals, names, ( )
//! ```
//!
//! `??` is split out into `ParseCoalesceExpression` for a real grammar
//! reason, not a stylistic one: the spec forbids mixing `??` with `||` or
//! `&&` without parentheses, so it cannot simply take a precedence slot.
//!
//! Deviations, tracked in `PORTING.md`:
//!
//! * **No preparser.** V8 parses function bodies lazily, scanning them once
//!   with a `PreParser` that only records scope information and parsing them
//!   fully on first call. `vx` parses everything eagerly.
//! * **No scope tree.** `var` declarations are hoisted into a flat per
//!   function list for the bytecode generator; V8 builds a `Scope` chain and
//!   resolves variables during parsing. `let`/`const` are parsed but treated
//!   as function scoped, so their block scoping and temporal dead zone are
//!   *not* enforced.
//! * No classes, generators, async functions, destructuring, spread,
//!   template literals, `for...in`/`for...of`, `switch`, `try`, `with`,
//!   labels, `new`, getters/setters.

use crate::ast::ast::*;
use crate::ast::scopes::{ScopeChain, ScopeKind};
use crate::parsing::scanner::{Location, Scanner};
use crate::parsing::token::Token;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParserError {
    pub message: String,
    pub location: Location,
}

impl ParserError {
    /// Renders the error against the source, the way a `SyntaxError` reads.
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

/// `ParserBase::FunctionState`: what the parser has to remember about the
/// function it is currently inside.
///
/// The variables themselves live in the [`ScopeChain`], which is what V8's
/// `Scope` tree does; this holds what is left.
#[derive(Default)]
struct FunctionState {
    hoisted_functions: Vec<FunctionLiteral>,
}

pub struct Parser {
    scanner: Scanner,
    /// `ParserBase::scope_`: the path from the script scope to the one being
    /// parsed. Declarations go in, references are recorded, and closing a
    /// scope resolves what it could not.
    scope_chain: ScopeChain,
    /// The `[In]` grammar parameter. Inside a `for` header, `in` is not a
    /// relational operator, so precedence climbing has to stop before it.
    accept_in: bool,
    function_state: Vec<FunctionState>,
    /// How deep the recursion is, so a pathological input fails with an
    /// error instead of blowing the stack. V8 does the same with
    /// `kMaxNumFunctionParameters`-style limits and a stack guard.
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

    /// `Parser::ParseProgram`
    pub fn parse_program(source: &str) -> Result<Program> {
        let mut parser = Parser::new(source);
        let body = parser.parse_statement_list(Token::Eos)?;
        parser.expect(Token::Eos)?;
        parser.check_scanner_error()?;
        let state = parser.function_state.pop().expect("the top-level state");
        // Closing the script scope resolves whatever is still unresolved:
        // those names are globals.
        let scope = parser.scope_chain.pop_root();
        Ok(Program {
            body,
            scope,
            hoisted_functions: state.hoisted_functions,
        })
    }

    // -- the token stream ---------------------------------------------------

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

    /// `ParserBase::Check`: consume the token if it is there.
    #[inline]
    fn check(&mut self, token: Token) -> bool {
        if self.peek() == token {
            self.next();
            true
        } else {
            false
        }
    }

    /// `ParserBase::Expect`
    fn expect(&mut self, token: Token) -> Result<()> {
        if self.peek() != token {
            return Err(self.unexpected_token_with_hint(&format!("expected `{token}`")));
        }
        self.next();
        Ok(())
    }

    /// `ParserBase::Consume`: expect and discard, for a token the caller has
    /// already peeked at.
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

    /// The scanner reports lexical errors out of band, so they have to be
    /// picked up before a parse is declared successful -- otherwise an
    /// unterminated string would parse as whatever token followed it.
    fn check_scanner_error(&self) -> Result<()> {
        match self.scanner.error() {
            Some(error) => Err(ParserError {
                message: error.message.clone(),
                location: error.location,
            }),
            None => Ok(()),
        }
    }

    /// `ParserBase::ExpectSemicolon`: automatic semicolon insertion.
    ///
    /// A statement may end without a `;` if the next token is on a new line,
    /// or is `}`, or is the end of input. This is the whole of ASI's "offered
    /// semicolon" rule.
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

    /// `Parser::DeclareVariable`: the binding belongs to the enclosing
    /// function scope, wherever in it the declaration was written. That is
    /// what hoisting means.
    fn declare_variable(&mut self, name: &str, mode: VariableMode) {
        self.scope_chain.current().declare(name, mode);
    }

    /// `Scope::NewUnresolved`: a reference, to be resolved when the scope
    /// closes -- it may be declared later in the same function, or in an
    /// enclosing one that is still being parsed.
    fn record_reference(&mut self, name: &str) {
        self.scope_chain.current().record_reference(name);
    }

    // -----------------------------------------------------------------------
    // Statements
    // -----------------------------------------------------------------------

    /// `ParserBase::ParseStatementList`
    fn parse_statement_list(&mut self, end: Token) -> Result<Vec<Statement>> {
        let mut statements = Vec::new();
        while self.peek() != end {
            if self.peek() == Token::Eos {
                // Ran out of input before the closing token.
                self.check_scanner_error()?;
                return Err(self.unexpected_token_with_hint(&format!("expected `{end}`")));
            }
            statements.push(self.parse_statement_list_item()?);
        }
        Ok(statements)
    }

    /// `ParserBase::ParseStatementListItem`: a statement, or one of the
    /// declarations that are only legal at the top of a list.
    fn parse_statement_list_item(&mut self) -> Result<Statement> {
        match self.peek() {
            Token::Function => self.parse_hoistable_declaration(),
            Token::Const => self.parse_variable_statement(VariableMode::Const),
            // `let` is contextual: `let x = 1` declares, but `let` alone, or
            // `let + 1`, is an expression using a variable called `let`.
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

    /// `ParserBase::ParseStatement`
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
            Token::Function => {
                // A function declaration is not a statement -- it is only
                // legal where a statement *list item* is expected. V8 allows
                // it in sloppy mode with `SloppyBlockFunctionStatement`;
                // `vx` just accepts it.
                self.parse_hoistable_declaration()
            }
            Token::Switch | Token::Try | Token::With | Token::Throw | Token::Debugger => {
                Err(self.unexpected_token_with_hint("this statement is not implemented"))
            }
            Token::Class => Err(self.unexpected_token_with_hint("classes are not implemented")),
            _ => self.parse_expression_statement(),
        }
    }

    /// `ParserBase::ParseBlock`
    fn parse_block(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        self.expect(Token::LeftBrace)?;
        let statements = self.parse_statement_list(Token::RightBrace)?;
        self.expect(Token::RightBrace)?;
        Ok(Statement::Block {
            statements,
            position,
        })
    }

    /// `ParserBase::ParseVariableStatement`
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

    /// `ParserBase::ParseVariableDeclarations`
    fn parse_variable_declarations(
        &mut self,
        mode: VariableMode,
    ) -> Result<Vec<VariableDeclaration>> {
        let mut declarations = Vec::new();
        loop {
            let position = self.peek_position();
            let name = self.parse_identifier()?;
            // Deviation: `let` and `const` are given *function* scope, like
            // `var`. Block scoping and the temporal dead zone need block
            // scopes, which `vx` does not have -- see `crate::ast::scopes`.
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

    /// `ParserBase::ParseHoistableDeclaration`
    fn parse_hoistable_declaration(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        self.consume(Token::Function);
        if self.peek() == Token::Mul {
            return Err(self.unexpected_token_with_hint("generators are not implemented"));
        }
        let name = self.parse_identifier()?;
        let function = self.parse_function_literal(Some(name.clone()), position)?;
        // `Parser::DeclareFunction`: the function is recorded on the
        // enclosing scope and an `EmptyStatement` is left in the body.
        //
        // It is not a statement, and that is the whole point: the binding is
        // created and initialized when the scope is entered, which is why a
        // function can be called above the line that defines it. Leaving a
        // `FunctionDeclaration` node in the statement list would make the
        // bytecode generator emit the closure twice -- once at scope entry
        // and once where it is written.
        self.state().hoisted_functions.push(function);
        self.declare_variable(&name, VariableMode::Var);
        Ok(Statement::Empty { position })
    }

    /// `ParserBase::ParseIfStatement`
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

    /// `ParserBase::ParseWhileStatement`
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

    /// `ParserBase::ParseDoWhileStatement`
    fn parse_do_while_statement(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        self.consume(Token::Do);
        let body = Box::new(self.parse_statement()?);
        self.expect(Token::While)?;
        self.expect(Token::LeftParen)?;
        let condition = self.parse_expression()?;
        self.expect(Token::RightParen)?;
        // `do {} while (x) y` is legal without a semicolon: the spec gives
        // do-while an explicit optional one, not just ASI.
        self.check(Token::Semicolon);
        Ok(Statement::DoWhile {
            body,
            condition,
            position,
        })
    }

    /// `ParserBase::ParseForStatement`
    fn parse_for_statement(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        self.consume(Token::For);
        self.expect(Token::LeftParen)?;

        // The init clause is parsed with `in` demoted, so that `for (a in b)`
        // is not read as `for (<a in b>; ...)`. This is the only reason the
        // `[In]` grammar parameter exists.
        let init = if self.peek() == Token::Semicolon {
            None
        } else {
            let saved = self.accept_in;
            self.accept_in = false;
            let init = self.parse_for_init();
            self.accept_in = saved;
            Some(Box::new(init?))
        };

        if matches!(self.peek(), Token::In | Token::Of) {
            return Err(
                self.unexpected_token_with_hint("for...in and for...of are not implemented")
            );
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

    /// `ParserBase::ParseReturnStatement`
    fn parse_return_statement(&mut self) -> Result<Statement> {
        let position = self.peek_position();
        self.consume(Token::Return);
        // A restricted production: a newline after `return` ends the
        // statement, so `return\n x` returns undefined. This is the rule that
        // makes `return` on its own line a bug magnet.
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

    /// `ParserBase::ParseExpressionOrLabelledStatement`.
    ///
    /// An identifier followed by `:` is a label, not an expression. V8 parses
    /// the expression first and then checks whether it turned out to be a
    /// bare identifier followed by a colon; `vx` looks ahead instead, which
    /// reaches the same decision without needing to un-parse.
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

    // -----------------------------------------------------------------------
    // Expressions
    // -----------------------------------------------------------------------

    /// `ParserBase::ParseExpression`: the comma operator.
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

    /// `ParserBase::ParseAssignmentExpression`
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

    /// `ParserBase::ParseConditionalExpression`
    fn parse_conditional_expression(&mut self) -> Result<Expression> {
        let position = self.peek_position();
        let expression = self.parse_logical_expression()?;
        if self.peek() != Token::Conditional {
            return Ok(expression);
        }
        self.consume(Token::Conditional);
        // The branches are assignment expressions, and the `then` branch is
        // parsed with `in` re-enabled even inside a `for` header.
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

    /// `ParserBase::ParseLogicalExpression`
    ///
    /// ```text
    ///   LogicalExpression ::
    ///     LogicalORExpression
    ///     CoalesceExpression
    /// ```
    ///
    /// Both alternatives start with a `BitwiseORExpression`, so that is
    /// parsed first -- at precedence **6**, not 4 -- and only then does the
    /// next token decide which production is in play.
    ///
    /// This is what makes `a || b ?? c` a syntax error without any explicit
    /// check for it. Once the `&&`/`||` branch is taken, the `??` arm is
    /// never reached; and inside the coalesce loop every operand is parsed at
    /// precedence 6, so a `||` can never be consumed there either. Either way
    /// the mixed operator is left in the stream and the statement parser
    /// rejects it. Parsing the head at precedence 4 instead would quietly
    /// accept the mixture.
    fn parse_logical_expression(&mut self) -> Result<Expression> {
        let expression = self.parse_binary_expression(6)?;
        if matches!(self.peek(), Token::And | Token::Or) {
            // A LogicalORExpression: pick up binary parsing where it left off.
            return self.parse_binary_continuation(expression, 4);
        }
        if self.peek() == Token::Nullish {
            return self.parse_coalesce_expression(expression);
        }
        Ok(expression)
    }

    /// `ParserBase::ParseCoalesceExpression`
    ///
    /// ```text
    ///   CoalesceExpression ::
    ///     CoalesceExpressionHead ?? BitwiseORExpression
    /// ```
    fn parse_coalesce_expression(&mut self, first: Expression) -> Result<Expression> {
        let mut expression = first;
        while self.peek() == Token::Nullish {
            let position = self.peek_position();
            self.consume(Token::Nullish);
            // `BitwiseORExpression` -- precedence 6, which is precisely why
            // `a ?? b || c` cannot parse.
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

    /// `ParserBase::ParseBinaryExpression` plus `ParseBinaryContinuation`:
    /// precedence climbing.
    ///
    /// Everything binds left, except `**`, which the spec makes
    /// right-associative -- hence the one-line difference in the precedence
    /// passed down for the right operand.
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

    /// `ParserBase::ParseBinaryContinuation`: the climbing loop, resumable
    /// with an operand that has already been parsed.
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

    /// `ParserBase::ParseUnaryOrPrefixExpression`
    fn parse_unary_or_prefix_expression(&mut self) -> Result<Expression> {
        let op = self.peek();
        if op.is_unary_op() {
            let position = self.peek_position();
            self.next();
            let expression = self.parse_unary_or_prefix_expression()?;
            // `-2 ** 2` is a syntax error: the spec refuses to guess whether
            // the unary minus binds tighter than `**`.
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

    /// `ParserBase::ParsePostfixExpression`
    fn parse_postfix_expression(&mut self) -> Result<Expression> {
        let position = self.peek_position();
        let expression = self.parse_left_hand_side_expression()?;
        let op = self.peek();
        // Another restricted production: a newline before `++` means the
        // `++` belongs to the *next* statement.
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

    /// `ParserBase::ParseLeftHandSideExpression`: member accesses and calls,
    /// which chain in any order (`a.b().c[d]()`).
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

    /// `ParserBase::ParseMemberExpression`
    fn parse_member_expression(&mut self) -> Result<Expression> {
        if self.peek() == Token::New {
            return Err(self.unexpected_token_with_hint("`new` is not implemented"));
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
                // Any reserved word is a legal property name: `o.class`.
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
                // `in` is a relational operator again inside brackets.
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

    /// The text of a token being used as a property name. For an identifier
    /// the scanner kept the literal; for a keyword, the spelling is the name.
    fn property_name_text(&self, token: Token) -> String {
        if token.is_any_identifier() {
            self.scanner.current_literal().to_string()
        } else {
            token.string().unwrap_or_default().to_string()
        }
    }

    /// `ParserBase::ParseArguments`
    fn parse_arguments(&mut self) -> Result<Vec<Expression>> {
        self.expect(Token::LeftParen)?;
        let mut arguments = Vec::new();
        // `in` is unrestricted inside an argument list.
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

    /// `ParserBase::ParsePrimaryExpression`
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
                Ok(Expression::This { position })
            }
            Token::LeftParen => {
                self.consume(Token::LeftParen);
                if self.peek() == Token::RightParen {
                    return Err(
                        self.unexpected_token_with_hint("arrow functions are not implemented")
                    );
                }
                let saved = self.accept_in;
                self.accept_in = true;
                let expression = self.parse_expression();
                self.accept_in = saved;
                let expression = expression?;
                self.expect(Token::RightParen)?;
                if self.peek() == Token::Arrow {
                    return Err(
                        self.unexpected_token_with_hint("arrow functions are not implemented")
                    );
                }
                Ok(expression)
            }
            Token::LeftBracket => self.parse_array_literal(),
            Token::LeftBrace => self.parse_object_literal(),
            Token::Function => {
                self.consume(Token::Function);
                if self.peek() == Token::Mul {
                    return Err(self.unexpected_token_with_hint("generators are not implemented"));
                }
                // A function *expression*'s name is optional and visible only
                // inside its own body.
                let name = if self.peek().is_any_identifier() {
                    Some(self.parse_identifier()?)
                } else {
                    None
                };
                let function = self.parse_function_literal(name, position)?;
                Ok(Expression::FunctionLiteral {
                    function: Box::new(function),
                    position,
                })
            }
            Token::Class => Err(self.unexpected_token_with_hint("classes are not implemented")),
            Token::Div | Token::AssignDiv => {
                Err(self
                    .unexpected_token_with_hint("regular expression literals are not implemented"))
            }
            token if token.is_any_identifier() => {
                let name = self.parse_identifier()?;
                if self.peek() == Token::Arrow {
                    return Err(
                        self.unexpected_token_with_hint("arrow functions are not implemented")
                    );
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

    /// `ParserBase::ParseIdentifier`
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
            // A contextual keyword used as a name: `var of = 1`.
            Ok(token.string().unwrap_or_default().to_string())
        }
    }

    /// `ParserBase::ParseArrayLiteral`
    fn parse_array_literal(&mut self) -> Result<Expression> {
        let position = self.peek_position();
        self.expect(Token::LeftBracket)?;
        let mut values = Vec::new();
        while self.peek() != Token::RightBracket {
            if self.peek() == Token::Comma {
                // A hole: `[1, , 3]`. Represented as `undefined`, which is
                // what reading one yields; V8 tracks holes in the elements
                // kind so it can tell them from a stored `undefined`.
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

    /// `ParserBase::ParseObjectLiteral`
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

    fn parse_object_property(&mut self) -> Result<ObjectLiteralProperty> {
        if self.peek() == Token::Ellipsis {
            return Err(self.unexpected_token_with_hint("spread is not implemented"));
        }

        // A computed key: `{[expr]: value}`.
        if self.peek() == Token::LeftBracket {
            self.consume(Token::LeftBracket);
            let key = self.parse_assignment_expression()?;
            self.expect(Token::RightBracket)?;
            self.expect(Token::Colon)?;
            let value = self.parse_assignment_expression()?;
            return Ok(ObjectLiteralProperty {
                key: PropertyKey::Computed(key),
                value,
            });
        }

        let token = self.peek();
        let position = self.peek_position();
        let name = match token {
            Token::String => {
                self.next();
                self.scanner.current_literal().to_string()
            }
            Token::Smi => {
                self.next();
                // A numeric key is a string key: `{1: x}` and `{"1": x}` name
                // the same property.
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
        };

        if self.peek() == Token::LeftParen {
            return Err(self.unexpected_token_with_hint("method shorthand is not implemented"));
        }

        // Shorthand: `{x}` means `{x: x}`.
        if matches!(self.peek(), Token::Comma | Token::RightBrace) {
            if !token.is_any_identifier() {
                return Err(ParserError {
                    message: format!("`{name}` cannot be used as a shorthand property"),
                    location: self.scanner.location(),
                });
            }
            // `{x}` is `{x: x}`, so the shorthand is a reference too.
            self.record_reference(&name);
            return Ok(ObjectLiteralProperty {
                key: PropertyKey::Name(name.clone()),
                value: Expression::VariableProxy { name, position },
            });
        }

        self.expect(Token::Colon)?;
        let value = self.parse_assignment_expression()?;
        Ok(ObjectLiteralProperty {
            key: PropertyKey::Name(name),
            value,
        })
    }

    /// `ParserBase::ParseFunctionLiteral`. The `function` keyword and the
    /// name have already been consumed.
    fn parse_function_literal(
        &mut self,
        name: Option<String>,
        position: Position,
    ) -> Result<FunctionLiteral> {
        let parameters = self.parse_formal_parameter_list()?;

        // A fresh scope and function state: `var`s hoist to the function
        // they are in, not to the enclosing one.
        self.function_state.push(FunctionState::default());
        self.scope_chain.push(ScopeKind::Function);
        for (index, parameter) in parameters.iter().enumerate() {
            self.scope_chain
                .current()
                .declare_parameter(parameter, index as i32);
        }

        let body = (|| -> Result<Vec<Statement>> {
            self.expect(Token::LeftBrace)?;
            // `in` is unrestricted inside a body, even when the function
            // literal appears in a `for` header.
            let saved = self.accept_in;
            self.accept_in = true;
            let body = self.parse_statement_list(Token::RightBrace);
            self.accept_in = saved;
            let body = body?;
            self.expect(Token::RightBrace)?;
            Ok(body)
        })();

        // Closing the scope is where capture is detected: a name this
        // function could not resolve is looked for outward, and finding it
        // forces that variable into a context.
        let scope = self.scope_chain.pop();
        let state = self.function_state.pop().expect("the function state");
        let body = body?;

        Ok(FunctionLiteral {
            name,
            parameters,
            body,
            scope,
            hoisted_functions: state.hoisted_functions,
            position,
        })
    }

    /// `ParserBase::ParseFormalParameterList`
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
