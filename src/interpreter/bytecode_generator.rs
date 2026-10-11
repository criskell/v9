use crate::ast::ast::*;
use crate::ast::scopes::Scope;
use crate::common::globals::VariableLocation;
use crate::interpreter::bytecode_array_builder::{BytecodeArrayBuilder, BytecodeArrayData};
use crate::interpreter::bytecode_label::BytecodeLabels;
use crate::interpreter::bytecode_register::Register;
use crate::interpreter::control_flow_builders::{
    ConditionalControlFlowBuilder, LoopBuilder, SwitchBuilder,
};
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

enum Breakable {
    Loop(LoopBuilder),
    Switch(SwitchBuilder),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Resolution {
    Local(Register),
    Parameter(Register),
    Context {
        depth: i32,
        slot: i32,
    },
    Global,
}

struct FinallyScope {
    token: Register,
    result: Register,
    loop_mark: usize,
    saw_return: bool,
    finalization_sites: BytecodeLabels,
}

pub struct BytecodeGenerator {
    builder: BytecodeArrayBuilder,
    scope_chain: Vec<Scope>,
    context_registers: Vec<Option<Register>>,
    loop_stack: Vec<Breakable>,
    finally_stack: Vec<FinallyScope>,
    functions: Vec<CompiledFunction>,
}

impl BytecodeGenerator {
    pub fn generate(program: &Program) -> Result<CompiledProgram> {
        let mut generator = BytecodeGenerator {
            builder: BytecodeArrayBuilder::new(1, 0),
            scope_chain: vec![program.scope.clone()],
            context_registers: Vec::new(),
            loop_stack: Vec::new(),
            finally_stack: Vec::new(),
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
            Statement::TryFinally {
                try_block,
                finally_block,
                ..
            } => self.visit_try_finally_statement(try_block, finally_block),
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
            Statement::Switch { tag, cases, .. } => self.visit_switch_statement(tag, cases),
            Statement::Break { .. } => {
                if self.loop_stack.is_empty() {
                    return Err(GeneratorError {
                        message: "`break` outside a loop".to_string(),
                    });
                }
                self.check_does_not_cross_finally("break")?;
                let depth = self.loop_stack.len() - 1;
                match &mut self.loop_stack[depth] {
                    Breakable::Loop(builder) => builder.emit_break(&mut self.builder),
                    Breakable::Switch(builder) => builder.emit_break(&mut self.builder),
                }
                Ok(())
            }
            Statement::Continue { .. } => {
                let Some(depth) = self.innermost_loop() else {
                    return Err(GeneratorError {
                        message: "`continue` outside a loop".to_string(),
                    });
                };
                self.check_does_not_cross_finally("continue")?;
                Self::loop_at(&mut self.loop_stack, depth).emit_continue(&mut self.builder);
                Ok(())
            }
        }
    }

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

    fn visit_while_statement(&mut self, condition: &Expression, body: &Statement) -> Result<()> {
        self.loop_stack.push(Breakable::Loop(LoopBuilder::new()));
        let depth = self.loop_stack.len() - 1;
        Self::loop_at(&mut self.loop_stack, depth).loop_header(&mut self.builder);

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
        Self::loop_at(&mut self.loop_stack, depth).bind_continue_target(&mut self.builder);
        Self::loop_at(&mut self.loop_stack, depth).jump_to_header(&mut self.builder);

        self.builder.bind_labels(&mut exit_labels);
        Self::loop_at(&mut self.loop_stack, depth).finish(&mut self.builder);
        self.loop_stack.pop();
        Ok(())
    }

    fn visit_do_while_statement(&mut self, body: &Statement, condition: &Expression) -> Result<()> {
        self.loop_stack.push(Breakable::Loop(LoopBuilder::new()));
        let depth = self.loop_stack.len() - 1;
        Self::loop_at(&mut self.loop_stack, depth).loop_header(&mut self.builder);

        self.visit_statement(body)?;
        Self::loop_at(&mut self.loop_stack, depth).bind_continue_target(&mut self.builder);

        let mut exit_labels = BytecodeLabels::new();
        if condition.to_boolean_is_true() == Some(true) {
            Self::loop_at(&mut self.loop_stack, depth).jump_to_header(&mut self.builder);
        } else {
            let mut again_labels = BytecodeLabels::new();
            self.visit_for_test(
                condition,
                &mut again_labels,
                &mut exit_labels,
                TestFallthrough::Then,
            )?;
            self.builder.bind_labels(&mut again_labels);
            Self::loop_at(&mut self.loop_stack, depth).jump_to_header(&mut self.builder);
        }

        self.builder.bind_labels(&mut exit_labels);
        Self::loop_at(&mut self.loop_stack, depth).finish(&mut self.builder);
        self.loop_stack.pop();
        Ok(())
    }

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

        self.loop_stack.push(Breakable::Loop(LoopBuilder::new()));
        let depth = self.loop_stack.len() - 1;
        Self::loop_at(&mut self.loop_stack, depth).loop_header(&mut self.builder);

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

        Self::loop_at(&mut self.loop_stack, depth).bind_continue_target(&mut self.builder);
        if let Some(next) = next {
            self.visit_for_effect(next)?;
        }
        Self::loop_at(&mut self.loop_stack, depth).jump_to_header(&mut self.builder);

        self.builder.bind_labels(&mut exit_labels);
        Self::loop_at(&mut self.loop_stack, depth).finish(&mut self.builder);
        self.loop_stack.pop();
        Ok(())
    }

    fn check_does_not_cross_finally(&self, command: &str) -> Result<()> {
        if let Some(scope) = self.finally_stack.last() {
            if self.loop_stack.len() <= scope.loop_mark {
                return Err(GeneratorError {
                    message: format!("`{command}` across a `finally` is not implemented"),
                });
            }
        }
        Ok(())
    }

    fn visit_switch_statement(&mut self, tag: &Expression, cases: &[CaseClause]) -> Result<()> {
        self.visit_for_accumulator_value(tag)?;

        let number_of_cases = cases.iter().filter(|clause| clause.label.is_some()).count();
        let default_case = cases.iter().position(|clause| clause.label.is_none());

        self.loop_stack
            .push(Breakable::Switch(SwitchBuilder::new(number_of_cases)));
        let depth = self.loop_stack.len() - 1;

        let result = (|| -> Result<()> {
            let mark = self.builder.register_allocator().next_register_index();
            let tag_holder = self.builder.register_allocator().new_register();
            self.builder.store_accumulator_in_register(tag_holder);

            let mut case_compare_ctr = 0;
            for clause in cases {
                let Some(label) = &clause.label else {
                    continue;
                };
                self.visit_for_accumulator_value(label)?;
                self.builder
                    .compare_operation(Token::EqStrict, tag_holder);
                Self::switch_at(&mut self.loop_stack, depth)
                    .jump_to_case_if_true(&mut self.builder, case_compare_ctr);
                case_compare_ctr += 1;
            }
            self.builder.register_allocator().release_registers(mark);

            if default_case.is_some() {
                Self::switch_at(&mut self.loop_stack, depth).jump_to_default(&mut self.builder);
            } else {
                Self::switch_at(&mut self.loop_stack, depth).emit_break(&mut self.builder);
            }

            let mut case_compare_ctr = 0;
            for (index, clause) in cases.iter().enumerate() {
                if Some(index) == default_case {
                    Self::switch_at(&mut self.loop_stack, depth).bind_default(&mut self.builder);
                } else {
                    Self::switch_at(&mut self.loop_stack, depth).bind_case_target_for_compare_jump(
                        &mut self.builder,
                        case_compare_ctr,
                    );
                    case_compare_ctr += 1;
                }
                self.visit_statements(&clause.statements)?;
            }
            Ok(())
        })();

        Self::switch_at(&mut self.loop_stack, depth).finish(&mut self.builder);
        self.loop_stack.pop();
        result
    }

    fn switch_at(stack: &mut [Breakable], depth: usize) -> &mut SwitchBuilder {
        match &mut stack[depth] {
            Breakable::Switch(builder) => builder,
            Breakable::Loop(_) => unreachable!(),
        }
    }

    fn visit_return_statement(&mut self, value: Option<&Expression>) -> Result<()> {
        match value {
            Some(value) => self.visit_for_accumulator_value(value)?,
            None => {
                self.builder.load_undefined();
            }
        }
        self.build_return()
    }

    fn build_return(&mut self) -> Result<()> {
        use crate::interpreter::bytecode_flags_and_tokens::TryFinallyContinuationToken;
        use crate::interpreter::control_flow_builders::TryFinallyBuilder;

        let Some(scope) = self.finally_stack.last_mut() else {
            self.builder.emit_return();
            return Ok(());
        };
        scope.saw_return = true;
        let token = scope.token;
        let result = scope.result;
        self.builder.store_accumulator_in_register(result);
        self.builder
            .load_literal_smi(TryFinallyContinuationToken::K_RETURN_TOKEN);
        self.builder.store_accumulator_in_register(token);
        let depth = self.finally_stack.len() - 1;
        let mut sites = std::mem::take(&mut self.finally_stack[depth].finalization_sites);
        TryFinallyBuilder::leave_try(&mut self.builder, &mut sites);
        self.finally_stack[depth].finalization_sites = sites;
        Ok(())
    }


    fn loop_at(stack: &mut [Breakable], depth: usize) -> &mut LoopBuilder {
        match &mut stack[depth] {
            Breakable::Loop(builder) => builder,
            Breakable::Switch(_) => unreachable!(),
        }
    }

    fn innermost_loop(&self) -> Option<usize> {
        self.loop_stack
            .iter()
            .rposition(|breakable| matches!(breakable, Breakable::Loop(_)))
    }

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

    fn build_this_variable_load(&mut self) {
        match self.resolve("this") {
            Resolution::Global => {
                self.builder
                    .load_accumulator_with_register(Register::receiver());
            }
            _ => self.build_variable_load("this"),
        }
    }

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
                self.build_this_variable_load();
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
            Expression::CallNew {
                callee, arguments, ..
            } => self.visit_call_new(callee, arguments),
            Expression::Call {
                callee, arguments, ..
            } => self.visit_call(callee, arguments),
            Expression::ObjectLiteral { properties, .. } => self.visit_object_literal(properties),
            Expression::ClassLiteral { class, .. } => self.visit_class_literal(class),
            Expression::SuperCallReference { .. } => {
                self.unsupported("`super` outside a call")
            }
            Expression::SuperCallForwardArgs { .. } => self.visit_super_call_forward_args(),
            Expression::SuperPropertyReference { .. } => {
                self.unsupported("`super` outside a property access")
            }
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

    fn visit_property(&mut self, object: &Expression, key: &PropertyAccess) -> Result<()> {
        if let Expression::SuperPropertyReference { is_static, .. } = object {
            let mark = self.builder.register_allocator().next_register_index();
            self.visit_named_super_property_load(*is_static, key, None)?;
            self.builder.register_allocator().release_registers(mark);
            return Ok(());
        }
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

    fn visit_named_super_property_load(
        &mut self,
        is_static: bool,
        key: &PropertyAccess,
        receiver_out: Option<Register>,
    ) -> Result<()> {
        use crate::ast::scopes::{K_HOME_OBJECT, K_STATIC_HOME_OBJECT};
        let PropertyAccess::Named(name) = key else {
            return self.unsupported("a keyed `super` property access");
        };
        let name = name.clone();
        let home_object = if is_static {
            K_STATIC_HOME_OBJECT
        } else {
            K_HOME_OBJECT
        };
        if !self.is_context_slot(home_object) {
            return self.unsupported("`super` outside a method");
        }

        let receiver = self.builder.register_allocator().new_register();
        self.builder
            .load_accumulator_with_register(Register::receiver())
            .store_accumulator_in_register(receiver);
        self.build_variable_load(home_object);
        self.builder.load_named_property_from_super(receiver, &name);
        if let Some(receiver_out) = receiver_out {
            self.builder.move_register(receiver, receiver_out);
        }
        Ok(())
    }

    fn visit_call_new(&mut self, callee: &Expression, arguments: &[Expression]) -> Result<()> {
        let mark = self.builder.register_allocator().next_register_index();
        let constructor = self.visit_for_register_value(callee)?;
        let args = self
            .builder
            .register_allocator()
            .new_register_list(arguments.len() as i32);
        for (index, argument) in arguments.iter().enumerate() {
            self.visit_for_accumulator_value(argument)?;
            self.builder
                .store_accumulator_in_register(args.at(index as i32));
        }
        self.builder.construct(constructor, args);
        self.builder.register_allocator().release_registers(mark);
        Ok(())
    }

    fn visit_call(&mut self, callee: &Expression, arguments: &[Expression]) -> Result<()> {
        let mark = self.builder.register_allocator().next_register_index();

        match callee {
            Expression::SuperCallReference { .. } => {
                self.build_super_call(arguments)?;
            }
            Expression::Property { object, key, .. }
                if matches!(object.as_ref(), Expression::SuperPropertyReference { .. }) =>
            {
                let Expression::SuperPropertyReference { is_static, .. } = object.as_ref() else {
                    unreachable!()
                };
                let callee_register = self.builder.register_allocator().new_register();
                let receiver_holder = self.builder.register_allocator().new_register();
                self.visit_named_super_property_load(*is_static, key, Some(receiver_holder))?;
                self.builder.store_accumulator_in_register(callee_register);

                let mut args = self
                    .builder
                    .register_allocator()
                    .new_growable_register_list();
                let receiver = self
                    .builder
                    .register_allocator()
                    .grow_register_list(&mut args);
                self.builder.move_register(receiver_holder, receiver);
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

    fn visit_super_call_forward_args(&mut self) -> Result<()> {
        let constructor = self.builder.register_allocator().new_register();
        self.builder
            .load_accumulator_with_register(Register::function_closure())
            .get_super_constructor(constructor)
            .construct_forward_all_args(constructor);
        self.builder
            .load_accumulator_with_register(Register::receiver());
        Ok(())
    }

    fn build_super_call(&mut self, arguments: &[Expression]) -> Result<()> {
        let constructor = self.builder.register_allocator().new_register();
        self.builder
            .load_accumulator_with_register(Register::function_closure())
            .get_super_constructor(constructor);

        let mut args = self
            .builder
            .register_allocator()
            .new_growable_register_list();
        let receiver = self
            .builder
            .register_allocator()
            .grow_register_list(&mut args);
        self.builder.move_register(Register::receiver(), receiver);
        for argument in arguments {
            self.visit_for_accumulator_value(argument)?;
            let register = self
                .builder
                .register_allocator()
                .grow_register_list(&mut args);
            self.builder.store_accumulator_in_register(register);
        }
        self.builder.call_property(constructor, args);
        self.builder
            .load_accumulator_with_register(Register::receiver());
        Ok(())
    }

    fn visit_class_literal(&mut self, class: &ClassLiteral) -> Result<()> {
        self.build_class_literal(class)
    }

    fn build_class_literal(&mut self, class: &ClassLiteral) -> Result<()> {
        use crate::objects::literal_objects::{ClassBoilerplate, ValueKind};

        let mut members = Vec::with_capacity(class.public_members.len());
        for property in &class.public_members {
            let PropertyKey::Name(name) = &property.key else {
                unreachable!()
            };
            let value_kind = match property.kind {
                ClassLiteralPropertyKind::Method => ValueKind::ConstantKind,
                ClassLiteralPropertyKind::Getter => ValueKind::GetterKind,
                ClassLiteralPropertyKind::Setter => ValueKind::SetterKind,
            };
            members.push((
                ClassBoilerplate::encode_flags(value_kind, property.is_static),
                name.clone(),
            ));
        }

        self.scope_chain.push(class.scope.clone());
        let result = self.build_class_literal_in_scope(class, &members);
        self.scope_chain.pop();
        let saved = self.context_registers.pop().flatten();
        if let Some(saved) = saved {
            self.builder.pop_context(saved);
        }
        result
    }

    fn build_class_literal_in_scope(
        &mut self,
        class: &ClassLiteral,
        members: &[(i32, String)],
    ) -> Result<()> {
        use crate::ast::scopes::{K_HOME_OBJECT, K_STATIC_HOME_OBJECT};
        use crate::objects::literal_objects::ClassBoilerplate;

        self.build_function_context(&class.scope)?;

        let mark = self.builder.register_allocator().next_register_index();
        let class_constructor = self.builder.register_allocator().new_register();
        let mut args = self.builder.register_allocator().new_growable_register_list();

        let class_boilerplate = self.builder.register_allocator().grow_register_list(&mut args);
        let class_constructor_in_args =
            self.builder.register_allocator().grow_register_list(&mut args);
        let super_class = self.builder.register_allocator().grow_register_list(&mut args);
        debug_assert_eq!(
            ClassBoilerplate::K_FIRST_DYNAMIC_ARGUMENT_INDEX,
            args.register_count()
        );

        match &class.extends {
            Some(extends) => self.visit_for_accumulator_value(extends)?,
            None => {
                self.builder.load_the_hole();
            }
        }
        self.builder.store_accumulator_in_register(super_class);

        let index = self.compile_function(&class.constructor)?;
        self.builder.create_closure(index);
        self.builder
            .store_accumulator_in_register(class_constructor)
            .move_register(class_constructor, class_constructor_in_args)
            .load_class_boilerplate(&members)
            .store_accumulator_in_register(class_boilerplate);

        for property in &class.public_members {
            let value = self.builder.register_allocator().grow_register_list(&mut args);
            let index = self.compile_function(&property.value)?;
            self.builder
                .create_closure(index)
                .store_accumulator_in_register(value);
        }

        self.builder
            .call_runtime(crate::runtime::runtime::FunctionId::DefineClass, args);

        if self.is_context_slot(K_HOME_OBJECT) {
            self.build_variable_assignment(K_HOME_OBJECT);
        }
        if self.is_context_slot(K_STATIC_HOME_OBJECT) {
            self.builder.load_accumulator_with_register(class_constructor);
            self.build_variable_assignment(K_STATIC_HOME_OBJECT);
        }

        self.builder.load_accumulator_with_register(class_constructor);
        self.builder.register_allocator().release_registers(mark);
        Ok(())
    }

    fn is_context_slot(&self, name: &str) -> bool {
        matches!(self.resolve(name), Resolution::Context { .. })
    }

    fn visit_object_literal(&mut self, properties: &[ObjectLiteralProperty]) -> Result<()> {
        let mark = self.builder.register_allocator().next_register_index();
        self.builder.create_empty_object_literal();
        if properties.is_empty() {
            return Ok(());
        }
        let object = self.builder.register_allocator().new_register();
        self.builder.store_accumulator_in_register(object);

        let mut accessor_table: Vec<(String, Option<Expression>, Option<Expression>)> = Vec::new();
        for property in properties {
            let PropertyKey::Name(name) = &property.key else {
                continue;
            };
            if property.kind == ObjectLiteralPropertyKind::Computed {
                continue;
            }
            let entry = match accessor_table.iter().position(|(key, ..)| key == name) {
                Some(index) => &mut accessor_table[index],
                None => {
                    accessor_table.push((name.clone(), None, None));
                    accessor_table.last_mut().expect("the new entry")
                }
            };
            if property.kind == ObjectLiteralPropertyKind::Getter {
                entry.1 = Some(property.value.clone());
            } else {
                entry.2 = Some(property.value.clone());
            }
        }
        let mut defined: Vec<&str> = Vec::new();

        for property in properties {
            match &property.key {
                PropertyKey::Name(name) if property.kind != ObjectLiteralPropertyKind::Computed => {
                    if defined.iter().any(|key| *key == name.as_str()) {
                        continue;
                    }
                    defined.push(name);
                    let (_, getter, setter) = accessor_table
                        .iter()
                        .find(|(key, ..)| key == name)
                        .expect("the accessor table entry");
                    let getter = getter.clone();
                    let setter = setter.clone();

                    let inner = self.builder.register_allocator().next_register_index();
                    let args = self.builder.register_allocator().new_register_list(5);
                    self.builder.move_register(object, args.first_register());
                    self.builder.load_literal_name(name);
                    self.builder.store_accumulator_in_register(Register::new(
                        args.first_register().index() + 1,
                    ));
                    self.visit_literal_accessor(
                        getter.as_ref(),
                        Register::new(args.first_register().index() + 2),
                    )?;
                    self.visit_literal_accessor(
                        setter.as_ref(),
                        Register::new(args.first_register().index() + 3),
                    )?;
                    self.builder
                        .load_literal_smi(crate::objects::property_details::NONE as i32);
                    self.builder.store_accumulator_in_register(Register::new(
                        args.first_register().index() + 4,
                    ));
                    self.builder.call_runtime(
                        crate::runtime::runtime::FunctionId::DefineAccessorPropertyUnchecked,
                        args,
                    );
                    self.builder.register_allocator().release_registers(inner);
                }
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

    fn visit_literal_accessor(
        &mut self,
        property: Option<&Expression>,
        value_out: Register,
    ) -> Result<()> {
        match property {
            Some(property) => {
                self.visit_for_accumulator_value(property)?;
                self.builder.store_accumulator_in_register(value_out);
            }
            None => {
                self.builder.load_null().store_accumulator_in_register(value_out);
            }
        }
        Ok(())
    }

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

        self.loop_stack.push(Breakable::Loop(LoopBuilder::new()));
        let depth = self.loop_stack.len() - 1;
        Self::loop_at(&mut self.loop_stack, depth).loop_header(&mut self.builder);

        let mut exit_labels = BytecodeLabels::new();
        self.builder.for_in_done(index, cache_length);
        let exit = exit_labels.new_label();
        self.builder.jump_if_true(exit_labels.label_mut(exit));

        self.builder
            .for_in_next(receiver, index, triple.truncate(2));
        let skip = Self::loop_at(&mut self.loop_stack, depth).continue_labels().new_label();
        let target = Self::loop_at(&mut self.loop_stack, depth).continue_labels().label_mut(skip);
        self.builder.jump_if_undefined(target);

        self.build_variable_assignment(each);
        self.visit_statement(body)?;

        Self::loop_at(&mut self.loop_stack, depth).bind_continue_target(&mut self.builder);
        self.builder.for_in_step(index);
        Self::loop_at(&mut self.loop_stack, depth).jump_to_header(&mut self.builder);

        self.builder.bind_labels(&mut exit_labels);
        Self::loop_at(&mut self.loop_stack, depth).finish(&mut self.builder);
        self.loop_stack.pop();

        self.builder.bind_labels(&mut subject_undefined);
        self.builder.register_allocator().release_registers(mark);
        Ok(())
    }

    fn visit_try_finally_statement(
        &mut self,
        try_block: &[Statement],
        finally_block: &[Statement],
    ) -> Result<()> {
        use crate::codegen::handler_table::CatchPrediction;
        use crate::interpreter::bytecode_flags_and_tokens::TryFinallyContinuationToken;
        use crate::interpreter::control_flow_builders::TryFinallyBuilder;

        if self.builder.exit_seen_in_block() {
            return Ok(());
        }

        let mark = self.builder.register_allocator().next_register_index();
        let mut try_control_builder =
            TryFinallyBuilder::new(&mut self.builder, CatchPrediction::Uncaught);

        let token = self.builder.register_allocator().new_register();
        let result = self.builder.register_allocator().new_register();
        let context = self.builder.register_allocator().new_register();
        self.builder
            .move_register(Register::current_context(), context);

        self.finally_stack.push(FinallyScope {
            token,
            result,
            loop_mark: self.loop_stack.len(),
            saw_return: false,
            finalization_sites: BytecodeLabels::new(),
        });

        try_control_builder.begin_try(&mut self.builder, context);
        self.visit_statements(try_block)?;
        try_control_builder.end_try(&mut self.builder);

        // Record fall-through and exception cases.
        let fall_through_needed = !self.builder.exit_seen_in_block();
        if fall_through_needed {
            self.builder
                .load_literal_smi(TryFinallyContinuationToken::Fallthrough.value());
            self.builder.store_accumulator_in_register(token);
            self.builder.store_accumulator_in_register(result);
        }
        let depth = self.finally_stack.len() - 1;
        let mut sites = std::mem::take(&mut self.finally_stack[depth].finalization_sites);
        TryFinallyBuilder::leave_try(&mut self.builder, &mut sites);
        try_control_builder.begin_handler(&mut self.builder);

        // The accumulator contains the exception object.
        self.builder.store_accumulator_in_register(result);
        self.builder
            .load_literal_smi(TryFinallyContinuationToken::Rethrow.value());
        self.builder.store_accumulator_in_register(token);

        let mut scope = self.finally_stack.pop().expect("the scope was pushed");
        sites.append(&mut scope.finalization_sites);
        TryFinallyBuilder::begin_finally(&mut self.builder, &mut sites);
        self.visit_statements(finally_block)?;
        try_control_builder.end_finally(&mut self.builder);

        // Dynamic dispatch after the finally-block.
        let mut done = BytecodeLabels::new();
        if scope.saw_return {
            let mut not_return = BytecodeLabels::new();
            self.builder
                .load_literal_smi(TryFinallyContinuationToken::K_RETURN_TOKEN);
            self.builder.compare_operation(Token::EqStrict, token);
            let label = not_return.new_label();
            self.builder.jump_if_false(not_return.label_mut(label));
            self.builder.load_accumulator_with_register(result);
            self.build_return()?;
            self.builder.bind_labels(&mut not_return);
        }
        self.builder
            .load_literal_smi(TryFinallyContinuationToken::Rethrow.value());
        self.builder.compare_operation(Token::EqStrict, token);
        let label = done.new_label();
        self.builder.jump_if_false(done.label_mut(label));
        self.builder.load_accumulator_with_register(result);
        self.builder.emit_rethrow();
        self.builder.bind_labels(&mut done);

        self.builder.register_allocator().release_registers(mark);
        Ok(())
    }

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
