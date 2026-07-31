//! Unit tests for the FSM data structures

use crate::fsm::{FsmDefinition, State, StateType, Transition, Event, Guard, Action};

#[test]
fn test_fsm_definition_new() {
    let fsm = FsmDefinition::new("TestMachine");
    assert_eq!(fsm.name, "TestMachine");
    assert!(fsm.initial_state.is_none());
    assert!(fsm.states.is_empty());
    assert!(fsm.transitions.is_empty());
}

#[test]
fn test_state_new() {
    let state = State::new("Idle", StateType::Simple);
    assert_eq!(state.name, "Idle");
    assert_eq!(state.state_type, StateType::Simple);
    assert!(state.description.is_none());
    assert!(state.entry_actions.is_empty());
    assert!(state.exit_actions.is_empty());
}

#[test]
fn test_transition_label() {
    // Transition with event only
    let t1 = Transition {
        source: "A".to_string(),
        target: "B".to_string(),
        event: Some(Event { name: "Click".to_string() }),
        guard: None,
        action: None,
        kind: crate::fsm::TransitionKind::External,
    };
    assert!(t1.label().contains("Click"));
    
    // Transition with guard
    let t2 = Transition {
        source: "A".to_string(),
        target: "B".to_string(),
        event: Some(Event { name: "Submit".to_string() }),
        guard: Some(Guard { expression: "is_valid".to_string() }),
        action: None,
        kind: crate::fsm::TransitionKind::External,
    };
    assert!(t2.label().contains("Submit"));
    assert!(t2.label().contains("[is_valid]"));
    
    // Transition with action
    let t3 = Transition {
        source: "A".to_string(),
        target: "B".to_string(),
        event: Some(Event { name: "Go".to_string() }),
        guard: None,
        action: Some(Action { name: "do_it".to_string(), params: vec![] }),
        kind: crate::fsm::TransitionKind::External,
    };
    assert!(t3.label().contains("Go"));
    assert!(t3.label().contains("do_it"));
}

#[test]
fn test_fsm_validation_no_initial_state() {
    let fsm = FsmDefinition::new("Test");
    let result = fsm.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("initial state")));
}

#[test]
fn test_fsm_validation_missing_state() {
    let mut fsm = FsmDefinition::new("Test");
    fsm.initial_state = Some("NonExistent".to_string());
    
    let result = fsm.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("NonExistent")));
}

#[test]
fn test_fsm_validation_valid() {
    let mut fsm = FsmDefinition::new("Test");
    fsm.initial_state = Some("Idle".to_string());
    fsm.states.push(State::new("Idle", StateType::Simple));
    fsm.states.push(State::new("Active", StateType::Simple));
    fsm.transitions.push(Transition {
        source: "Idle".to_string(),
        target: "Active".to_string(),
        event: Some(Event { name: "Start".to_string() }),
        guard: None,
        action: None,
        kind: crate::fsm::TransitionKind::External,
    });
    
    let result = fsm.validate();
    assert!(result.is_ok());
}

#[test]
fn test_fsm_collect_events() {
    let mut fsm = FsmDefinition::new("Test");
    fsm.transitions.push(Transition {
        source: "A".to_string(),
        target: "B".to_string(),
        event: Some(Event { name: "Event1".to_string() }),
        guard: None,
        action: None,
        kind: crate::fsm::TransitionKind::External,
    });
    fsm.transitions.push(Transition {
        source: "B".to_string(),
        target: "C".to_string(),
        event: Some(Event { name: "Event2".to_string() }),
        guard: None,
        action: None,
        kind: crate::fsm::TransitionKind::External,
    });
    fsm.transitions.push(Transition {
        source: "C".to_string(),
        target: "A".to_string(),
        event: Some(Event { name: "Event1".to_string() }), // Duplicate
        guard: None,
        action: None,
        kind: crate::fsm::TransitionKind::External,
    });
    
    let events = fsm.collect_events();
    assert_eq!(events.len(), 2); // Should be deduplicated
}
