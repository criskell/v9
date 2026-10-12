use crate::ast::ast::VariableMode;
use crate::common::globals::VariableLocation;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ScopeKind {
    Script,
    Function,
    Class,
}

#[derive(Clone, Debug)]
pub struct Variable {
    pub name: String,
    pub mode: VariableMode,
    pub location: VariableLocation,
    pub index: i32,
    pub force_context_allocation: bool,
    pub is_parameter: bool,
    pub parameter_index: i32,
}

impl Variable {
    fn new(name: &str, mode: VariableMode) -> Variable {
        Variable {
            name: name.to_string(),
            mode,
            location: VariableLocation::Unallocated,
            index: -1,
            force_context_allocation: false,
            is_parameter: false,
            parameter_index: -1,
        }
    }

    #[inline]
    pub fn is_context_slot(&self) -> bool {
        self.location == VariableLocation::Context
    }

    #[inline]
    pub fn is_unallocated(&self) -> bool {
        self.location == VariableLocation::Unallocated
    }
}

#[derive(Clone, Debug)]
pub struct Scope {
    pub kind: ScopeKind,
    variables: Vec<Variable>,
    unresolved: Vec<String>,
    context_slot_count: i32,
    local_count: i32,
    needs_context: bool,
    allocated: bool,
}

pub const K_HOME_OBJECT: &str = ".home_object";
pub const K_STATIC_HOME_OBJECT: &str = ".static_home_object";

impl Scope {
    pub fn new(kind: ScopeKind) -> Scope {
        Scope {
            kind,
            variables: Vec::new(),
            unresolved: Vec::new(),
            context_slot_count: 0,
            local_count: 0,
            needs_context: false,
            allocated: false,
        }
    }

    pub fn declare(&mut self, name: &str, mode: VariableMode) -> usize {
        if let Some(index) = self.variables.iter().position(|v| v.name == name) {
            return index;
        }
        self.variables.push(Variable::new(name, mode));
        self.variables.len() - 1
    }

    pub fn declare_parameter(&mut self, name: &str, parameter_index: i32) -> usize {
        let index = self.declare(name, VariableMode::Var);
        let variable = &mut self.variables[index];
        variable.is_parameter = true;
        variable.parameter_index = parameter_index;
        index
    }

    pub fn unresolved_names(&self) -> &[String] {
        &self.unresolved
    }

    pub fn record_reference(&mut self, name: &str) {
        if self.lookup_local(name).is_some() {
            return;
        }
        if !self.unresolved.iter().any(|existing| existing == name) {
            self.unresolved.push(name.to_string());
        }
    }

    pub fn lookup_local(&self, name: &str) -> Option<&Variable> {
        self.variables.iter().find(|v| v.name == name)
    }

    fn lookup_local_mut(&mut self, name: &str) -> Option<&mut Variable> {
        self.variables.iter_mut().find(|v| v.name == name)
    }

    pub fn variables(&self) -> &[Variable] {
        &self.variables
    }

    #[inline]
    pub fn needs_context(&self) -> bool {
        debug_assert!(self.allocated);
        self.needs_context
    }

    #[inline]
    pub fn context_slot_count(&self) -> i32 {
        self.context_slot_count
    }

    #[inline]
    pub fn local_count(&self) -> i32 {
        self.local_count
    }

    pub fn context_local_names(&self) -> Vec<String> {
        let mut names: Vec<(i32, String)> = self
            .variables
            .iter()
            .filter(|v| v.is_context_slot())
            .map(|v| (v.index, v.name.clone()))
            .collect();
        names.sort_by_key(|(index, _)| *index);
        names.into_iter().map(|(_, name)| name).collect()
    }

    pub fn context_allocated_parameters(&self) -> Vec<(i32, i32)> {
        self.variables
            .iter()
            .filter(|v| v.is_parameter && v.is_context_slot())
            .map(|v| (v.parameter_index, v.index))
            .collect()
    }
}

#[derive(Debug, Default)]
pub struct ScopeChain {
    scopes: Vec<Scope>,
}

impl ScopeChain {
    pub fn new() -> ScopeChain {
        ScopeChain::default()
    }

    pub fn push(&mut self, kind: ScopeKind) {
        self.scopes.push(Scope::new(kind));
    }

    pub fn push_scope(&mut self, scope: Scope) {
        self.scopes.push(scope);
    }

    pub fn detach(&mut self) -> Scope {
        self.scopes.pop().expect("a scope is open")
    }

    pub fn current(&mut self) -> &mut Scope {
        self.scopes.last_mut().expect("a scope is open")
    }

    pub fn depth(&self) -> usize {
        self.scopes.len()
    }

    pub fn pop(&mut self) -> Scope {
        let mut scope = self.scopes.pop().expect("a scope is open");

        let unresolved = std::mem::take(&mut scope.unresolved);
        for name in unresolved {
            let mut found = false;
            for outer in self.scopes.iter_mut().rev() {
                if let Some(variable) = outer.lookup_local_mut(&name) {
                    variable.force_context_allocation = true;
                    found = true;
                    break;
                }
            }
            if !found {
                if let Some(parent) = self.scopes.last_mut() {
                    parent.record_reference(&name);
                }
            }
        }

        allocate_variables(&mut scope);
        scope
    }

    pub fn pop_root(&mut self) -> Scope {
        debug_assert_eq!(self.scopes.len(), 1);
        self.pop()
    }
}

pub fn allocate_variables(scope: &mut Scope) {
    let mut context_slots = 0;
    let mut locals = 0;

    for variable in scope.variables.iter_mut() {
        if variable.force_context_allocation {
            variable.location = VariableLocation::Context;
            variable.index = crate::objects::contexts::Context::MIN_CONTEXT_SLOTS + context_slots;
            context_slots += 1;
            continue;
        }
        match scope.kind {
            ScopeKind::Script | ScopeKind::Class => {
                variable.location = VariableLocation::Unallocated;
                variable.index = -1;
            }
            ScopeKind::Function => {
                if variable.is_parameter {
                    variable.location = VariableLocation::Parameter;
                    variable.index = variable.parameter_index;
                } else {
                    variable.location = VariableLocation::Local;
                    variable.index = locals;
                    locals += 1;
                }
            }
        }
    }

    scope.context_slot_count = context_slots;
    scope.local_count = locals;
    scope.needs_context = context_slots > 0;
    scope.allocated = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_nothing_captures_stays_in_a_register() {
        let mut chain = ScopeChain::new();
        chain.push(ScopeKind::Function);
        chain.current().declare("n", VariableMode::Var);
        chain.current().record_reference("n");
        let scope = chain.pop();

        let n = scope.lookup_local("n").unwrap();
        assert_eq!(n.location, VariableLocation::Local);
        assert_eq!(n.index, 0);
        assert!(!scope.needs_context(), "no capture, no context");
        assert_eq!(scope.local_count(), 1);
        assert_eq!(scope.context_slot_count(), 0);
    }

    #[test]
    fn a_local_an_inner_function_captures_is_promoted_to_the_context() {
        let mut chain = ScopeChain::new();
        chain.push(ScopeKind::Function); // makeCounter
        chain.current().declare("n", VariableMode::Var);

        chain.push(ScopeKind::Function); // the returned closure
        chain.current().record_reference("n");
        let inner = chain.pop();
        assert!(
            inner.lookup_local("n").is_none(),
            "the inner function does not declare n"
        );

        let outer = chain.pop();
        let n = outer.lookup_local("n").unwrap();
        assert!(n.force_context_allocation, "crossing a function boundary");
        assert_eq!(n.location, VariableLocation::Context);
        assert!(outer.needs_context());
        assert_eq!(outer.context_slot_count(), 1);
        assert_eq!(outer.local_count(), 0, "it is no longer a register");
    }

    #[test]
    fn a_reference_may_precede_its_declaration() {
        let mut chain = ScopeChain::new();
        chain.push(ScopeKind::Function);
        chain.current().record_reference("x");
        chain.current().declare("x", VariableMode::Var);
        let scope = chain.pop();
        assert_eq!(
            scope.lookup_local("x").unwrap().location,
            VariableLocation::Local,
            "the later declaration still claims the earlier reference"
        );
    }

    #[test]
    fn capture_propagates_through_an_intermediate_function() {
        let mut chain = ScopeChain::new();
        chain.push(ScopeKind::Function); // declares a
        chain.current().declare("a", VariableMode::Var);
        chain.push(ScopeKind::Function); // captures nothing itself
        chain.push(ScopeKind::Function); // references a
        chain.current().record_reference("a");

        let innermost = chain.pop();
        assert!(innermost.lookup_local("a").is_none());
        let middle = chain.pop();
        assert!(
            !middle.needs_context(),
            "a scope that captures nothing allocates no context"
        );
        let outer = chain.pop();
        assert_eq!(
            outer.lookup_local("a").unwrap().location,
            VariableLocation::Context
        );
    }

    #[test]
    fn a_captured_parameter_keeps_its_frame_slot_and_gains_a_context_slot() {
        let mut chain = ScopeChain::new();
        chain.push(ScopeKind::Function);
        chain.current().declare_parameter("x", 0);
        chain.push(ScopeKind::Function);
        chain.current().record_reference("x");
        chain.pop();
        let scope = chain.pop();

        let x = scope.lookup_local("x").unwrap();
        assert!(x.is_parameter);
        assert_eq!(x.location, VariableLocation::Context);
        assert_eq!(
            scope.context_allocated_parameters(),
            vec![(0, x.index)],
            "the generator has to copy it in at entry"
        );
    }

    #[test]
    fn an_uncaptured_parameter_is_read_from_the_frame() {
        let mut chain = ScopeChain::new();
        chain.push(ScopeKind::Function);
        chain.current().declare_parameter("x", 0);
        chain.current().record_reference("x");
        let scope = chain.pop();
        let x = scope.lookup_local("x").unwrap();
        assert_eq!(x.location, VariableLocation::Parameter);
        assert_eq!(x.index, 0);
        assert!(scope.context_allocated_parameters().is_empty());
    }

    #[test]
    fn an_undeclared_name_is_a_global() {
        let mut chain = ScopeChain::new();
        chain.push(ScopeKind::Script);
        chain.push(ScopeKind::Function);
        chain.current().record_reference("console");
        chain.pop();
        let script = chain.pop();
        assert!(
            script.lookup_local("console").is_none(),
            "nothing declared it, so it stays a global access"
        );
    }

    #[test]
    fn a_top_level_declaration_is_a_global_not_a_register() {
        let mut chain = ScopeChain::new();
        chain.push(ScopeKind::Script);
        chain.current().declare("x", VariableMode::Var);
        chain.current().record_reference("x");
        let script = chain.pop();
        assert_eq!(
            script.lookup_local("x").unwrap().location,
            VariableLocation::Unallocated,
            "a top-level var is a property of the global object"
        );
        assert_eq!(script.local_count(), 0);
    }

    #[test]
    fn a_captured_top_level_variable_still_goes_in_a_context() {
        let mut chain = ScopeChain::new();
        chain.push(ScopeKind::Script);
        chain.current().declare("x", VariableMode::Var);
        chain.push(ScopeKind::Function);
        chain.current().record_reference("x");
        chain.pop();
        let script = chain.pop();
        assert_eq!(
            script.lookup_local("x").unwrap().location,
            VariableLocation::Context
        );
        assert!(script.needs_context());
    }

    #[test]
    fn context_slots_start_after_the_fixed_header() {
        let mut chain = ScopeChain::new();
        chain.push(ScopeKind::Function);
        chain.current().declare("a", VariableMode::Var);
        chain.current().declare("b", VariableMode::Var);
        chain.push(ScopeKind::Function);
        chain.current().record_reference("a");
        chain.current().record_reference("b");
        chain.pop();
        let scope = chain.pop();

        let min = crate::objects::contexts::Context::MIN_CONTEXT_SLOTS;
        assert_eq!(scope.lookup_local("a").unwrap().index, min);
        assert_eq!(scope.lookup_local("b").unwrap().index, min + 1);
        assert_eq!(scope.context_local_names(), vec!["a", "b"]);
    }

    #[test]
    fn declaring_the_same_var_twice_is_one_binding() {
        let mut chain = ScopeChain::new();
        chain.push(ScopeKind::Function);
        chain.current().declare("x", VariableMode::Var);
        chain.current().declare("x", VariableMode::Var);
        let scope = chain.pop();
        assert_eq!(scope.variables().len(), 1);
        assert_eq!(scope.local_count(), 1);
    }
}
