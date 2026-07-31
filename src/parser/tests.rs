//! Unit tests for the FSM parser

use crate::parser::parse_fsm;

#[test]
fn test_parse_simple_fsm() {
    let source = r#"
        fsm Simple {
            [*] --> Idle
            state Idle
            state Running
            Idle --> Running : Start
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    assert_eq!(fsms.len(), 1);

    let fsm = &fsms[0];
    assert_eq!(fsm.name, "Simple");
    assert_eq!(fsm.initial_state, Some("Idle".to_string()));
    assert_eq!(fsm.states.len(), 2);
    // Only external transitions count (initial is a pseudo-transition)
    assert_eq!(fsm.transitions.len(), 1); // Idle->Running
}

#[test]
fn test_parse_state_with_description() {
    let source = r#"
        fsm Test {
            [*] --> Active
            state Active: "The system is active"
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    let fsm = &fsms[0];
    let active = fsm.states.iter().find(|s| s.name == "Active").unwrap();
    assert!(active.description.as_ref().unwrap().contains("active"));
}

#[test]
fn test_parse_state_with_entry_exit() {
    let source = r#"
        fsm Test {
            [*] --> Active
            state Active {
                entry / on_enter()
                exit / on_exit()
            }
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    let fsm = &fsms[0];
    let active = fsm.states.iter().find(|s| s.name == "Active").unwrap();
    
    assert_eq!(active.entry_actions.len(), 1);
    assert_eq!(active.entry_actions[0].name, "on_enter");
    
    assert_eq!(active.exit_actions.len(), 1);
    assert_eq!(active.exit_actions[0].name, "on_exit");
}
#[test]
fn test_parse_state_with_multiple_entry_exit() {
    let source = r#"
        fsm Test {
            [*] --> Active
            state Active {
                entry / on_enter_v1()
                entry / on_enter_v2()
                exit / on_exit_v1()
                exit / on_exit_v2()
            }
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    let fsm = &fsms[0];
    let active = fsm.states.iter().find(|s| s.name == "Active").unwrap();
    
    assert_eq!(active.entry_actions.len(), 2);
    assert_eq!(active.entry_actions[0].name, "on_enter_v1");
    assert_eq!(active.entry_actions[1].name, "on_enter_v2");

    assert_eq!(active.exit_actions.len(), 2);
    assert_eq!(active.exit_actions[0].name, "on_exit_v1");
    assert_eq!(active.exit_actions[1].name, "on_exit_v2");
}


#[test]
fn test_parse_transition_with_event() {
    let source = r#"
        fsm Test {
            [*] --> A
            A --> B : ButtonPress
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    let fsm = &fsms[0];
    
    let transition = fsm.transitions.iter()
        .find(|t| t.source == "A" && t.target == "B")
        .unwrap();
    
    assert!(transition.event.is_some());
    assert_eq!(transition.event.as_ref().unwrap().name, "ButtonPress");
}

#[test]
fn test_parse_transition_with_guard() {
    let source = r#"
        fsm Test {
            [*] --> A
            A --> B : Submit [is_valid]
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    let fsm = &fsms[0];
    
    let transition = fsm.transitions.iter()
        .find(|t| t.source == "A" && t.target == "B")
        .unwrap();
    
    assert!(transition.guard.is_some());
    assert_eq!(transition.guard.as_ref().unwrap().expression, "is_valid");
}

#[test]
fn test_parse_transition_with_action() {
    let source = r#"
        fsm Test {
            [*] --> A
            A --> B : Go / do_something()
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    let fsm = &fsms[0];
    
    let transition = fsm.transitions.iter()
        .find(|t| t.source == "A" && t.target == "B")
        .unwrap();
    
    assert!(transition.action.is_some());
    assert_eq!(transition.action.as_ref().unwrap().name, "do_something");
}

#[test]
fn test_parse_full_transition() {
    let source = r#"
        fsm Test {
            [*] --> A
            A --> B : Submit [is_valid] / process()
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    let fsm = &fsms[0];
    
    let transition = fsm.transitions.iter()
        .find(|t| t.source == "A" && t.target == "B")
        .unwrap();
    
    assert!(transition.event.is_some());
    assert!(transition.guard.is_some());
    assert!(transition.action.is_some());
}

#[test]
fn test_parse_timer() {
    let source = r#"
        fsm Test {
            timer timeout = 5000 -> Expired
            timer heartbeat = 1000 -> Tick periodic
            [*] --> Idle
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    let fsm = &fsms[0];
    
    assert_eq!(fsm.timers.len(), 2);
    
    let timeout = fsm.timers.iter().find(|t| t.name == "timeout").unwrap();
    assert_eq!(timeout.duration_ms, 5000);
    assert_eq!(timeout.event.name, "Expired");
    assert_eq!(timeout.mode, crate::fsm::TimerMode::OneShot);
    
    let heartbeat = fsm.timers.iter().find(|t| t.name == "heartbeat").unwrap();
    assert_eq!(heartbeat.duration_ms, 1000);
    assert_eq!(heartbeat.mode, crate::fsm::TimerMode::Periodic);
}

#[test]
fn test_parse_choice_point() {
    let source = r#"
        fsm Test {
            [*] --> Check
            
            choice Validate {
                [is_ok] -> Success
                [is_warning] -> Warning / log_warning()
                [else] -> Error
            }
            
            Check --> <<Validate>> : Done
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    let fsm = &fsms[0];
    
    assert_eq!(fsm.choice_points.len(), 1);
    let choice = &fsm.choice_points[0];
    assert_eq!(choice.name, "Validate");
    assert_eq!(choice.branches.len(), 3);
}

#[test]
fn test_parse_self_transition() {
    let source = r#"
        fsm Test {
            [*] --> Active
            Active --> Active : Tick / update()
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    let fsm = &fsms[0];
    
    let self_trans = fsm.transitions.iter()
        .find(|t| t.source == "Active" && t.target == "Active")
        .unwrap();
    
    assert!(self_trans.event.is_some());
    assert_eq!(self_trans.event.as_ref().unwrap().name, "Tick");
}

#[test]
fn test_parse_multiple_fsms() {
    let source = r#"
        fsm First {
            [*] --> A
        }
        
        fsm Second {
            [*] --> B
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    assert_eq!(fsms.len(), 2);
    assert_eq!(fsms[0].name, "First");
    assert_eq!(fsms[1].name, "Second");
}

#[test]
fn test_parse_comments() {
    let source = r#"
        // This is a comment
        fsm Test {
            /* Multi-line
               comment */
            [*] --> Idle // Inline comment
            state Idle
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    assert_eq!(fsms.len(), 1);
    assert_eq!(fsms[0].states.len(), 1);
}

#[test]
fn test_parse_error_invalid_syntax() {
    let source = "fsm { }"; // Missing name
    let result = parse_fsm(source);
    assert!(result.is_err());
}

#[test]
fn test_parse_empty_fsm() {
    let source = r#"
        fsm Empty {
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    assert_eq!(fsms.len(), 1);
    assert_eq!(fsms[0].name, "Empty");
    assert!(fsms[0].states.is_empty());
}

#[test]
fn test_implicit_state_creation() {
    let source = r#"
        fsm Test {
            [*] --> A
            A --> B : Go
            B --> C : Next
        }
    "#;

    let fsms = parse_fsm(source).expect("Should parse successfully");
    let fsm = &fsms[0];
    
    // States A, B, C should be created implicitly from transitions
    assert!(fsm.states.iter().any(|s| s.name == "A"));
    assert!(fsm.states.iter().any(|s| s.name == "B"));
    assert!(fsm.states.iter().any(|s| s.name == "C"));
}
