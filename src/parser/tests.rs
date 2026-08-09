//! Unit tests for the FSM parser

use crate::fsm::StateType;
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

#[test]
fn test_state_declared_twice_is_rejected() {
    // The merge used to overwrite the description and discard the earlier
    // internal transitions, so a duplicate silently lost part of the model.
    let source = r#"
        fsm Probe {
            [*] --> A
            state A: "important" {
                Tick / on_tick()
            }
            state B
            A --> B : Go
            state A { entry / late() }
        }
    "#;

    let error = parse_fsm(source).expect_err("duplicate declaration should be rejected");
    let message = error.to_string();
    assert!(
        message.contains("'A'") && message.contains("more than once"),
        "unexpected error: {message}"
    );
}

#[test]
fn test_implicit_then_explicit_declaration_still_merges() {
    // A transition creates its endpoints implicitly and bare; filling one in
    // afterwards is the legitimate use of the merge and must keep working.
    let source = r#"
        fsm Probe {
            [*] --> A
            A --> B : Go
            state B: "arrived" { entry / on_arrive() }
        }
    "#;

    let fsms = parse_fsm(source).expect("should parse");
    let b = fsms[0].states.iter().find(|s| s.name == "B").expect("B");
    assert_eq!(b.description.as_deref(), Some("arrived"));
    assert_eq!(b.entry_actions.len(), 1);
}

#[test]
fn test_two_fsms_with_the_same_name_are_rejected() {
    // Both would generate a `SameState` enum; written to one module that is a
    // compile error found much later.
    let source = r#"
        fsm Same { [*] --> A state A }
        fsm Same { [*] --> B state B }
    "#;

    let error = parse_fsm(source).expect_err("duplicate fsm name should be rejected");
    assert!(error.to_string().contains("more than once"), "{error}");
}

#[test]
fn test_description_quotes_are_stripped() {
    // The grammar captures up to `{` or a newline, so the quotes came along and
    // ended up inside the generated doc comment: `/// "Stop - wait for green"`.
    let fsms = parse_fsm(
        r#"
        fsm Probe {
            [*] --> Red
            state Red: "Stop - wait for green"
            state Green: Go now
        }
    "#,
    )
    .expect("should parse");

    let red = fsms[0].states.iter().find(|s| s.name == "Red").unwrap();
    assert_eq!(red.description.as_deref(), Some("Stop - wait for green"));

    // An unquoted description is left as it was written.
    let green = fsms[0].states.iter().find(|s| s.name == "Green").unwrap();
    assert_eq!(green.description.as_deref(), Some("Go now"));
}

#[test]
fn test_composite_state_holds_its_own_machine() {
    // A state's body may contain states, its own `[*]` marker and transitions.
    // The point is that a transition on the parent covers every substate:
    // `Running --> Fault : EStop` replaces one line per operational state, and
    // forgetting one of those is a silent safety bug.
    let source = r#"
        fsm Oven {
            [*] --> Idle
            state Idle
            state Fault

            state Running {
                entry / engage_lock()
                exit / disengage_lock()

                [*] --> Heating
                state Heating { entry / heater_on() }
                state Holding
                Heating --> Holding : TempReached
            }

            Idle --> Running : Start
            Running --> Fault : EStop
        }
    "#;

    let fsms = parse_fsm(source).expect("should parse");
    let running = fsms[0]
        .states
        .iter()
        .find(|s| s.name == "Running")
        .expect("Running");

    assert_eq!(running.state_type, StateType::Composite);
    assert_eq!(running.entry_actions.len(), 1);
    assert_eq!(running.exit_actions.len(), 1);

    let sub = running.sub_fsm.as_ref().expect("Running should carry a sub-machine");
    assert_eq!(sub.initial_state.as_deref(), Some("Heating"));
    assert_eq!(sub.states.len(), 2);
    assert_eq!(sub.transitions.len(), 1);

    // The parent's own transitions stay at the level they were written.
    assert!(fsms[0].transitions.iter().any(|t| t.source == "Running" && t.target == "Fault"));
}

#[test]
fn test_nesting_goes_deeper_than_one_level() {
    let fsms = parse_fsm(
        r#"
        fsm Deep {
            [*] --> Outer
            state Outer {
                [*] --> Middle
                state Middle {
                    [*] --> Inner
                    state Inner
                }
            }
        }
    "#,
    )
    .expect("should parse");

    let outer = &fsms[0].states[0];
    let middle = &outer.sub_fsm.as_ref().unwrap().states[0];
    assert_eq!(middle.name, "Middle");
    assert_eq!(middle.state_type, StateType::Composite);
    assert_eq!(
        middle.sub_fsm.as_ref().unwrap().initial_state.as_deref(),
        Some("Inner")
    );
}

#[test]
fn test_a_state_without_nesting_stays_simple() {
    // Entry and exit actions alone must not turn a state composite.
    let fsms = parse_fsm(
        r#"
        fsm Plain {
            [*] --> A
            state A { entry / f() exit / g() Tick / h() }
            state B
            A --> B : Go
        }
    "#,
    )
    .expect("should parse");

    let a = &fsms[0].states.iter().find(|s| s.name == "A").unwrap();
    assert_eq!(a.state_type, StateType::Simple);
    assert!(a.sub_fsm.is_none());
}
