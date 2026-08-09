//! FSM Data Structures
//! Core types representing Finite State Machines

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

/// A complete FSM definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsmDefinition {
    /// Name of the FSM
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Initial state name
    pub initial_state: Option<String>,
    /// All states in the FSM
    pub states: Vec<State>,
    /// All transitions between states
    pub transitions: Vec<Transition>,
    /// Events that this FSM responds to
    pub events: Vec<Event>,
    /// Choice/Decision points
    pub choice_points: Vec<ChoicePoint>,
    /// Software timers
    pub timers: Vec<Timer>,
}

impl FsmDefinition {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            initial_state: None,
            states: Vec::new(),
            transitions: Vec::new(),
            events: Vec::new(),
            choice_points: Vec::new(),
            timers: Vec::new(),
        }
    }

    /// Get all unique events from transitions
    pub fn collect_events(&self) -> Vec<Event> {
        let mut events: Vec<Event> = self
            .transitions
            .iter()
            .filter_map(|t| t.event.clone())
            .collect();

        // Add internal transition events
        for state in &self.states {
            for internal in &state.internal_transitions {
                if let Some(event) = &internal.event {
                    if !events.iter().any(|e| e.name == event.name) {
                        events.push(event.clone());
                    }
                }
            }
        }

        // A declared timer fires an event, and that event has to be
        // dispatchable even when no transition mentions it yet — otherwise the
        // caller has nothing to pass to `process` when the timer expires.
        for timer in &self.timers {
            if !events.iter().any(|e| e.name == timer.event.name) {
                events.push(timer.event.clone());
            }
        }

        // Deduplicate
        events.sort_by(|a, b| a.name.cmp(&b.name));
        events.dedup_by(|a, b| a.name == b.name);
        events
    }

    /// Validate the FSM definition
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Check for initial state
        if self.initial_state.is_none() {
            errors.push("No initial state defined".to_string());
        }

        // Check that initial state exists
        if let Some(ref initial) = self.initial_state {
            if !self.states.iter().any(|s| &s.name == initial) {
                errors.push(format!("Initial state '{}' not found", initial));
            }
        }

        // Check transition references
        for transition in &self.transitions {
            if transition.source != "[*]"
                && !self.states.iter().any(|s| s.name == transition.source)
            {
                errors.push(format!(
                    "Transition source state '{}' not found",
                    transition.source
                ));
            }
            // `<<Name>>` targets a choice point, which is resolved at
            // generation time rather than being a state of its own.
            let choice_target = transition
                .target
                .strip_prefix("<<")
                .and_then(|t| t.strip_suffix(">>"));
            if let Some(name) = choice_target {
                if !self.choice_points.iter().any(|c| c.name == name) {
                    errors.push(format!("Choice point '{name}' is not defined"));
                }
            } else if transition.target != "[*]"
                && !self.states.iter().any(|s| s.name == transition.target)
            {
                errors.push(format!(
                    "Transition target state '{}' not found",
                    transition.target
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// A state in the FSM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    /// State name (identifier)
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// State type
    pub state_type: StateType,
    /// Entry actions (can have multiple)
    pub entry_actions: Vec<Action>,
    /// Exit actions (can have multiple)
    pub exit_actions: Vec<Action>,
    /// Internal transitions (transitions that don't leave the state)
    pub internal_transitions: Vec<Transition>,
    /// For hierarchical states: nested FSM
    pub sub_fsm: Option<FsmDefinition>,
    /// Visual position in the GUI (x, y)
    pub position: Option<(f32, f32)>,
}

impl State {
    pub fn new(name: impl Into<String>, state_type: StateType) -> Self {
        Self {
            name: name.into(),
            description: None,
            state_type,
            entry_actions: Vec::new(),
            exit_actions: Vec::new(),
            internal_transitions: Vec::new(),
            sub_fsm: None,
            position: None,
        }
    }

}

/// Type of state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateType {
    /// Simple state with no substates
    Simple,
    /// Composite state containing substates
    Composite,
    /// History state (shallow)
    History,
    /// Deep history state
    DeepHistory,
    /// Final state
    Final,
}

/// A transition between states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    /// Source state name
    pub source: String,
    /// Target state name
    pub target: String,
    /// Triggering event (optional for completion transitions)
    pub event: Option<Event>,
    /// Guard condition
    pub guard: Option<Guard>,
    /// Action to execute
    pub action: Option<Action>,
    /// Transition kind
    pub kind: TransitionKind,
}

impl Transition {
    pub fn new(source: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            target: target.into(),
            event: None,
            guard: None,
            action: None,
            kind: TransitionKind::External,
        }
    }

    /// Format transition label for display
    pub fn label(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref event) = self.event {
            parts.push(event.name.clone());
        }

        if let Some(ref guard) = self.guard {
            parts.push(format!("[{}]", guard.expression));
        }

        if let Some(ref action) = self.action {
            parts.push(format!("/ {}", action.name));
        }

        parts.join(" ")
    }
}

/// Kind of transition
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransitionKind {
    /// External transition (exits and re-enters states)
    External,
    /// Internal transition (doesn't exit the state)
    Internal,
    /// Local transition (stays within composite state boundary)
    Local,
}

/// An event that triggers transitions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    /// Event name
    pub name: String,
}

impl Event {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

/// A guard condition for transitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Guard {
    /// Guard expression (will be generated as a function)
    pub expression: String,
}

impl Guard {
    pub fn new(expression: impl Into<String>) -> Self {
        Self {
            expression: expression.into(),
        }
    }
}

/// An action to execute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    /// Action function name
    pub name: String,
    /// Optional parameters
    pub params: Vec<String>,
}

impl Action {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            params: Vec::new(),
        }
    }

}

// ============================================================================
// SOFTWARE TIMERS
// ============================================================================

/// A software timer that can trigger events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timer {
    /// Timer name/identifier
    pub name: String,
    /// Duration in milliseconds
    pub duration_ms: u32,
    /// Event to fire when timer expires
    pub event: Event,
    /// Timer mode
    pub mode: TimerMode,
    /// Optional: start automatically on state entry
    pub auto_start_state: Option<String>,
}

impl Timer {
    pub fn new(name: impl Into<String>, duration_ms: u32, event: Event) -> Self {
        Self {
            name: name.into(),
            duration_ms,
            event,
            mode: TimerMode::OneShot,
            auto_start_state: None,
        }
    }
    
}

/// Timer mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimerMode {
    /// Fire once and stop
    OneShot,
    /// Fire repeatedly until stopped
    Periodic,
}

// ============================================================================
// CHOICE/DECISION POINTS
// ============================================================================

/// A choice point (decision diamond) for conditional branching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoicePoint {
    /// Choice point name/identifier
    pub name: String,
    /// Branches from this choice point
    pub branches: Vec<ChoiceBranch>,
    /// Visual position
    pub position: Option<(f32, f32)>,
}

impl ChoicePoint {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            branches: Vec::new(),
            position: None,
        }
    }
    
}

/// A branch from a choice point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoiceBranch {
    /// Guard condition for this branch
    pub guard: Guard,
    /// Target state
    pub target: String,
    /// Optional action to execute
    pub action: Option<Action>,
}
