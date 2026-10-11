//! Ported from `src/interpreter/bytecode-generator.h` /
//! `bytecode-generator.cc`.
//!
//! The AST walker that emits bytecode.
//!
//! Where the value goes
//! --------------------
//! The central idea, and the reason V8's generator is not a naive tree walk,
//! is that an expression is compiled differently depending on what its
//! result is *for*. V8 tracks this with an `ExpressionResultScope`; `vx`
//! passes it as a parameter, which is the same decision reached without a
//! stack:
//!
//! * [`Self::visit_for_accumulator_value`] -- leave the value in the
//!   accumulator.
//! * [`Self::visit_for_register_value`] -- leave it in a fresh register.
//! * [`Self::visit_for_effect`] -- the value is discarded, so don't produce
//!   it.
//! * [`Self::visit_for_test`] -- the value is a branch condition, so compile
//!   it *into* the branch rather than materialising a boolean and then
//!   testing it.
//!
//! The last one is what makes `if (a && b)` compile to two conditional jumps
//! with no boolean ever existing, and `if (!x)` compile with the branches
//! swapped instead of with a `LogicalNot`.
//!
//! Deviations, tracked in `PORTING.md`: no scope tree, so a variable is
//! either a register in the current function or a global -- there are no
//! contexts and therefore no closures over locals. No generators, no
//! `try`/`catch`, no classes, no destructuring, no `for...in`/`for...of`,
//! no labelled break. `let`/`const` are compiled as function-scoped, so
//! their temporal dead zone is not enforced.

use crate::ast::ast::*;
use crate::ast::scopes::Scope;
use crate::common::globals::VariableLocation;
use crate::interpreter::bytecode_array_builder::{BytecodeArrayBuilder, BytecodeArrayData};
use crate::interpreter::bytecode_label::BytecodeLabels;
use crate::interpreter::bytecode_register::Register;
use crate::interpreter::control_flow_builders::{ConditionalControlFlowBuilder, LoopBuilder};
use crate::parsing::token::Token;

/// `TestFallthrough`: which branch control reaches by *not* jumping.
///
/// Knowing this lets the generator emit one conditional jump instead of a
/// conditional jump plus an unconditional one.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TestFallthrough {
    Then,
    Else,
    None,
}

impl TestFallthrough {
    /// Swapping the branches swaps the fallthrough, which is how `!x` is
    /// compiled without emitting anything.
    fn inverted(self) -> TestFallthrough {
        match self {
            TestFallthrough::Then => TestFallthrough::Else,
            TestFallthrough::Else => TestFallthrough::Then,
            TestFallthrough::None => TestFallthrough::None,
        }
    }
}

/// One compiled function. Index 0 of a [`CompiledProgram`] is the top-level
/// script; the rest are its nested function literals, in the order the
/// generator reached them.
#[derive(Clone, Debug)]
pub struct CompiledFunction {
    pub name: String,
    pub declared_parameter_count: i32,
    pub data: BytecodeArrayData,
}

#[derive(Clone, Debug)]
pub struct CompiledProgram {
    pub functions: Vec<CompiledFunction>,
    /// `var`s and function declarations at the top level, which become
    /// properties of the global object.
    pub global_declarations: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub struct GeneratorError {
    pub message: String,
}

type Result<T> = std::result::Result<T, GeneratorError>;

/// `BytecodeGenerator`
/// Where a name's storage is, as the generator needs it.
///
/// `Variable::location()` plus the *depth* -- how many contexts out the
/// declaring scope is, which only the chain can say.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Resolution {
    /// A register in this frame.
    Local(Register),
    /// A parameter of this frame.
    Parameter(Register),
    /// A slot `depth` contexts out. Depth zero is the current context.
    Context { depth: i32, slot: i32 },
    /// A property of the global object.
    Global,
}

pub struct BytecodeGenerator {
    builder: BytecodeArrayBuilder,
    /// The scopes from the script down to the function being generated,
    /// which is what makes a context depth computable.
    scope_chain: Vec<Scope>,
    /// The register holding the context saved by `PushContext`, per scope
    /// that allocated one. `None` for a function that needed no context.
    context_registers: Vec<Option<Register>>,
    /// Loops currently being generated, innermost last. `break` and
    /// `continue` target the last entry.
    loop_stack: Vec<LoopBuilder>,
    /// Functions compiled so far, shared across the whole generation so that
    /// `CreateClosure` can name one by index.
    functions: Vec<CompiledFunction>,
}

impl BytecodeGenerator {
    /// `BytecodeGenerator::GenerateBytecode` for a whole script.
    pub fn generate(program: &Program) -> Result<CompiledProgram> {
        // The top-level script has only the receiver as a parameter.
        let mut generator = BytecodeGenerator {
            builder: BytecodeArrayBuilder::new(1, 0),
            scope_chain: vec![program.scope.clone()],
            context_registers: Vec::new(),
            loop_stack: Vec::new(),
            functions: Vec::new(),
        };
        // Reserve the slot the script itself will occupy, so nested
        // functions get stable indices while it is still being generated.
        generator.functions.push(CompiledFunction {
            name: "<script>".to_string(),
            declared_parameter_count: 0,
            data: BytecodeArrayBuilder::new(1, 0).to_bytecode_array(),
        });

        // At the top level, `var` declarations become globals rather than
        // registers -- which is why `var x` at the top level is visible as
        // `globalThis.x`.
        generator.builder = BytecodeArrayBuilder::new(1, 0);
        // The script may itself need a context, if a function captured one
        // of its top-level variables.
        generator.build_function_context(&program.scope)?;
        for function in &program.hoisted_functions {
            let index = generator.compile_function(function)?;
            let name = function
                .name
                .clone()
                .expect("a hoisted function declaration has a name");
            generator.builder.create_closure(index);
            generator.builder.store_global(&name);
        }
        // A script has a *completion value*: the value of its last
        // expression statement, which is what `eval` yields and what a REPL
        // prints. Everything before it is generated for effect.
        match program.body.split_last() {
            Some((Statement::Expression { expression, .. }, rest)) => {
                generator.visit_statements(rest)?;
                if !generator.builder.exit_seen_in_block() {
                    generator.visit_for_accumulator_value(expression)?;
                    generator.builder.emit_return();
                }
            }
            _ => generator.visit_statements(&program.body)?,
        }

        // Top-level variables that stayed unallocated are globals; the ones
        // a function captured live in the script's context instead.
        let declarations: Vec<String> = program
            .scope
            .variables()
            .iter()
            .filter(|variable| variable.location == VariableLocation::Unallocated)
            .map(|variable| variable.name.clone())
            .collect();

        let data = std::mem::replace(&mut generator.builder, BytecodeArrayBuilder::new(1, 0))
            .to_bytecode_array();
        generator.functions[0] = CompiledFunction {
            name: "<script>".to_string(),
            declared_parameter_count: 0,
            data,
        };
        Ok(CompiledProgram {
            functions: generator.functions,
            global_declarations: declarations,
        })
    }

    /// Compiles a nested function literal and returns its index.
    fn compile_function(&mut self, function: &FunctionLiteral) -> Result<usize> {
        // Reserve the index first, so a recursive function can refer to
        // itself while its own body is still being generated.
        let index = self.functions.len();
        self.functions.push(CompiledFunction {
            name: function.name.clone().unwrap_or_default(),
            declared_parameter_count: function.parameters.len() as i32,
            data: BytecodeArrayBuilder::new(1, 0).to_bytecode_array(),
        });

        // Scope analysis already decided where everything lives; the
        // register count is what it left in the frame.
        let parameter_count = function.parameters.len() as i32 + 1;
        let builder = BytecodeArrayBuilder::new(parameter_count, function.scope.local_count());

        let saved_builder = std::mem::replace(&mut self.builder, builder);
        let saved_loops = std::mem::take(&mut self.loop_stack);
        self.scope_chain.push(function.scope.clone());

        let result = (|| -> Result<()> {
            // If anything captured one of this function's variables, the
            // context has to exist before the first statement runs.
            self.build_function_context(&function.scope)?;

            // Nested function declarations are created on entry, before any
            // statement runs -- that is what hoisting means.
            let nested: Vec<FunctionLiteral> = function.hoisted_functions.clone();
            for inner in &nested {
                let inner_index = self.compile_function(inner)?;
                let name = inner.name.clone().expect("a declaration has a name");
                self.builder.create_closure(inner_index);
                self.build_variable_assignment(&name);
            }
            self.visit_statements(&function.body)
        })();

        let builder = std::mem::replace(&mut self.builder, saved_builder);
        self.scope_chain.pop();
        self.context_registers.pop();
        self.loop_stack = saved_loops;
        result?;

        self.functions[index] = CompiledFunction {
            name: function.name.clone().unwrap_or_default(),
            declared_parameter_count: function.parameters.len() as i32,
            data: builder.to_bytecode_array(),
        };
        Ok(index)
    }

    fn unsupported<T>(&self, what: &str) -> Result<T> {
        Err(GeneratorError {
            message: format!("{what} is not implemented"),
        })
    }

    // -----------------------------------------------------------------------
    // Statements
    // -----------------------------------------------------------------------

    /// `BytecodeGenerator::VisitStatements`
    fn visit_statements(&mut self, statements: &[Statement]) -> Result<()> {
        for statement in statements {
            self.visit_statement(statement)?;
            // V8 stops emitting once a block cannot be left normally; the
            // writer drops the rest anyway, but stopping here saves the work.
            if self.builder.exit_seen_in_block() {
                break;
            }
        }
        Ok(())
    }

    fn visit_statement(&mut self, statement: &Statement) -> Result<()> {
        self.builder.set_source_position(statement.position());
        match statement {
            Statement::Block { statements, .. } => self.visit_statements(statements),
            Statement::Expression { expression, .. } => self.visit_for_effect(expression),
            Statement::Empty { .. } => Ok(()),
            Statement::Declaration { declarations, .. } => {
                self.visit_variable_declarations(declarations)
            }
            Statement::If {
                condition,
                then_statement,
                else_statement,
                ..
            } => self.visit_if_statement(condition, then_statement, else_statement.as_deref()),
            Statement::While {
                condition, body, ..
            } => self.visit_while_statement(condition, body),
            Statement::DoWhile {
                body, condition, ..
            } => self.visit_do_while_statement(body, condition),
            Statement::For {
                init,
                condition,
                next,
                body,
                ..
            } => self.visit_for_statement(init.as_deref(), condition.as_ref(), next.as_ref(), body),
            Statement::Return { value, .. } => self.visit_return_statement(value.as_ref()),
            Statement::Break { .. } => {
                if self.loop_stack.is_empty() {
                    return Err(GeneratorError {
                        message: "`break` outside a loop".to_string(),
                    });
                }
                let depth = self.loop_stack.len() - 1;
                self.loop_stack[depth].emit_break(&mut self.builder);
                Ok(())
            }
            Statement::Continue { .. } => {
                if self.loop_stack.is_empty() {
                    return Err(GeneratorError {
                        message: "`continue` outside a loop".to_string(),
                    });
                }
                let depth = self.loop_stack.len() - 1;
                self.loop_stack[depth].emit_continue(&mut self.builder);
                Ok(())
            }
        }
    }

    /// `BytecodeGenerator::VisitVariableDeclaration`
    fn visit_variable_declarations(&mut self, declarations: &[VariableDeclaration]) -> Result<()> {
        for declaration in declarations {
            match &declaration.initializer {
                Some(initializer) => {
                    self.visit_for_accumulator_value(initializer)?;
                    self.build_variable_assignment(&declaration.name);
                }
                // A `var` with no initialiser does nothing at this point:
                // the binding already exists, holding undefined, because it
                // was hoisted.
                None => {}
            }
        }
        Ok(())
    }

    /// `BytecodeGenerator::VisitIfStatement`
    fn visit_if_statement(
        &mut self,
        condition: &Expression,
        then_statement: &Statement,
        else_statement: Option<&Statement>,
    ) -> Result<()> {
        let mut conditional = ConditionalControlFlowBuilder::new();

        // A constant condition is folded away, so `if (false)` emits nothing
        // and `while (true)` emits no test.
        match condition.to_boolean_is_true() {
            Some(true) => return self.visit_statement(then_statement),
            Some(false) => {
                return match else_statement {
                    Some(statement) => self.visit_statement(statement),
                    None => Ok(()),
                }
            }
            None => {}
        }

        let mut then_labels = std::mem::take(conditional.then_labels());
        let mut else_labels = std::mem::take(conditional.else_labels());
        self.visit_for_test(
            condition,
            &mut then_labels,
            &mut else_labels,
            TestFallthrough::Then,
        )?;
        *conditional.then_labels() = then_labels;
        *conditional.else_labels() = else_labels;

        conditional.then_branch(&mut self.builder);
        self.visit_statement(then_statement)?;

        if let Some(else_statement) = else_statement {
            conditional.jump_to_end(&mut self.builder);
            conditional.else_branch(&mut self.builder);
            self.visit_statement(else_statement)?;
        } else {
            conditional.else_branch(&mut self.builder);
        }

        conditional.finish(&mut self.builder);
        Ok(())
    }

    /// `BytecodeGenerator::VisitWhileStatement`
    fn visit_while_statement(&mut self, condition: &Expression, body: &Statement) -> Result<()> {
        self.loop_stack.push(LoopBuilder::new());
        let depth = self.loop_stack.len() - 1;
        self.loop_stack[depth].loop_header(&mut self.builder);

        // `while (true)` needs no test at all.
        let mut exit_labels = BytecodeLabels::new();
        let always_true = condition.to_boolean_is_true() == Some(true);
        if !always_true {
            let mut body_labels = BytecodeLabels::new();
            self.visit_for_test(
                condition,
                &mut body_labels,
                &mut exit_labels,
                TestFallthrough::Then,
            )?;
            self.builder.bind_labels(&mut body_labels);
        }

        self.visit_statement(body)?;
        self.loop_stack[depth].bind_continue_target(&mut self.builder);
        self.loop_stack[depth].jump_to_header(&mut self.builder);

        // The condition's false exit and `break` both land here.
        self.builder.bind_labels(&mut exit_labels);
        self.loop_stack[depth].finish(&mut self.builder);
        self.loop_stack.pop();
        Ok(())
    }

    /// `BytecodeGenerator::VisitDoWhileStatement`: the body runs before the
    /// condition is ever evaluated.
    fn visit_do_while_statement(&mut self, body: &Statement, condition: &Expression) -> Result<()> {
        self.loop_stack.push(LoopBuilder::new());
        let depth = self.loop_stack.len() - 1;
        self.loop_stack[depth].loop_header(&mut self.builder);

        self.visit_statement(body)?;
        self.loop_stack[depth].bind_continue_target(&mut self.builder);

        let mut exit_labels = BytecodeLabels::new();
        if condition.to_boolean_is_true() == Some(true) {
            self.loop_stack[depth].jump_to_header(&mut self.builder);
        } else {
            // The instruction after the test is the backward jump, so the
            // `then` branch is the one reached by falling through -- jump
            // out only when the condition is false.
            let mut again_labels = BytecodeLabels::new();
            self.visit_for_test(
                condition,
                &mut again_labels,
                &mut exit_labels,
                TestFallthrough::Then,
            )?;
            self.builder.bind_labels(&mut again_labels);
            self.loop_stack[depth].jump_to_header(&mut self.builder);
        }

        self.builder.bind_labels(&mut exit_labels);
        self.loop_stack[depth].finish(&mut self.builder);
        self.loop_stack.pop();
        Ok(())
    }

    /// `BytecodeGenerator::VisitForStatement`
    fn visit_for_statement(
        &mut self,
        init: Option<&Statement>,
        condition: Option<&Expression>,
        next: Option<&Expression>,
        body: &Statement,
    ) -> Result<()> {
        if let Some(init) = init {
            self.visit_statement(init)?;
        }

        self.loop_stack.push(LoopBuilder::new());
        let depth = self.loop_stack.len() - 1;
        self.loop_stack[depth].loop_header(&mut self.builder);

        let mut exit_labels = BytecodeLabels::new();
        if let Some(condition) = condition {
            if condition.to_boolean_is_true() != Some(true) {
                let mut body_labels = BytecodeLabels::new();
                self.visit_for_test(
                    condition,
                    &mut body_labels,
                    &mut exit_labels,
                    TestFallthrough::Then,
                )?;
                self.builder.bind_labels(&mut body_labels);
            }
        }

        self.visit_statement(body)?;

        // `continue` lands here, *before* the increment, so that it still
        // runs. That is the whole reason the continue target is separate
        // from the loop header.
        self.loop_stack[depth].bind_continue_target(&mut self.builder);
        if let Some(next) = next {
            self.visit_for_effect(next)?;
        }
        self.loop_stack[depth].jump_to_header(&mut self.builder);

        self.builder.bind_labels(&mut exit_labels);
        self.loop_stack[depth].finish(&mut self.builder);
        self.loop_stack.pop();
        Ok(())
    }

    /// `BytecodeGenerator::VisitReturnStatement`
    fn visit_return_statement(&mut self, value: Option<&Expression>) -> Result<()> {
        match value {
            Some(value) => self.visit_for_accumulator_value(value)?,
            // `return;` returns undefined.
            None => {
                self.builder.load_undefined();
            }
        }
        self.builder.emit_return();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Variables
    // -----------------------------------------------------------------------

    /// `Scope::LookupRecursively`, with the depth counted.
    ///
    /// Depth counts only the scopes that *have* a context: a function that
    /// captured nothing allocates none, and a closure created in it keeps
    /// using the context it was created in. Counting scope levels instead of
    /// contexts would walk one link too many.
    fn resolve(&self, name: &str) -> Resolution {
        let mut context_depth = 0;
        for (level, scope) in self.scope_chain.iter().rev().enumerate() {
            if let Some(variable) = scope.lookup_local(name) {
                return match variable.location {
                    VariableLocation::Context => Resolution::Context {
                        depth: context_depth,
                        slot: variable.index,
                    },
                    VariableLocation::Local => {
                        debug_assert_eq!(
                            level, 0,
                            "a register in an enclosing frame should have been \
                             promoted to a context slot by scope analysis"
                        );
                        Resolution::Local(Register::new(variable.index))
                    }
                    VariableLocation::Parameter => {
                        debug_assert_eq!(level, 0, "likewise for a parameter");
                        // Parameter 0 is the receiver, so a declared
                        // parameter is one further along.
                        Resolution::Parameter(Register::from_parameter_index(variable.index + 1))
                    }
                    VariableLocation::Unallocated => Resolution::Global,
                };
            }
            if scope.needs_context() {
                context_depth += 1;
            }
        }
        Resolution::Global
    }

    /// `BytecodeGenerator::BuildFunctionContext`: allocate this scope's
    /// context, if it needs one, and copy the captured parameters in.
    fn build_function_context(&mut self, scope: &Scope) -> Result<()> {
        if !scope.needs_context() {
            self.context_registers.push(None);
            return Ok(());
        }

        let names = scope.context_local_names();
        self.builder
            .create_function_context(&names, scope.context_slot_count());
        // `PushContext` saves the context we were called in and installs the
        // new one. The saved one is what the new context's `previous` points
        // at, so the chain is built by the act of pushing.
        let saved = self.builder.register_allocator().new_register();
        self.builder.push_context(saved);
        self.context_registers.push(Some(saved));

        // A captured parameter arrives in the frame but has to be read from
        // the context from now on, so it is copied once at entry.
        for (parameter_index, slot) in scope.context_allocated_parameters() {
            let register = self.builder.parameter(parameter_index);
            self.builder.load_accumulator_with_register(register);
            self.builder.store_current_context_slot(slot);
        }
        Ok(())
    }

    /// `BytecodeGenerator::BuildVariableLoad`
    fn build_variable_load(&mut self, name: &str) {
        match self.resolve(name) {
            Resolution::Local(register) | Resolution::Parameter(register) => {
                self.builder.load_accumulator_with_register(register);
            }
            // The depth-zero case gets its own bytecode because it is the
            // overwhelmingly common one.
            Resolution::Context { depth: 0, slot } => {
                self.builder.load_current_context_slot(slot);
            }
            Resolution::Context { depth, slot } => {
                self.builder.load_context_slot(slot, depth);
            }
            // Not declared anywhere on the chain: a global, deferred to run
            // time with a feedback slot to cache it.
            Resolution::Global => {
                self.builder.load_global(name);
            }
        }
    }

    /// `BytecodeGenerator::BuildVariableAssignment`: stores the accumulator.
    fn build_variable_assignment(&mut self, name: &str) {
        match self.resolve(name) {
            Resolution::Local(register) | Resolution::Parameter(register) => {
                self.builder.store_accumulator_in_register(register);
            }
            Resolution::Context { depth: 0, slot } => {
                self.builder.store_current_context_slot(slot);
            }
            Resolution::Context { depth, slot } => {
                self.builder.store_context_slot(slot, depth);
            }
            Resolution::Global => {
                self.builder.store_global(name);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Expression result scopes
    // -----------------------------------------------------------------------

    /// `BytecodeGenerator::VisitForAccumulatorValue`
    fn visit_for_accumulator_value(&mut self, expression: &Expression) -> Result<()> {
        self.builder.set_source_position(expression.position());
        match expression {
            Expression::Literal { value, .. } => {
                self.builder.load_literal(value);
                Ok(())
            }
            Expression::VariableProxy { name, .. } => {
                self.build_variable_load(name);
                Ok(())
            }
            // No contexts, so `this` at the top level is undefined.
            Expression::This { .. } => {
                self.builder
                    .load_accumulator_with_register(Register::receiver());
                Ok(())
            }
            Expression::Assignment {
                op, target, value, ..
            } => self.visit_assignment(*op, target, value),
            Expression::Binary {
                op, left, right, ..
            } => self.visit_binary_operation(*op, left, right),
            Expression::Compare {
                op, left, right, ..
            } => self.visit_compare_operation(*op, left, right),
            Expression::Logical {
                op, left, right, ..
            } => self.visit_logical_operation_for_value(*op, left, right),
            Expression::Unary { op, expression, .. } => self.visit_unary_operation(*op, expression),
            Expression::Count {
                op,
                is_prefix,
                expression,
                ..
            } => self.visit_count_operation(*op, *is_prefix, expression),
            Expression::Conditional {
                condition,
                then_expression,
                else_expression,
                ..
            } => self.visit_conditional(condition, then_expression, else_expression),
            Expression::Property { object, key, .. } => self.visit_property(object, key),
            Expression::Call {
                callee, arguments, ..
            } => self.visit_call(callee, arguments),
            Expression::ObjectLiteral { properties, .. } => self.visit_object_literal(properties),
            Expression::ArrayLiteral { values, .. } => self.visit_array_literal(values),
            Expression::FunctionLiteral { function, .. } => {
                let index = self.compile_function(function)?;
                self.builder.create_closure(index);
                Ok(())
            }
            Expression::Sequence { expressions, .. } => {
                let (last, rest) = expressions
                    .split_last()
                    .expect("a sequence has at least one expression");
                for expression in rest {
                    self.visit_for_effect(expression)?;
                }
                self.visit_for_accumulator_value(last)
            }
        }
    }

    /// `BytecodeGenerator::VisitForRegisterValue`: the value in a fresh
    /// register, which is what any operation needing two operands does with
    /// its left-hand side.
    fn visit_for_register_value(&mut self, expression: &Expression) -> Result<Register> {
        // A variable already in a register needs no copy -- the peephole
        // that saves a Star on every `a + b`. A context variable is *not* in
        // a register, so it does not qualify.
        if let Expression::VariableProxy { name, .. } = expression {
            match self.resolve(name) {
                Resolution::Local(register) | Resolution::Parameter(register) => {
                    return Ok(register)
                }
                _ => {}
            }
        }
        self.visit_for_accumulator_value(expression)?;
        let register = self.builder.register_allocator().new_register();
        self.builder.store_accumulator_in_register(register);
        Ok(register)
    }

    /// `BytecodeGenerator::VisitForEffect`: the value is thrown away, so
    /// don't compute one.
    fn visit_for_effect(&mut self, expression: &Expression) -> Result<()> {
        match expression {
            // A literal or a variable read with nothing done to it has no
            // effect at all.
            Expression::Literal { .. }
            | Expression::VariableProxy { .. }
            | Expression::This { .. } => Ok(()),
            Expression::Sequence { expressions, .. } => {
                for expression in expressions {
                    self.visit_for_effect(expression)?;
                }
                Ok(())
            }
            Expression::Conditional {
                condition,
                then_expression,
                else_expression,
                ..
            } => {
                // Both arms for effect, so neither produces a value.
                let mut conditional = ConditionalControlFlowBuilder::new();
                let mut then_labels = BytecodeLabels::new();
                let mut else_labels = BytecodeLabels::new();
                self.visit_for_test(
                    condition,
                    &mut then_labels,
                    &mut else_labels,
                    TestFallthrough::Then,
                )?;
                self.builder.bind_labels(&mut then_labels);
                self.visit_for_effect(then_expression)?;
                conditional.jump_to_end(&mut self.builder);
                self.builder.bind_labels(&mut else_labels);
                self.visit_for_effect(else_expression)?;
                conditional.finish(&mut self.builder);
                Ok(())
            }
            // Everything else may have a side effect, so it is generated and
            // its value simply left in the accumulator for the next
            // instruction to overwrite.
            _ => self.visit_for_accumulator_value(expression),
        }
    }

    /// `BytecodeGenerator::VisitForTest`.
    ///
    /// Compiles `expression` as a condition: control reaches one of the two
    /// label sets, and no boolean is produced. `fallthrough` says which side
    /// control arrives at by not jumping, so only the other side needs one.
    fn visit_for_test(
        &mut self,
        expression: &Expression,
        then_labels: &mut BytecodeLabels,
        else_labels: &mut BytecodeLabels,
        fallthrough: TestFallthrough,
    ) -> Result<()> {
        match expression {
            // `!x`: swap the branches instead of emitting a negation.
            Expression::Unary {
                op: Token::Not,
                expression,
                ..
            } => self.visit_for_test(expression, else_labels, then_labels, fallthrough.inverted()),

            // `a && b`: if `a` is false the whole thing is false; otherwise
            // the answer is `b`. No boolean is materialised for `a`.
            Expression::Logical {
                op: Token::And,
                left,
                right,
                ..
            } => {
                let mut inner_then = BytecodeLabels::new();
                self.visit_for_test(left, &mut inner_then, else_labels, TestFallthrough::Then)?;
                self.builder.bind_labels(&mut inner_then);
                self.visit_for_test(right, then_labels, else_labels, fallthrough)
            }

            // `a || b`, the mirror image.
            Expression::Logical {
                op: Token::Or,
                left,
                right,
                ..
            } => {
                let mut inner_else = BytecodeLabels::new();
                self.visit_for_test(left, then_labels, &mut inner_else, TestFallthrough::Else)?;
                self.builder.bind_labels(&mut inner_else);
                self.visit_for_test(right, then_labels, else_labels, fallthrough)
            }

            _ => {
                // Anything else: produce the value, then branch on it.
                // `JumpIfToBoolean*` does the coercion, so no `ToBoolean` is
                // needed first.
                let produces_boolean = matches!(expression, Expression::Compare { .. });
                self.visit_for_accumulator_value(expression)?;
                match fallthrough {
                    TestFallthrough::Then => {
                        let label = else_labels.new_label();
                        if produces_boolean {
                            self.builder.jump_if_false(else_labels.label_mut(label));
                        } else {
                            self.builder
                                .jump_if_to_boolean_false(else_labels.label_mut(label));
                        }
                    }
                    TestFallthrough::Else => {
                        let label = then_labels.new_label();
                        if produces_boolean {
                            self.builder.jump_if_true(then_labels.label_mut(label));
                        } else {
                            self.builder
                                .jump_if_to_boolean_true(then_labels.label_mut(label));
                        }
                    }
                    TestFallthrough::None => {
                        let then_label = then_labels.new_label();
                        if produces_boolean {
                            self.builder.jump_if_true(then_labels.label_mut(then_label));
                        } else {
                            self.builder
                                .jump_if_to_boolean_true(then_labels.label_mut(then_label));
                        }
                        let else_label = else_labels.new_label();
                        self.builder.jump(else_labels.label_mut(else_label));
                    }
                }
                Ok(())
            }
        }
    }

    // -----------------------------------------------------------------------
    // Expressions
    // -----------------------------------------------------------------------

    /// `BytecodeGenerator::VisitBinaryOperation`
    fn visit_binary_operation(
        &mut self,
        op: Token,
        left: &Expression,
        right: &Expression,
    ) -> Result<()> {
        // `x + 1`: the Smi-literal form saves a register and a load. V8 only
        // takes it when the *right* operand is the literal, because the
        // operators that have this form are not all commutative.
        if let Expression::Literal {
            value: Literal::Smi(literal),
            ..
        } = right
        {
            if matches!(
                op,
                Token::Add | Token::Sub | Token::Mul | Token::Div | Token::Mod
            ) {
                self.visit_for_accumulator_value(left)?;
                self.builder.binary_operation_smi_literal(op, *literal);
                return Ok(());
            }
        }

        let mark = self.builder.register_allocator().next_register_index();
        let left_register = self.visit_for_register_value(left)?;
        self.visit_for_accumulator_value(right)?;
        self.builder.binary_operation(op, left_register);
        self.builder.register_allocator().release_registers(mark);
        Ok(())
    }

    /// `BytecodeGenerator::VisitCompareOperation`
    fn visit_compare_operation(
        &mut self,
        op: Token,
        left: &Expression,
        right: &Expression,
    ) -> Result<()> {
        // A comparison against `null` or `undefined` gets an operand-free
        // bytecode -- but which one depends on strictness, and getting that
        // wrong makes `null == undefined` false.
        if let Some((value, oddball)) = compare_against_oddball(op, left, right) {
            self.visit_for_accumulator_value(value)?;
            match op {
                // `x === null` is identity, and so is `x === undefined`.
                Token::EqStrict => match oddball {
                    Literal::Null => self.builder.compare_null(),
                    Literal::Undefined => self.builder.compare_undefined(),
                    _ => unreachable!(),
                },
                // `x == null` and `x == undefined` are the same test: both
                // are true for either value.
                Token::Eq => self.builder.compare_undetectable(),
                other => unreachable!("{other} does not fold against an oddball"),
            };
            return Ok(());
        }

        // `!=` and `!==` have no bytecode: emit the positive test and negate
        // it. The result of a comparison is always a boolean, so the cheap
        // `LogicalNot` is enough -- no coercion needed.
        let (positive, negate) = match op {
            Token::NotEq => (Token::Eq, true),
            Token::NotEqStrict => (Token::EqStrict, true),
            other => (other, false),
        };

        let mark = self.builder.register_allocator().next_register_index();
        let left_register = self.visit_for_register_value(left)?;
        self.visit_for_accumulator_value(right)?;
        self.builder.compare_operation(positive, left_register);
        if negate {
            self.builder.logical_not();
        }
        self.builder.register_allocator().release_registers(mark);
        Ok(())
    }

    /// `a && b` where the *value* is wanted, not a branch. Unlike the test
    /// form, this has to leave one of the operands in the accumulator --
    /// `a && b` evaluates to `a` when `a` is falsy, not to `false`.
    fn visit_logical_operation_for_value(
        &mut self,
        op: Token,
        left: &Expression,
        right: &Expression,
    ) -> Result<()> {
        let mut end_labels = BytecodeLabels::new();
        self.visit_for_accumulator_value(left)?;
        match op {
            // `a && b` is `a` when `a` is falsy, so jump over `b` then.
            Token::And => {
                let label = end_labels.new_label();
                self.builder
                    .jump_if_to_boolean_false(end_labels.label_mut(label));
                self.visit_for_accumulator_value(right)?;
            }
            // `a || b` is `a` when `a` is truthy.
            Token::Or => {
                let label = end_labels.new_label();
                self.builder
                    .jump_if_to_boolean_true(end_labels.label_mut(label));
                self.visit_for_accumulator_value(right)?;
            }
            // `a ?? b` is `a` unless `a` is nullish -- the *opposite* test
            // from the other two, and there is no `JumpIfNotUndefinedOrNull`
            // bytecode, so the sense is inverted with a second jump.
            Token::Nullish => {
                let mut right_labels = BytecodeLabels::new();
                let to_right = right_labels.new_label();
                self.builder
                    .jump_if_undefined_or_null(right_labels.label_mut(to_right));
                let to_end = end_labels.new_label();
                self.builder.jump(end_labels.label_mut(to_end));
                self.builder.bind_labels(&mut right_labels);
                self.visit_for_accumulator_value(right)?;
            }
            other => unreachable!("{other} is not a logical operator"),
        }
        self.builder.bind_labels(&mut end_labels);
        Ok(())
    }

    /// `BytecodeGenerator::VisitUnaryOperation`
    fn visit_unary_operation(&mut self, op: Token, expression: &Expression) -> Result<()> {
        match op {
            Token::Not => {
                self.visit_for_accumulator_value(expression)?;
                self.builder.to_boolean_logical_not();
                Ok(())
            }
            Token::TypeOf => {
                self.visit_for_accumulator_value(expression)?;
                self.builder.type_of();
                Ok(())
            }
            Token::Void => {
                self.visit_for_effect(expression)?;
                self.builder.load_undefined();
                Ok(())
            }
            Token::Add | Token::Sub | Token::BitNot => {
                self.visit_for_accumulator_value(expression)?;
                self.builder.unary_operation(op);
                Ok(())
            }
            Token::Delete => self.unsupported("`delete`"),
            other => unreachable!("{other} is not a unary operator"),
        }
    }

    /// `BytecodeGenerator::VisitCountOperation`: `++x` and `x++`.
    ///
    /// The difference is only *when* the old value is saved: a postfix
    /// operation keeps it in a register before incrementing, which is the
    /// one extra instruction `x++` costs over `++x`.
    fn visit_count_operation(
        &mut self,
        op: Token,
        is_prefix: bool,
        target: &Expression,
    ) -> Result<()> {
        let mark = self.builder.register_allocator().next_register_index();

        match target {
            Expression::VariableProxy { name, .. } => {
                let name = name.clone();
                self.build_variable_load(&name);
                // `x++` must evaluate to a number even if `x` was a string,
                // so the old value is converted before being saved.
                self.builder.to_number();
                let old_value = if is_prefix {
                    None
                } else {
                    let register = self.builder.register_allocator().new_register();
                    self.builder.store_accumulator_in_register(register);
                    Some(register)
                };
                self.emit_count(op);
                self.build_variable_assignment(&name);
                if let Some(old_value) = old_value {
                    self.builder.load_accumulator_with_register(old_value);
                }
            }
            Expression::Property { object, key, .. } => {
                let object_register = self.visit_for_register_value(object)?;
                match key.as_ref() {
                    PropertyAccess::Named(name) => {
                        let name = name.clone();
                        self.builder.load_named_property(object_register, &name);
                        self.builder.to_number();
                        let old_value = if is_prefix {
                            None
                        } else {
                            let register = self.builder.register_allocator().new_register();
                            self.builder.store_accumulator_in_register(register);
                            Some(register)
                        };
                        self.emit_count(op);
                        self.builder.set_named_property(object_register, &name);
                        match old_value {
                            Some(old_value) => {
                                self.builder.load_accumulator_with_register(old_value);
                            }
                            None => {
                                // The store clobbers the accumulator, so the
                                // new value has to be reloaded.
                                self.builder.load_named_property(object_register, &name);
                            }
                        }
                    }
                    PropertyAccess::Keyed(key) => {
                        let key_register = self.visit_for_register_value(key)?;
                        self.builder.load_accumulator_with_register(key_register);
                        self.builder.load_keyed_property(object_register);
                        self.builder.to_number();
                        let value = self.builder.register_allocator().new_register();
                        if !is_prefix {
                            self.builder.store_accumulator_in_register(value);
                        }
                        self.emit_count(op);
                        let new_value = self.builder.register_allocator().new_register();
                        self.builder.store_accumulator_in_register(new_value);
                        self.builder.load_accumulator_with_register(new_value);
                        self.builder
                            .set_keyed_property(object_register, key_register);
                        if is_prefix {
                            self.builder.load_accumulator_with_register(new_value);
                        } else {
                            self.builder.load_accumulator_with_register(value);
                        }
                    }
                }
            }
            _ => {
                return Err(GeneratorError {
                    message: format!("invalid target for `{op}`"),
                })
            }
        }

        self.builder.register_allocator().release_registers(mark);
        Ok(())
    }

    fn emit_count(&mut self, op: Token) {
        if op == Token::Inc {
            self.builder.increment();
        } else {
            self.builder.decrement();
        }
    }

    /// `BytecodeGenerator::VisitAssignment`
    fn visit_assignment(
        &mut self,
        op: Token,
        target: &Expression,
        value: &Expression,
    ) -> Result<()> {
        if op.is_logical_assignment_op() {
            return self.unsupported("logical assignment");
        }
        let mark = self.builder.register_allocator().next_register_index();

        match target {
            Expression::VariableProxy { name, .. } => {
                let name = name.clone();
                if op == Token::Assign {
                    self.visit_for_accumulator_value(value)?;
                } else {
                    // `a += b` evaluates `a` once. The old value goes in a
                    // register so the operator can take it as its left side.
                    let binary_op = op.binary_op_for_assignment();
                    self.build_variable_load(&name);
                    let old_value = self.builder.register_allocator().new_register();
                    self.builder.store_accumulator_in_register(old_value);
                    self.visit_for_accumulator_value(value)?;
                    self.builder.binary_operation(binary_op, old_value);
                }
                self.build_variable_assignment(&name);
                // A store to a register leaves the accumulator alone, so the
                // assignment's value is already there.
            }
            Expression::Property { object, key, .. } => {
                let object_register = self.visit_for_register_value(object)?;
                match key.as_ref() {
                    PropertyAccess::Named(name) => {
                        let name = name.clone();
                        if op == Token::Assign {
                            self.visit_for_accumulator_value(value)?;
                        } else {
                            let binary_op = op.binary_op_for_assignment();
                            self.builder.load_named_property(object_register, &name);
                            let old_value = self.builder.register_allocator().new_register();
                            self.builder.store_accumulator_in_register(old_value);
                            self.visit_for_accumulator_value(value)?;
                            self.builder.binary_operation(binary_op, old_value);
                        }
                        // The value has to survive the store, which clobbers
                        // the accumulator.
                        let stored = self.builder.register_allocator().new_register();
                        self.builder.store_accumulator_in_register(stored);
                        self.builder.load_accumulator_with_register(stored);
                        self.builder.set_named_property(object_register, &name);
                        self.builder.load_accumulator_with_register(stored);
                    }
                    PropertyAccess::Keyed(key) => {
                        let key_register = self.visit_for_register_value(key)?;
                        if op == Token::Assign {
                            self.visit_for_accumulator_value(value)?;
                        } else {
                            let binary_op = op.binary_op_for_assignment();
                            self.builder.load_accumulator_with_register(key_register);
                            self.builder.load_keyed_property(object_register);
                            let old_value = self.builder.register_allocator().new_register();
                            self.builder.store_accumulator_in_register(old_value);
                            self.visit_for_accumulator_value(value)?;
                            self.builder.binary_operation(binary_op, old_value);
                        }
                        let stored = self.builder.register_allocator().new_register();
                        self.builder.store_accumulator_in_register(stored);
                        self.builder.load_accumulator_with_register(stored);
                        self.builder
                            .set_keyed_property(object_register, key_register);
                        self.builder.load_accumulator_with_register(stored);
                    }
                }
            }
            _ => {
                return Err(GeneratorError {
                    message: "invalid assignment target".to_string(),
                })
            }
        }

        self.builder.register_allocator().release_registers(mark);
        Ok(())
    }

    /// `BytecodeGenerator::VisitConditional`
    fn visit_conditional(
        &mut self,
        condition: &Expression,
        then_expression: &Expression,
        else_expression: &Expression,
    ) -> Result<()> {
        let mut conditional = ConditionalControlFlowBuilder::new();
        let mut then_labels = BytecodeLabels::new();
        let mut else_labels = BytecodeLabels::new();
        self.visit_for_test(
            condition,
            &mut then_labels,
            &mut else_labels,
            TestFallthrough::Then,
        )?;
        self.builder.bind_labels(&mut then_labels);
        self.visit_for_accumulator_value(then_expression)?;
        conditional.jump_to_end(&mut self.builder);
        self.builder.bind_labels(&mut else_labels);
        self.visit_for_accumulator_value(else_expression)?;
        conditional.finish(&mut self.builder);
        Ok(())
    }

    /// `BytecodeGenerator::VisitProperty`
    fn visit_property(&mut self, object: &Expression, key: &PropertyAccess) -> Result<()> {
        let mark = self.builder.register_allocator().next_register_index();
        let object_register = self.visit_for_register_value(object)?;
        match key {
            // A name known at compile time gets `GetNamedProperty` with an
            // inline cache; a computed one gets the keyed form.
            PropertyAccess::Named(name) => {
                let name = name.clone();
                self.builder.load_named_property(object_register, &name);
            }
            PropertyAccess::Keyed(key) => {
                self.visit_for_accumulator_value(key)?;
                self.builder.load_keyed_property(object_register);
            }
        }
        self.builder.register_allocator().release_registers(mark);
        Ok(())
    }

    /// `BytecodeGenerator::VisitCall`
    fn visit_call(&mut self, callee: &Expression, arguments: &[Expression]) -> Result<()> {
        let mark = self.builder.register_allocator().next_register_index();

        // `o.f(x)` passes `o` as the receiver; `f(x)` passes undefined. The
        // distinction has to be made here, before the callee is evaluated,
        // because `o` must be evaluated exactly once.
        match callee {
            Expression::Property { object, key, .. } => {
                let object_register = self.visit_for_register_value(object)?;
                let callee_register = self.builder.register_allocator().new_register();
                match key.as_ref() {
                    PropertyAccess::Named(name) => {
                        let name = name.clone();
                        self.builder.load_named_property(object_register, &name);
                    }
                    PropertyAccess::Keyed(key) => {
                        self.visit_for_accumulator_value(key)?;
                        self.builder.load_keyed_property(object_register);
                    }
                }
                self.builder.store_accumulator_in_register(callee_register);

                // The receiver is the first register of the argument list.
                let mut args = self
                    .builder
                    .register_allocator()
                    .new_growable_register_list();
                let receiver = self
                    .builder
                    .register_allocator()
                    .grow_register_list(&mut args);
                self.builder.move_register(object_register, receiver);
                for argument in arguments {
                    self.visit_for_accumulator_value(argument)?;
                    let register = self
                        .builder
                        .register_allocator()
                        .grow_register_list(&mut args);
                    self.builder.store_accumulator_in_register(register);
                }
                self.builder.call_property(callee_register, args);
            }
            _ => {
                let callee_register = self.visit_for_register_value(callee)?;
                let mut args = self
                    .builder
                    .register_allocator()
                    .new_growable_register_list();
                for argument in arguments {
                    self.visit_for_accumulator_value(argument)?;
                    let register = self
                        .builder
                        .register_allocator()
                        .grow_register_list(&mut args);
                    self.builder.store_accumulator_in_register(register);
                }
                self.builder.call_undefined_receiver(callee_register, args);
            }
        }

        self.builder.register_allocator().release_registers(mark);
        Ok(())
    }

    /// `BytecodeGenerator::VisitObjectLiteral`
    fn visit_object_literal(&mut self, properties: &[ObjectLiteralProperty]) -> Result<()> {
        let mark = self.builder.register_allocator().next_register_index();
        self.builder.create_empty_object_literal();
        if properties.is_empty() {
            return Ok(());
        }
        let object = self.builder.register_allocator().new_register();
        self.builder.store_accumulator_in_register(object);

        for property in properties {
            match &property.key {
                PropertyKey::Name(name) => {
                    let name = name.clone();
                    self.visit_for_accumulator_value(&property.value)?;
                    // The property is known not to exist yet, so no setter
                    // on the prototype chain can intercept it.
                    self.builder.define_named_own_property(object, &name);
                }
                PropertyKey::Computed(key) => {
                    let key_register = self.visit_for_register_value(key)?;
                    self.builder.load_accumulator_with_register(key_register);
                    self.builder.to_name();
                    let name_register = self.builder.register_allocator().new_register();
                    self.builder.store_accumulator_in_register(name_register);
                    self.visit_for_accumulator_value(&property.value)?;
                    self.builder.set_keyed_property(object, name_register);
                }
            }
        }

        self.builder.load_accumulator_with_register(object);
        self.builder.register_allocator().release_registers(mark);
        Ok(())
    }

    /// `BytecodeGenerator::VisitArrayLiteral`
    fn visit_array_literal(&mut self, values: &[Expression]) -> Result<()> {
        let mark = self.builder.register_allocator().next_register_index();
        self.builder.create_empty_array_literal();
        if values.is_empty() {
            return Ok(());
        }
        let array = self.builder.register_allocator().new_register();
        self.builder.store_accumulator_in_register(array);
        let index_register = self.builder.register_allocator().new_register();

        for (index, value) in values.iter().enumerate() {
            self.builder.load_literal_smi(index as i32);
            self.builder.store_accumulator_in_register(index_register);
            self.visit_for_accumulator_value(value)?;
            self.builder.set_keyed_property(array, index_register);
        }

        self.builder.load_accumulator_with_register(array);
        self.builder.register_allocator().release_registers(mark);
        Ok(())
    }
}

/// `x == null` / `null == x` and the `undefined` forms, which have
/// operand-free bytecodes because they are common and need no feedback.
fn compare_against_oddball<'a>(
    op: Token,
    left: &'a Expression,
    right: &'a Expression,
) -> Option<(&'a Expression, Literal)> {
    if !matches!(op, Token::Eq | Token::EqStrict) {
        return None;
    }
    let is_oddball = |expression: &Expression| match expression {
        Expression::Literal { value, .. } => {
            matches!(value, Literal::Null | Literal::Undefined).then(|| value.clone())
        }
        _ => None,
    };
    if let Some(oddball) = is_oddball(right) {
        return Some((left, oddball));
    }
    if let Some(oddball) = is_oddball(left) {
        return Some((right, oddball));
    }
    None
}
