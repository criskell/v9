use crate::ast::ast::*;
use crate::ast::scopes::Scope;
use crate::common::globals::VariableLocation;
use crate::interpreter::bytecode_array_builder::{BytecodeArrayBuilder, BytecodeArrayData};
use crate::interpreter::bytecode_label::BytecodeLabels;
use crate::interpreter::bytecode_register::Register;
use crate::interpreter::control_flow_builders::{ConditionalControlFlowBuilder, LoopBuilder};
use crate::parsing::token::Token;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TestFallthrough {
    Then,
    Else,
    None,
}

impl TestFallthrough {
    fn inverted(self) -> TestFallthrough {
        match self {
            TestFallthrough::Then => TestFallthrough::Else,
            TestFallthrough::Else => TestFallthrough::Then,
            TestFallthrough::None => TestFallthrough::None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CompiledFunction {
    pub name: String,
    pub declared_parameter_count: i32,
    pub data: BytecodeArrayData,
}

#[derive(Clone, Debug)]
pub struct CompiledProgram {
    pub functions: Vec<CompiledFunction>,
    pub global_declarations: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub struct GeneratorError {
    pub message: String,
}

type Result<T> = std::result::Result<T, GeneratorError>;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Resolution {
    Local(Register),
    Parameter(Register),
    Context {
        depth: i32,
        slot: i32,
    },
    /// A property of the global object.
    Global,
}

pub struct BytecodeGenerator {
    builder: BytecodeArrayBuilder,
    scope_chain: Vec<Scope>,
    context_registers: Vec<Option<Register>>,
    loop_stack: Vec<LoopBuilder>,
    functions: Vec<CompiledFunction>,
}

impl BytecodeGenerator {
    pub fn generate(program: &Program) -> Result<CompiledProgram> {
        let mut generator = BytecodeGenerator {
            builder: BytecodeArrayBuilder::new(1, 0),
            scope_chain: vec![program.scope.clone()],
            context_registers: Vec::new(),
            loop_stack: Vec::new(),
            functions: Vec::new(),
        };
        generator.functions.push(CompiledFunction {
            name: "<script>".to_string(),
            declared_parameter_count: 0,
            data: BytecodeArrayBuilder::new(1, 0).to_bytecode_array(),
        });

        generator.builder = BytecodeArrayBuilder::new(1, 0);
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

    fn compile_function(&mut self, function: &FunctionLiteral) -> Result<usize> {
        let index = self.functions.len();
        self.functions.push(CompiledFunction {
            name: function.name.clone().unwrap_or_default(),
            declared_parameter_count: function.parameters.len() as i32,
            data: BytecodeArrayBuilder::new(1, 0).to_bytecode_array(),
        });

        let parameter_count = function.parameters.len() as i32 + 1;
        let builder = BytecodeArrayBuilder::new(parameter_count, function.scope.local_count());

        let saved_builder = std::mem::replace(&mut self.builder, builder);
        let saved_loops = std::mem::take(&mut self.loop_stack);
        self.scope_chain.push(function.scope.clone());

        let result = (|| -> Result<()> {
            self.build_function_context(&function.scope)?;

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
            Statement::TryCatch {
                try_block,
                catch_variable,
                catch_block,
                ..
            } => self.visit_try_catch_statement(try_block, catch_variable.as_deref(), catch_block),
            Statement::ForIn {
                each,
                subject,
                body,
                ..
            } => self.visit_for_in_statement(each, subject, body),
            Statement::Throw { exception, .. } => {
                self.visit_for_accumulator_value(exception)?;
                self.builder.emit_throw();
                Ok(())
            }
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

        self.builder.bind_labels(&mut exit_labels);
        self.loop_stack[depth].finish(&mut self.builder);
        self.loop_stack.pop();
        Ok(())
    }

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
                        debug_assert_eq!(level, 0);
                        Resolution::Local(Register::new(variable.index))
                    }
                    VariableLocation::Parameter => {
                        debug_assert_eq!(level, 0);
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

    fn build_function_context(&mut self, scope: &Scope) -> Result<()> {
        if !scope.needs_context() {
            self.context_registers.push(None);
            return Ok(());
        }

        let names = scope.context_local_names();
        self.builder
            .create_function_context(&names, scope.context_slot_count());
        let saved = self.builder.register_allocator().new_register();
        self.builder.push_context(saved);
        self.context_registers.push(Some(saved));

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
            Resolution::Context { depth: 0, slot } => {
                self.builder.load_current_context_slot(slot);
            }
            Resolution::Context { depth, slot } => {
                self.builder.load_context_slot(slot, depth);
            }
            Resolution::Global => {
                self.builder.load_global(name);
            }
        }
    }

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

    fn visit_for_register_value(&mut self, expression: &Expression) -> Result<Register> {
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

    fn visit_for_effect(&mut self, expression: &Expression) -> Result<()> {
        match expression {
            Expression::Literal { .. } | Expression::This { .. } => Ok(()),
            Expression::VariableProxy { name, .. } => match self.resolve(name) {
                Resolution::Global => self.visit_for_accumulator_value(expression),
                _ => Ok(()),
            },
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
            _ => self.visit_for_accumulator_value(expression),
        }
    }

    fn visit_for_test(
        &mut self,
        expression: &Expression,
        then_labels: &mut BytecodeLabels,
        else_labels: &mut BytecodeLabels,
        fallthrough: TestFallthrough,
    ) -> Result<()> {
        match expression {
            Expression::Unary {
                op: Token::Not,
                expression,
                ..
            } => self.visit_for_test(expression, else_labels, then_labels, fallthrough.inverted()),

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
        if let Some((value, oddball)) = compare_against_oddball(op, left, right) {
            self.visit_for_accumulator_value(value)?;
            match op {
                Token::EqStrict => match oddball {
                    Literal::Null => self.builder.compare_null(),
                    Literal::Undefined => self.builder.compare_undefined(),
                    _ => unreachable!(),
                },
                Token::Eq => self.builder.compare_undetectable(),
                other => unreachable!("{other} does not fold against an oddball"),
            };
            return Ok(());
        }

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

    fn visit_logical_operation_for_value(
        &mut self,
        op: Token,
        left: &Expression,
        right: &Expression,
    ) -> Result<()> {
        let mut end_labels = BytecodeLabels::new();
        self.visit_for_accumulator_value(left)?;
        match op {
            Token::And => {
                let label = end_labels.new_label();
                self.builder
                    .jump_if_to_boolean_false(end_labels.label_mut(label));
                self.visit_for_accumulator_value(right)?;
            }
            Token::Or => {
                let label = end_labels.new_label();
                self.builder
                    .jump_if_to_boolean_true(end_labels.label_mut(label));
                self.visit_for_accumulator_value(right)?;
            }
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
            Token::Delete => self.visit_delete(expression),
            other => unreachable!("{other} is not a unary operator"),
        }
    }

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
                    let binary_op = op.binary_op_for_assignment();
                    self.build_variable_load(&name);
                    let old_value = self.builder.register_allocator().new_register();
                    self.builder.store_accumulator_in_register(old_value);
                    self.visit_for_accumulator_value(value)?;
                    self.builder.binary_operation(binary_op, old_value);
                }
                self.build_variable_assignment(&name);
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

    /// `BytecodeGenerator::BuildTryCatch`
    fn visit_try_catch_statement(
        &mut self,
        try_block: &[Statement],
        catch_variable: Option<&str>,
        catch_block: &[Statement],
    ) -> Result<()> {
        use crate::codegen::handler_table::CatchPrediction;
        use crate::interpreter::control_flow_builders::TryCatchBuilder;

        if self.builder.exit_seen_in_block() {
            return Ok(());
        }

        let mark = self.builder.register_allocator().next_register_index();
        let mut try_control_builder =
            TryCatchBuilder::new(&mut self.builder, CatchPrediction::Caught);

        let context = self.builder.register_allocator().new_register();
        self.builder
            .move_register(Register::current_context(), context);

        try_control_builder.begin_try(&mut self.builder, context);
        self.visit_statements(try_block)?;
        try_control_builder.end_try(&mut self.builder);

        if let Some(name) = catch_variable {
            self.build_variable_assignment(name);
        }
        self.visit_statements(catch_block)?;
        try_control_builder.end_catch(&mut self.builder);
        self.builder.register_allocator().release_registers(mark);
        Ok(())
    }

    /// `BytecodeGenerator::VisitForInStatement`
    fn visit_for_in_statement(
        &mut self,
        each: &str,
        subject: &Expression,
        body: &Statement,
    ) -> Result<()> {
        let mark = self.builder.register_allocator().next_register_index();
        let mut subject_undefined = BytecodeLabels::new();

        self.visit_for_accumulator_value(subject)?;
        let label = subject_undefined.new_label();
        self.builder
            .jump_if_undefined_or_null(subject_undefined.label_mut(label));
        let receiver = self.builder.register_allocator().new_register();
        self.builder.store_accumulator_in_register(receiver);

        // Used as kRegTriple and kRegPair in ForInPrepare and ForInNext.
        let triple = self.builder.register_allocator().new_register_list(3);
        let cache_length = triple.at(2);
        self.builder.for_in_enumerate(receiver);
        self.builder.for_in_prepare(triple);

        let index = self.builder.register_allocator().new_register();
        self.builder.load_literal_smi(0);
        self.builder.store_accumulator_in_register(index);

        self.loop_stack.push(LoopBuilder::new());
        let depth = self.loop_stack.len() - 1;
        self.loop_stack[depth].loop_header(&mut self.builder);

        let mut exit_labels = BytecodeLabels::new();
        self.builder.for_in_done(index, cache_length);
        let exit = exit_labels.new_label();
        self.builder.jump_if_true(exit_labels.label_mut(exit));

        self.builder
            .for_in_next(receiver, index, triple.truncate(2));
        let skip = self.loop_stack[depth].continue_labels().new_label();
        let target = self.loop_stack[depth].continue_labels().label_mut(skip);
        self.builder.jump_if_undefined(target);

        self.build_variable_assignment(each);
        self.visit_statement(body)?;

        self.loop_stack[depth].bind_continue_target(&mut self.builder);
        self.builder.for_in_step(index);
        self.loop_stack[depth].jump_to_header(&mut self.builder);

        self.builder.bind_labels(&mut exit_labels);
        self.loop_stack[depth].finish(&mut self.builder);
        self.loop_stack.pop();

        self.builder.bind_labels(&mut subject_undefined);
        self.builder.register_allocator().release_registers(mark);
        Ok(())
    }

    /// `BytecodeGenerator::VisitDelete`
    fn visit_delete(&mut self, expression: &Expression) -> Result<()> {
        match expression {
            // Delete of an object property is allowed both in sloppy
            // and strict modes.
            Expression::Property { object, key, .. } => {
                let mark = self.builder.register_allocator().next_register_index();
                let register = self.visit_for_register_value(object)?;
                match key.as_ref() {
                    PropertyAccess::Named(name) => {
                        let name = name.clone();
                        self.builder.load_literal_string(&name);
                    }
                    PropertyAccess::Keyed(key) => {
                        self.visit_for_accumulator_value(key)?;
                    }
                }
                self.builder.delete(register, false);
                self.builder.register_allocator().release_registers(mark);
                Ok(())
            }
            _ => {
                self.visit_for_effect(expression)?;
                self.builder.load_true();
                Ok(())
            }
        }
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
