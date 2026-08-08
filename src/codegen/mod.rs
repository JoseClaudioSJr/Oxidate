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

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
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
        "//! Auto-generated FSM: {}\n",
        fsm.name
    ));
    code.push_str("//! Generated by Oxidate\n\n");
    
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
    
    code
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
    
    code.push_str(&format!("pub struct {}<T: {}Actions> {{\n", fsm.name, fsm.name));
    code.push_str(&format!("    state: {}State,\n", fsm.name));
    code.push_str("    context: T,\n");
    code.push_str("}\n");
    
    code
}

fn generate_fsm_impl(fsm: &FsmDefinition) -> String {
    let mut code = String::new();
    
    let initial_state = fsm.initial_state.as_ref()
        .map(|s| to_pascal_case(s))
        .unwrap_or_else(|| "Unknown".to_string());
    
    code.push_str(&format!("impl<T: {}Actions> {}<T> {{\n", fsm.name, fsm.name));
    
    // Constructor
    code.push_str("    pub fn new(context: T) -> Self {\n");
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
    code.push_str("    pub fn context(&self) -> &T {\n");
    code.push_str("        &self.context\n");
    code.push_str("    }\n\n");
    
    // Context mutable getter
    code.push_str("    pub fn context_mut(&mut self) -> &mut T {\n");
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
    const KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
        "where", "while", "async", "await", "dyn",
    ];
    if KEYWORDS.contains(&ident.as_str()) {
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