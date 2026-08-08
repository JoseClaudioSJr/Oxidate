//! Code Generator Module
//! 
//! Generates Rust code from FSM definitions.
//! 
//! ## Available Targets
//! 
//! - **Standard** (MIT): Basic Rust FSM with states, events, and transitions
//! 
//! ### Premium Targets (Oxidate Pro)
//! 
//! The following targets are available in Oxidate Pro (available separately):
//! 
//! - **Embassy**: Async embedded with Active Object pattern
//! - **RTIC**: Real-time embedded with event queues
//! 
//! Contact:
//! - Issues: https://github.com/JoseClaudioSJr/Oxidate/issues
//! - Discussions: https://github.com/JoseClaudioSJr/Oxidate/discussions

use crate::fsm::{FsmDefinition, Transition};

#[cfg(test)]
mod tests;

/// Code generation target
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodegenTarget {
    /// Standard Rust (std) - MIT licensed
    Standard,
    /// Embassy async (no_std, embedded) - Premium
    Embassy,
    /// RTIC (no_std, embedded) - Premium
    Rtic,
}

impl CodegenTarget {
    /// Check if target is available (premium features)
    pub fn is_available(&self) -> bool {
        match self {
            CodegenTarget::Standard => true,
            CodegenTarget::Embassy | CodegenTarget::Rtic => false, // Premium
        }
    }
    
    /// Get upgrade message for premium targets
    pub fn upgrade_message(&self) -> Option<&'static str> {
        match self {
            CodegenTarget::Standard => None,
            CodegenTarget::Embassy => Some(
                "Embassy code generation is available in Oxidate Pro.\n\
                 Contact: https://github.com/JoseClaudioSJr/Oxidate/discussions"
            ),
            CodegenTarget::Rtic => Some(
                "RTIC code generation is available in Oxidate Pro.\n\
                 Contact: https://github.com/JoseClaudioSJr/Oxidate/discussions"
            ),
        }
    }
}

impl Default for CodegenTarget {
    fn default() -> Self {
        Self::Standard
    }
}

/// Errors that prevented code generation.
pub type CodegenErrors = Vec<String>;

/// Generate Rust code from an FSM definition.
///
/// The FSM is validated first: generating from a machine with, say, no initial
/// state used to emit a reference to a non-existent enum variant, so a broken
/// definition surfaced as a confusing rustc error in the user's crate instead
/// of a clear message here.
pub fn generate_rust_code(fsm: &FsmDefinition) -> Result<String, CodegenErrors> {
    generate_rust_code_with_target(fsm, CodegenTarget::Standard)
}

/// Generate Rust code with specific target
pub fn generate_rust_code_with_target(
    fsm: &FsmDefinition,
    target: CodegenTarget,
) -> Result<String, CodegenErrors> {
    validate_for_codegen(fsm)?;

    Ok(match target {
        CodegenTarget::Standard => generate_standard_code(fsm),
        CodegenTarget::Embassy => generate_premium_stub(fsm, "Embassy"),
        CodegenTarget::Rtic => generate_premium_stub(fsm, "RTIC"),
    })
}

/// Structural validation plus the naming checks that only the generator can do.
///
/// `FsmDefinition::validate` covers the graph itself (missing initial state,
/// dangling transition endpoints). What it cannot know is that the generator
/// rewrites names into Rust identifiers, and that rewrite is not injective:
/// `idle_state` and `IdleState` both become the variant `IdleState`, which
/// yields an enum that does not compile.
fn validate_for_codegen(fsm: &FsmDefinition) -> Result<(), CodegenErrors> {
    let mut errors = fsm.validate().err().unwrap_or_default();

    collect_collisions(
        fsm.states.iter().map(|s| s.name.as_str()),
        to_pascal_case,
        "states",
        "enum variant",
        &mut errors,
    );

    let mut event_names: Vec<&str> = fsm
        .transitions
        .iter()
        .chain(fsm.states.iter().flat_map(|s| s.internal_transitions.iter()))
        .filter_map(|t| t.event.as_ref().map(|e| e.name.as_str()))
        .collect();
    event_names.sort_unstable();
    event_names.dedup();
    collect_collisions(
        event_names.into_iter(),
        to_pascal_case,
        "events",
        "enum variant",
        &mut errors,
    );

    let mut action_names: Vec<&str> = fsm
        .states
        .iter()
        .flat_map(|s| s.entry_actions.iter().chain(s.exit_actions.iter()))
        .map(|a| a.name.as_str())
        .chain(
            fsm.transitions
                .iter()
                .chain(fsm.states.iter().flat_map(|s| s.internal_transitions.iter()))
                .filter_map(|t| t.action.as_ref().map(|a| a.name.as_str())),
        )
        .collect();
    action_names.sort_unstable();
    action_names.dedup();
    collect_collisions(
        action_names.into_iter(),
        to_snake_case,
        "actions",
        "trait method",
        &mut errors,
    );

    let mut guard_exprs: Vec<&str> = fsm
        .transitions
        .iter()
        .chain(fsm.states.iter().flat_map(|s| s.internal_transitions.iter()))
        .filter_map(|t| t.guard.as_ref().map(|g| g.expression.as_str()))
        .collect();
    guard_exprs.sort_unstable();
    guard_exprs.dedup();
    collect_collisions(
        guard_exprs.into_iter(),
        to_guard_ident,
        "guards",
        "trait method",
        &mut errors,
    );

    // Uniqueness is not enough: the identifier also has to be legal Rust.
    check_identifiers_are_legal(fsm, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Rejects names whose generated identifier is not a valid Rust identifier.
///
/// The collision checks above compare names against each other; this compares
/// each generated identifier against the language. An action called `match`
/// yields `fn match(&mut self)`, and anything called `Self` survives both case
/// conversions — neither compiles.
fn check_identifiers_are_legal(fsm: &FsmDefinition, errors: &mut CodegenErrors) {
    // The struct is `Foo<Ctx: FooActions>`, so an FSM named `Ctx` would shadow
    // its own type parameter.
    if to_pascal_case(&fsm.name) == CONTEXT_TYPE_PARAM {
        errors.push(format!(
            "fsm '{}' collides with the generated type parameter '{CONTEXT_TYPE_PARAM}'",
            fsm.name
        ));
    }

    let mut report = |kind: &str, name: &str, generated: &str, produces: &str| {
        if is_rust_keyword(generated) {
            errors.push(format!(
                "{kind} '{name}' generates the {produces} '{generated}', which is a Rust keyword"
            ));
        }
    };

    for state in &fsm.states {
        report("state", &state.name, &to_pascal_case(&state.name), "enum variant");
    }

    for transition in fsm
        .transitions
        .iter()
        .chain(fsm.states.iter().flat_map(|s| s.internal_transitions.iter()))
    {
        if let Some(event) = &transition.event {
            report("event", &event.name, &to_pascal_case(&event.name), "enum variant");
        }
        if let Some(action) = &transition.action {
            report("action", &action.name, &to_snake_case(&action.name), "trait method");
        }
    }

    for state in &fsm.states {
        for action in state.entry_actions.iter().chain(state.exit_actions.iter()) {
            report("action", &action.name, &to_snake_case(&action.name), "trait method");
        }
    }
}

/// Reports every pair of distinct names that `convert` maps onto one identifier.
fn collect_collisions<'a, I, F>(
    names: I,
    convert: F,
    kind: &str,
    produces: &str,
    errors: &mut CodegenErrors,
) where
    I: Iterator<Item = &'a str>,
    F: Fn(&str) -> String,
{
    let mut seen: Vec<(String, &str)> = Vec::new();
    for name in names {
        let generated = convert(name);
        if let Some((_, first)) = seen.iter().find(|(g, _)| *g == generated) {
            errors.push(format!(
                "{kind} '{first}' and '{name}' both generate the {produces} '{generated}'"
            ));
        } else {
            seen.push((generated, name));
        }
    }
}

/// Generate stub for premium features
fn generate_premium_stub(fsm: &FsmDefinition, target_name: &str) -> String {
    format!(
        "//! {} code generation requires Oxidate Pro\n\
         //!\n\
         //! FSM: {}\n\
         //!\n\
         //! To generate {} code:\n\
         //!   1. Purchase/access: https://github.com/JoseClaudioSJr/Oxidate/discussions\n\
         //!   2. Then use: oxidate-pro generate --target {} your_fsm.fsm\n\
         //!\n\
         //! Oxidate Pro includes:\n\
         //!   - Embassy async Active Object pattern\n\
         //!   - RTIC real-time event queues\n\
         //!   - Events with payload\n\
         //!   - HSM hierarchical states\n\
         //!   - Priority support\n\
         \n\
         compile_error!(\"This target requires Oxidate Pro. Contact: https://github.com/JoseClaudioSJr/Oxidate/discussions\");\n",
        target_name,
        fsm.name,
        target_name,
        target_name.to_lowercase()
    )
}

// ============================================================================
// STANDARD CODE GENERATION
// ============================================================================

fn generate_standard_code(fsm: &FsmDefinition) -> String {
    let mut code = String::new();
    
    // Header
    code.push_str(&format!(
        "// Auto-generated FSM: {}\n",
        fsm.name
    ));
    code.push_str("// Generated by Oxidate\n");
    code.push_str("\n");
    
    // Generate state enum
    code.push_str(&generate_state_enum(fsm));
    code.push_str("\n");
    
    // Generate event enum
    code.push_str(&generate_event_enum(fsm));
    code.push_str("\n");
    
    // Generate FSM struct
    code.push_str(&generate_fsm_struct(fsm));
    code.push_str("\n");
    
    // Generate implementation
    code.push_str(&generate_fsm_impl(fsm));
    code.push_str("\n");
    
    // Generate action trait
    code.push_str(&generate_action_trait(fsm));
    code.push_str("\n");

    // Generated tests: gated on cfg(test), so no cost in a release build.
    code.push_str(&generate_test_module(fsm));
    
    code
}

/// Builds the `//! # Usage` block at the top of the generated file.
///
/// Answers the first question anyone has on opening generated code: how do I
/// actually wire this up? Uses the machine's real method and variant names, so
/// it is copy-pasteable rather than generic boilerplate.
fn generate_usage_doc(fsm: &FsmDefinition) -> String {
    let name = &fsm.name;
    let actions = collect_action_idents(fsm);
    let guards = collect_guard_idents(fsm);

    let Some(initial) = fsm.initial_state.as_ref() else {
        return String::new();
    };

    let mut doc = String::new();
    doc.push_str("/// # Usage\n///\n");
    doc.push_str("/// Implement the actions trait with whatever this machine should drive —\n");
    doc.push_str("/// GPIO, a display, a socket — then feed it events.\n///\n");
    doc.push_str("/// ```ignore\n");
    doc.push_str("/// struct Hardware { /* peripherals, counters, buffers */ }\n///\n");
    doc.push_str(&format!("/// impl {name}Actions for Hardware {{\n"));

    // A couple of real signatures is enough to show the shape; listing every
    // method would bury the interesting part on a large machine.
    for action in actions.iter().take(3) {
        doc.push_str(&format!("///     fn {action}(&mut self) {{ /* ... */ }}\n"));
    }
    if actions.len() > 3 {
        doc.push_str(&format!(
            "///     // ... {} more action{}\n",
            actions.len() - 3,
            if actions.len() - 3 == 1 { "" } else { "s" }
        ));
    }
    for guard in guards.iter().take(2) {
        doc.push_str(&format!("///     fn {guard}(&self) -> bool {{ /* ... */ true }}\n"));
    }
    if guards.len() > 2 {
        doc.push_str(&format!("///     // ... {} more guard{}\n", guards.len() - 2,
            if guards.len() - 2 == 1 { "" } else { "s" }));
    }
    doc.push_str("/// }\n///\n");

    doc.push_str(&format!(
        "/// let mut machine = {name}::new(Hardware::new());\n"
    ));
    doc.push_str(&format!(
        "/// assert_eq!(machine.state(), {name}State::{});\n",
        to_pascal_case(initial)
    ));

    // Show one real event and where it lands, taken from the initial state.
    let sample = fsm
        .transitions
        .iter()
        .find(|t| t.source == *initial && t.event.is_some());
    if let Some(transition) = sample {
        let event = transition.event.as_ref().unwrap();
        let target = if transition.target == "[*]" {
            final_variant_name(fsm)
        } else {
            to_pascal_case(&transition.target)
        };
        doc.push_str("///\n");
        doc.push_str(&format!(
            "/// machine.process({name}Event::{});\n",
            to_pascal_case(&event.name)
        ));
        doc.push_str(&format!(
            "/// assert_eq!(machine.state(), {name}State::{target});\n"
        ));
    }
    doc.push_str("/// ```\n///\n");
    doc.push_str("/// `process` silently ignores an event with no transition from the current\n");
    doc.push_str("/// state, so the caller cannot tell a dropped event from a handled one.\n");

    doc
}

/// Generates a `#[cfg(test)] mod generated_tests` for the machine.
///
/// The point is to catch the mistakes a hand-written FSM makes: a transition
/// that lands in the wrong state, actions running in the wrong order, or a
/// guarded transition that never fires because an earlier arm shadows it.
///
/// Costs nothing in a release build, and runs under plain `cargo test`.
fn generate_test_module(fsm: &FsmDefinition) -> String {
    let mut code = String::new();
    let name = &fsm.name;

    let guards = collect_guard_idents(fsm);
    let actions = collect_action_idents(fsm);

    code.push_str("#[cfg(test)]\nmod generated_tests {\n");
    code.push_str("    //! Generated by Oxidate. Regenerated whenever the FSM changes.\n\n");
    code.push_str("    use super::*;\n\n");

    // ---- recorder -------------------------------------------------------
    code.push_str("    /// Records every action call, and lets each guard be forced.\n");
    code.push_str("    struct Recorder {\n");
    code.push_str("        calls: Vec<&'static str>,\n");
    for guard in &guards {
        code.push_str(&format!("        {guard}: bool,\n"));
    }
    code.push_str("    }\n\n");

    code.push_str("    impl Recorder {\n");
    code.push_str("        /// Guards start true so a guarded transition is taken by default.\n");
    code.push_str("        fn new() -> Self {\n");
    code.push_str("            Self {\n                calls: Vec::new(),\n");
    for guard in &guards {
        code.push_str(&format!("                {guard}: true,\n"));
    }
    code.push_str("            }\n        }\n    }\n\n");

    code.push_str(&format!("    impl {name}Actions for Recorder {{\n"));
    for action in &actions {
        code.push_str(&format!(
            "        fn {action}(&mut self) {{\n            self.calls.push(\"{action}\");\n        }}\n"
        ));
    }
    for guard in &guards {
        code.push_str(&format!(
            "        fn {guard}(&self) -> bool {{\n            self.{guard}\n        }}\n"
        ));
    }
    code.push_str("    }\n\n");

    // ---- initial state --------------------------------------------------
    if let Some(initial) = &fsm.initial_state {
        code.push_str("    #[test]\n    fn starts_in_initial_state() {\n");
        code.push_str(&format!(
            "        let machine = {name}::new(Recorder::new());\n        assert_eq!(machine.state(), {name}State::{});\n",
            to_pascal_case(initial)
        ));
        code.push_str("    }\n\n");
    }

    // ---- one test per reachable transition -------------------------------
    let paths = reachable_paths(fsm);

    for (index, transition) in fsm.transitions.iter().enumerate() {
        let Some(event) = &transition.event else {
            continue;
        };
        if transition.source == "[*]" {
            continue;
        }
        // `generate_process_event` drops an external transition when an
        // unguarded internal transition on the same (state, event) shadows it.
        // Emitting a test for an arm that was never generated makes that test
        // fail against the machine's real behaviour.
        let shadowed_by_internal = fsm
            .states
            .iter()
            .filter(|s| s.name == transition.source)
            .flat_map(|s| s.internal_transitions.iter())
            .any(|t| {
                t.guard.is_none()
                    && t.event.as_ref().map(|e| &e.name) == Some(&event.name)
            });
        if shadowed_by_internal {
            code.push_str(&format!(
                "    // Skipped: an internal transition on '{}' takes precedence, so this\n    // transition is never reached. Transition: {}\n\n",
                event.name,
                transition.label()
            ));
            continue;
        }
        let Some(path) = paths.get(&transition.source) else {
            code.push_str(&format!(
                "    // Skipped: '{}' is not reachable from the initial state, so a test\n    // cannot drive the machine into it. Transition: {}\n\n",
                transition.source,
                transition.label()
            ));
            continue;
        };

        // Earlier transitions on the same (source, event) shadow this one when
        // their guard holds, so force those guards false.
        let shadowing: Vec<String> = fsm.transitions[..index]
            .iter()
            .filter(|t| {
                t.source == transition.source
                    && t.event.as_ref().map(|e| &e.name) == Some(&event.name)
            })
            .filter_map(|t| t.guard.as_ref().map(|g| to_guard_ident(&g.expression)))
            .collect();

        let target = if transition.target == "[*]" {
            final_variant_name(fsm)
        } else {
            to_pascal_case(&transition.target)
        };

        code.push_str("    #[test]\n");
        code.push_str(&format!(
            "    fn {}_on_{}_reaches_{}() {{\n",
            to_snake_case(&transition.source),
            to_snake_case(&event.name),
            to_snake_case(&target)
        ));
        code.push_str(&format!(
            "        let mut machine = {name}::new(Recorder::new());\n"
        ));

        for step in path {
            code.push_str(&format!(
                "        machine.process({name}Event::{});\n",
                to_pascal_case(step)
            ));
        }
        code.push_str(&format!(
            "        assert_eq!(machine.state(), {name}State::{});\n",
            to_pascal_case(&transition.source)
        ));

        for guard in &shadowing {
            code.push_str(&format!(
                "        // An earlier arm on this event would win otherwise.\n        machine.context_mut().{guard} = false;\n"
            ));
        }
        code.push_str("        machine.context_mut().calls.clear();\n\n");

        code.push_str(&format!(
            "        machine.process({name}Event::{});\n\n",
            to_pascal_case(&event.name)
        ));
        code.push_str(&format!(
            "        assert_eq!(machine.state(), {name}State::{target});\n"
        ));

        // Expected call order: exit actions, transition action, entry actions.
        let mut expected: Vec<String> = Vec::new();
        if let Some(source) = fsm.states.iter().find(|s| s.name == transition.source) {
            for action in &source.exit_actions {
                expected.push(to_snake_case(&action.name));
            }
        }
        if let Some(action) = &transition.action {
            expected.push(to_snake_case(&action.name));
        }
        if let Some(dest) = fsm.states.iter().find(|s| s.name == transition.target) {
            for action in &dest.entry_actions {
                expected.push(to_snake_case(&action.name));
            }
        }

        if expected.is_empty() {
            code.push_str("        assert!(machine.context().calls.is_empty());\n");
        } else {
            code.push_str("        // Exit actions, then the transition action, then entry actions.\n");
            code.push_str(&format!(
                "        assert_eq!(machine.context().calls, [{}]);\n",
                expected
                    .iter()
                    .map(|a| format!("\"{a}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        code.push_str("    }\n\n");
    }

    // ---- one test per internal transition ---------------------------------
    for state in &fsm.states {
        for internal in &state.internal_transitions {
            let Some(event) = &internal.event else {
                continue;
            };
            let Some(path) = paths.get(&state.name) else {
                continue;
            };

            code.push_str("    #[test]\n");
            code.push_str(&format!(
                "    fn {}_on_{}_stays_put() {{\n",
                to_snake_case(&state.name),
                to_snake_case(&event.name)
            ));
            code.push_str(&format!(
                "        let mut machine = {name}::new(Recorder::new());\n"
            ));
            for step in path {
                code.push_str(&format!(
                    "        machine.process({name}Event::{});\n",
                    to_pascal_case(step)
                ));
            }
            code.push_str(&format!(
                "        assert_eq!(machine.state(), {name}State::{});\n",
                to_pascal_case(&state.name)
            ));
            code.push_str("        machine.context_mut().calls.clear();\n\n");
            code.push_str(&format!(
                "        machine.process({name}Event::{});\n\n",
                to_pascal_case(&event.name)
            ));
            code.push_str("        // Internal transition: no exit, no entry, no state change.\n");
            code.push_str(&format!(
                "        assert_eq!(machine.state(), {name}State::{});\n",
                to_pascal_case(&state.name)
            ));
            match &internal.action {
                Some(action) => code.push_str(&format!(
                    "        assert_eq!(machine.context().calls, [\"{}\"]);\n",
                    to_snake_case(&action.name)
                )),
                None => code.push_str("        assert!(machine.context().calls.is_empty());\n"),
            }
            code.push_str("    }\n\n");
        }
    }

    code.push_str("}\n");
    code
}

/// Shortest event sequence from the initial state to each reachable state.
///
/// Breadth-first, and only across transitions the recorder would actually take
/// with every guard true: for a given (source, event) the first arm wins, so
/// later arms are not usable as path steps.
fn reachable_paths(fsm: &FsmDefinition) -> std::collections::HashMap<String, Vec<String>> {
    let mut paths: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let Some(initial) = fsm.initial_state.clone() else {
        return paths;
    };
    paths.insert(initial.clone(), Vec::new());

    let mut queue = std::collections::VecDeque::new();
    queue.push_back(initial);

    while let Some(current) = queue.pop_front() {
        let mut seen_events: Vec<String> = Vec::new();
        for transition in &fsm.transitions {
            if transition.source != current || transition.target == "[*]" {
                continue;
            }
            let Some(event) = &transition.event else {
                continue;
            };
            // Only the first arm for this event is reachable by default.
            if seen_events.contains(&event.name) {
                continue;
            }
            seen_events.push(event.name.clone());

            if paths.contains_key(&transition.target) {
                continue;
            }
            let mut path = paths[&current].clone();
            path.push(event.name.clone());
            paths.insert(transition.target.clone(), path);
            queue.push_back(transition.target.clone());
        }
    }

    paths
}

/// Distinct guard identifiers, in a stable order.
fn collect_guard_idents(fsm: &FsmDefinition) -> Vec<String> {
    let mut guards: Vec<String> = fsm
        .transitions
        .iter()
        .chain(fsm.states.iter().flat_map(|s| s.internal_transitions.iter()))
        .filter_map(|t| t.guard.as_ref().map(|g| to_guard_ident(&g.expression)))
        .collect();
    guards.sort();
    guards.dedup();
    guards
}

/// Distinct action identifiers, in a stable order.
fn collect_action_idents(fsm: &FsmDefinition) -> Vec<String> {
    let mut actions: Vec<String> = fsm
        .states
        .iter()
        .flat_map(|s| s.entry_actions.iter().chain(s.exit_actions.iter()))
        .map(|a| to_snake_case(&a.name))
        .chain(
            fsm.transitions
                .iter()
                .chain(fsm.states.iter().flat_map(|s| s.internal_transitions.iter()))
                .filter_map(|t| t.action.as_ref().map(|a| to_snake_case(&a.name))),
        )
        .collect();
    actions.sort();
    actions.dedup();
    actions
}


fn generate_state_enum(fsm: &FsmDefinition) -> String {
    let mut code = String::new();
    
    code.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
    code.push_str(&format!("pub enum {}State {{\n", fsm.name));
    
    for state in &fsm.states {
        if let Some(ref desc) = state.description {
            code.push_str(&format!("    /// {}\n", desc));
        }
        code.push_str(&format!("    {},\n", to_pascal_case(&state.name)));
    }

    if has_final_state(fsm) {
        code.push_str("    /// UML final pseudo-state: the machine has terminated.\n");
        code.push_str(&format!("    {},\n", final_variant_name(fsm)));
    }
    
    code.push_str("}\n");
    code
}

fn generate_event_enum(fsm: &FsmDefinition) -> String {
    let mut code = String::new();
    
    // Collect unique events from external transitions...
    let mut events: Vec<String> = fsm.transitions
        .iter()
        .filter_map(|t| t.event.as_ref().map(|e| e.name.clone()))
        .collect();
    // ...and from internal transitions declared inside state bodies.
    events.extend(
        fsm.states
            .iter()
            .flat_map(|s| s.internal_transitions.iter())
            .filter_map(|t| t.event.as_ref().map(|e| e.name.clone())),
    );
    events.sort();
    events.dedup();

    // Even with no events the enum has to exist: `process()` takes it as a
    // parameter, so returning early left a reference to an undefined type.
    // An empty enum is uninhabited, which correctly says the machine can
    // never be driven by an event.
    
    code.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n");
    code.push_str(&format!("pub enum {}Event {{\n", fsm.name));
    
    for event in &events {
        code.push_str(&format!("    {},\n", to_pascal_case(event)));
    }
    
    code.push_str("}\n");
    code
}

fn generate_fsm_struct(fsm: &FsmDefinition) -> String {
    let mut code = String::new();

    // Attached to the struct rather than emitted as `//!` at the top of the
    // file: an inner doc comment is illegal inside `include!`, which is how
    // build.rs output is normally consumed.
    code.push_str(&generate_usage_doc(fsm));
    
    code.push_str(&format!(
        "pub struct {}<{CONTEXT_TYPE_PARAM}: {}Actions> {{\n",
        fsm.name, fsm.name
    ));
    code.push_str(&format!("    state: {}State,\n", fsm.name));
    code.push_str(&format!("    context: {CONTEXT_TYPE_PARAM},\n"));
    code.push_str("}\n");
    
    code
}

fn generate_fsm_impl(fsm: &FsmDefinition) -> String {
    let mut code = String::new();
    
    let initial_state = fsm.initial_state.as_ref()
        .map(|s| to_pascal_case(s))
        .unwrap_or_else(|| "Unknown".to_string());
    
    code.push_str(&format!(
        "impl<{CONTEXT_TYPE_PARAM}: {}Actions> {}<{CONTEXT_TYPE_PARAM}> {{\n",
        fsm.name, fsm.name
    ));
    
    // Constructor
    code.push_str(&format!("    pub fn new(context: {CONTEXT_TYPE_PARAM}) -> Self {{\n"));
    code.push_str(&format!("        Self {{\n"));
    code.push_str(&format!("            state: {}State::{},\n", fsm.name, initial_state));
    code.push_str("            context,\n");
    code.push_str("        }\n");
    code.push_str("    }\n\n");
    
    // State getter
    code.push_str(&format!("    pub fn state(&self) -> {}State {{\n", fsm.name));
    code.push_str("        self.state\n");
    code.push_str("    }\n\n");
    
    // Context getter
    code.push_str(&format!("    pub fn context(&self) -> &{CONTEXT_TYPE_PARAM} {{\n"));
    code.push_str("        &self.context\n");
    code.push_str("    }\n\n");
    
    // Context mutable getter
    code.push_str(&format!("    pub fn context_mut(&mut self) -> &mut {CONTEXT_TYPE_PARAM} {{\n"));
    code.push_str("        &mut self.context\n");
    code.push_str("    }\n\n");
    
    // Process event
    code.push_str(&generate_process_event(fsm));
    
    code.push_str("}\n");
    code
}

fn generate_process_event(fsm: &FsmDefinition) -> String {
    let mut code = String::new();
    
    code.push_str(&format!("    pub fn process(&mut self, event: {}Event) {{\n", fsm.name));
    code.push_str("        match (self.state, event) {\n");

    // Internal transitions are emitted first: per UML semantics they take
    // precedence over external transitions on the same (state, event) pair.
    // They run their action without leaving the state, so no exit/entry
    // actions and no state change.
    for state in &fsm.states {
        for internal in &state.internal_transitions {
            let Some(ref event) = internal.event else {
                continue;
            };
            let source = to_pascal_case(&state.name);
            let event_name = to_pascal_case(&event.name);

            if let Some(ref guard) = internal.guard {
                code.push_str(&format!(
                    "            ({}State::{}, {}Event::{}) if self.context.{}() => {{\n",
                    fsm.name, source, fsm.name, event_name, to_guard_ident(&guard.expression)
                ));
            } else {
                code.push_str(&format!(
                    "            ({}State::{}, {}Event::{}) => {{\n",
                    fsm.name, source, fsm.name, event_name
                ));
            }

            if let Some(ref action) = internal.action {
                code.push_str(&format!(
                    "                self.context.{}();\n",
                    to_snake_case(&action.name)
                ));
            }

            code.push_str("                // Internal transition: state unchanged\n");
            code.push_str("            }\n");
        }
    }

    // A (state, event) pair already handled by an unguarded internal
    // transition is unreachable below, so skip it to avoid emitting code
    // that trips `unreachable_patterns` in the user's crate.
    let shadowed: Vec<(String, String)> = fsm
        .states
        .iter()
        .flat_map(|s| s.internal_transitions.iter())
        .filter(|t| t.guard.is_none())
        .filter_map(|t| {
            t.event
                .as_ref()
                .map(|e| (t.source.clone(), e.name.clone()))
        })
        .collect();

    // Guarded transitions must be emitted before unguarded ones. A match arm
    // without a guard is a catch-all for its (state, event) pair, so if the
    // DSL happened to list the unguarded transition first, every guarded
    // transition below it became unreachable and its guard silently never ran.
    // Arms for different (state, event) pairs are disjoint, so ordering all
    // guarded arms first is enough — no per-pair grouping needed.
    let ordered: Vec<&Transition> = fsm
        .transitions
        .iter()
        .filter(|t| t.guard.is_some())
        .chain(fsm.transitions.iter().filter(|t| t.guard.is_none()))
        .collect();

    for transition in ordered {
        if transition.source == "[*]" {
            continue; // Skip initial transitions
        }

        if let Some(ref event) = transition.event {
            if shadowed
                .iter()
                .any(|(s, e)| *s == transition.source && *e == event.name)
            {
                continue;
            }
        }
        
        if let Some(ref event) = transition.event {
            let source = to_pascal_case(&transition.source);
            let target = if transition.target == "[*]" {
                final_variant_name(fsm)
            } else {
                to_pascal_case(&transition.target)
            };
            let event_name = to_pascal_case(&event.name);
            
            // Check for guard
            if let Some(ref guard) = transition.guard {
                code.push_str(&format!(
                    "            ({}State::{}, {}Event::{}) if self.context.{}() => {{\n",
                    fsm.name, source, fsm.name, event_name, to_guard_ident(&guard.expression)
                ));
            } else {
                code.push_str(&format!(
                    "            ({}State::{}, {}Event::{}) => {{\n",
                    fsm.name, source, fsm.name, event_name
                ));
            }
            
            // Exit actions
            if let Some(state) = fsm.states.iter().find(|s| s.name == transition.source) {
                for exit_action in &state.exit_actions {
                    code.push_str(&format!(
                        "                self.context.{}();\n",
                        to_snake_case(&exit_action.name)
                    ));
                }
            }
            
            // Transition action
            if let Some(ref action) = transition.action {
                code.push_str(&format!(
                    "                self.context.{}();\n",
                    to_snake_case(&action.name)
                ));
            }
            
            // State change
            code.push_str(&format!(
                "                self.state = {}State::{};\n",
                fsm.name, target
            ));
            
            // Entry actions
            if let Some(state) = fsm.states.iter().find(|s| s.name == transition.target) {
                for entry_action in &state.entry_actions {
                    code.push_str(&format!(
                        "                self.context.{}();\n",
                        to_snake_case(&entry_action.name)
                    ));
                }
            }
            
            code.push_str("            }\n");
        }
    }
    
    // Default case - no transition.
    //
    // Only emitted when something is actually left uncovered. When every
    // (state, event) pair already has an unguarded arm, a trailing wildcard is
    // dead code and makes the generated crate warn on `unreachable_patterns` —
    // which breaks anyone building with `-D warnings`.
    if !is_match_exhaustive(fsm) {
        code.push_str("            _ => {} // No transition\n");
    }
    code.push_str("        }\n");
    code.push_str("    }\n");
    
    code
}

fn generate_action_trait(fsm: &FsmDefinition) -> String {
    let mut code = String::new();
    
    // Collect all actions
    let mut actions: Vec<String> = Vec::new();
    let mut guards: Vec<String> = Vec::new();
    
    for state in &fsm.states {
        for action in &state.entry_actions {
            actions.push(action.name.clone());
        }
        for action in &state.exit_actions {
            actions.push(action.name.clone());
        }
        for internal in &state.internal_transitions {
            if let Some(ref action) = internal.action {
                actions.push(action.name.clone());
            }
            if let Some(ref guard) = internal.guard {
                guards.push(guard.expression.clone());
            }
        }
    }
    
    for transition in &fsm.transitions {
        if let Some(ref action) = transition.action {
            actions.push(action.name.clone());
        }
        if let Some(ref guard) = transition.guard {
            guards.push(guard.expression.clone());
        }
    }
    
    actions.sort();
    actions.dedup();
    guards.sort();
    guards.dedup();
    
    code.push_str(&format!("pub trait {}Actions {{\n", fsm.name));
    
    for action in &actions {
        code.push_str(&format!("    fn {}(&mut self);\n", to_snake_case(action)));
    }
    
    for guard in &guards {
        code.push_str(&format!("    /// Guard: `{}`\n", guard));
        code.push_str(&format!("    fn {}(&self) -> bool;\n", to_guard_ident(guard)));
    }
    
    code.push_str("}\n");
    
    code
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

/// True when every (state, event) pair is covered by an *unguarded* arm.
///
/// Guarded arms don't count: a guard can evaluate false, so control falls
/// through and the pair is still reachable by the wildcard.
fn is_match_exhaustive(fsm: &FsmDefinition) -> bool {
    let mut state_variants: Vec<String> =
        fsm.states.iter().map(|s| to_pascal_case(&s.name)).collect();
    if has_final_state(fsm) {
        // Nothing ever transitions *out* of the final state, so its presence
        // alone means the match cannot be exhaustive.
        state_variants.push(final_variant_name(fsm));
    }

    let mut event_variants: Vec<String> = fsm
        .transitions
        .iter()
        .chain(fsm.states.iter().flat_map(|s| s.internal_transitions.iter()))
        .filter_map(|t| t.event.as_ref().map(|e| to_pascal_case(&e.name)))
        .collect();
    event_variants.sort();
    event_variants.dedup();

    // An empty match is only valid on an uninhabited type; keep the wildcard
    // rather than reason about that corner.
    if state_variants.is_empty() || event_variants.is_empty() {
        return false;
    }

    let mut covered: Vec<(String, String)> = fsm
        .transitions
        .iter()
        .filter(|t| t.guard.is_none() && t.source != "[*]")
        .filter_map(|t| {
            t.event
                .as_ref()
                .map(|e| (to_pascal_case(&t.source), to_pascal_case(&e.name)))
        })
        .chain(
            fsm.states
                .iter()
                .flat_map(|s| s.internal_transitions.iter())
                .filter(|t| t.guard.is_none())
                .filter_map(|t| {
                    t.event
                        .as_ref()
                        .map(|e| (to_pascal_case(&t.source), to_pascal_case(&e.name)))
                }),
        )
        .collect();
    covered.sort();
    covered.dedup();

    covered.len() == state_variants.len() * event_variants.len()
}

/// True when some transition targets the UML final pseudo-state `[*]`.
///
/// The variant is only emitted when it is actually reachable, so machines that
/// never terminate don't carry a dead variant.
fn has_final_state(fsm: &FsmDefinition) -> bool {
    fsm.transitions.iter().any(|t| t.target == "[*]")
}

/// Name of the enum variant standing in for `[*]` as a transition target.
///
/// A user is free to declare a state literally called `Final`, so the name is
/// widened until it stops colliding rather than silently producing a duplicate
/// variant.
fn final_variant_name(fsm: &FsmDefinition) -> String {
    let mut name = "Final".to_string();
    while fsm
        .states
        .iter()
        .any(|s| to_pascal_case(&s.name) == name)
    {
        name.push('_');
    }
    name
}

/// Whether `word` cannot be used as a Rust identifier.
///
/// Covers strict and reserved keywords for the 2015, 2018 and 2021 editions,
/// plus the weak ones — `union` and `'static` are contextual, but a generated
/// method named `union` is confusing enough to be worth rejecting too.
fn is_rust_keyword(word: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        // strict
        "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false",
        "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
        "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
        "unsafe", "use", "where", "while",
        // strict, 2018+
        "async", "await",
        // reserved for future use
        "abstract", "become", "box", "do", "final", "macro", "override", "priv", "typeof",
        "unsized", "virtual", "yield", "try",
        // weak / contextual
        "union",
    ];
    KEYWORDS.contains(&word)
}

/// Name of the generic parameter in the generated struct and impl.
///
/// Deliberately not `T`: an FSM named `T` produced `impl<T: TActions> T<T>`,
/// which does not compile. `Ctx` also reads better at the call site.
const CONTEXT_TYPE_PARAM: &str = "Ctx";

/// Turns a guard expression into a valid Rust identifier for the actions trait.
///
/// A guard is written as a free-form expression in the DSL (`[attempts > 3]`,
/// `[response.is_success()]`), but it is generated as a method on the actions
/// trait, so it has to become a legal identifier. Operators are spelled out
/// rather than dropped, so `[attempts > 3]` and `[attempts < 3]` don't collide.
fn to_guard_ident(expr: &str) -> String {
    // Longest first: `>=` must not be matched as `>`.
    const OPERATORS: &[(&str, &str)] = &[
        (">=", " ge "),
        ("<=", " le "),
        ("==", " eq "),
        ("!=", " ne "),
        ("&&", " and "),
        ("||", " or "),
        (">", " gt "),
        ("<", " lt "),
        ("!", " not "),
    ];

    let mut spelled = expr.to_string();
    for (symbol, word) in OPERATORS {
        spelled = spelled.replace(symbol, word);
    }

    // Everything that is not alphanumeric becomes a separator; `.` and `()`
    // from method-call syntax fall into this bucket too.
    let mut ident = String::new();
    let mut pending_underscore = false;
    for c in spelled.chars() {
        if c.is_alphanumeric() || c == '_' {
            if pending_underscore && !ident.is_empty() {
                ident.push('_');
            }
            pending_underscore = false;
            ident.extend(c.to_lowercase());
        } else {
            pending_underscore = true;
        }
    }

    if ident.is_empty() {
        return "guard".to_string();
    }

    // An identifier may not start with a digit.
    if ident.starts_with(|c: char| c.is_ascii_digit()) {
        ident.insert_str(0, "guard_");
    }

    // Avoid colliding with a Rust keyword.
    if is_rust_keyword(&ident) {
        ident.push('_');
    }

    ident
}

fn to_snake_case(s: &str) -> String {    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_lowercase().next().unwrap_or(c));
    }
    // Replace spaces and special chars
    result.replace(' ', "_").replace('-', "_")
}

// ============================================================================
// PREMIUM TARGETS (Embassy, RTIC)
// ============================================================================
// Available in Oxidate Pro (separately): https://github.com/JoseClaudioSJr/Oxidate/discussions
// 
// Features include:
// - Embassy Active Object pattern (async embedded)
// - RTIC real-time event queues
// - Events with typed payloads
// - HSM hierarchical states
// - Software timers
// - ISR-safe event posting
// ============================================================================