//! Unit tests for the Rust code generator.
//!
//! These cover the generator specifically. The parser tests already assert that
//! constructs like internal transitions are read correctly into the AST; what
//! is checked here is that the generator actually *emits* them, which is where
//! issue #1 lived.

use crate::codegen::generate_rust_code;
use crate::parser::parse_fsm;

/// The example from issue #1, trimmed to the parts under test.
const CONNECTION_MANAGER: &str = r#"
    fsm ConnectionManager {
        timer keepalive = 30000 -> KeepaliveTick periodic

        [*] --> Disconnected

        state Disconnected: "No active connection" {
            entry / reset_connection()
        }

        state Connecting: "Establishing connection" {
            entry / initiate_connection()
            entry / start_timer(connect_timeout)
            exit / stop_timer(connect_timeout)
        }

        state Connected: "Connection active" {
            entry / start_timer(keepalive)
            exit / stop_timer(keepalive)
            KeepaliveTick / send_keepalive()
        }

        Disconnected --> Connecting : Connect
        Connecting --> Connected : ConnectionEstablished / on_connected()
        Connected --> Disconnected : Disconnect / graceful_close()
    }
"#;

fn generate(source: &str) -> String {
    let fsms = parse_fsm(source).expect("Should parse successfully");
    generate_rust_code(&fsms[0])
}

#[test]
fn test_all_entry_actions_are_emitted() {
    let code = generate(CONNECTION_MANAGER);

    // Regression: only the last entry action of a state used to survive, so
    // `initiate_connection` silently vanished from both the trait and process().
    assert!(
        code.contains("fn initiate_connection(&mut self);"),
        "first entry action missing from the actions trait:\n{code}"
    );
    assert!(
        code.contains("self.context.initiate_connection();"),
        "first entry action never invoked in process():\n{code}"
    );
    assert!(
        code.contains("fn start_timer(&mut self);"),
        "second entry action missing from the actions trait:\n{code}"
    );
}

#[test]
fn test_internal_transition_event_reaches_event_enum() {
    let code = generate(CONNECTION_MANAGER);

    // KeepaliveTick only ever appears as an internal transition, never as an
    // external one, so it was absent from the generated event enum entirely.
    assert!(
        code.contains("KeepaliveTick,"),
        "internal-only event missing from the event enum:\n{code}"
    );
}

#[test]
fn test_internal_transition_action_reaches_trait() {
    let code = generate(CONNECTION_MANAGER);

    assert!(
        code.contains("fn send_keepalive(&mut self);"),
        "internal transition action missing from the actions trait:\n{code}"
    );
}

#[test]
fn test_internal_transition_does_not_change_state() {
    let code = generate(CONNECTION_MANAGER);

    let arm = code
        .split("(ConnectionManagerState::Connected, ConnectionManagerEvent::KeepaliveTick)")
        .nth(1)
        .expect("no match arm generated for the internal transition");
    // Take just this arm, up to the start of the next one.
    let arm = arm.split("\n            (").next().unwrap();

    assert!(
        arm.contains("self.context.send_keepalive();"),
        "internal transition does not run its action:\n{arm}"
    );
    assert!(
        !arm.contains("self.state ="),
        "internal transition must not reassign the state:\n{arm}"
    );
    assert!(
        !arm.contains("stop_timer"),
        "internal transition must not run exit actions:\n{arm}"
    );
}

#[test]
fn test_external_transition_runs_exit_then_action_then_entry() {
    let code = generate(CONNECTION_MANAGER);

    let arm = code
        .split("(ConnectionManagerState::Connected, ConnectionManagerEvent::Disconnect)")
        .nth(1)
        .expect("no match arm generated for the external transition");
    let arm = arm.split("\n            (").next().unwrap();

    let exit = arm.find("stop_timer").expect("exit action missing");
    let action = arm.find("graceful_close").expect("transition action missing");
    let entry = arm.find("reset_connection").expect("entry action missing");

    assert!(
        exit < action && action < entry,
        "UML ordering broken (exit -> action -> entry):\n{arm}"
    );
}

#[test]
fn test_unguarded_internal_transition_shadows_external_on_same_event() {
    // An unguarded internal transition wins over an external one on the same
    // (state, event) pair. The external arm must be dropped rather than emitted
    // dead, otherwise the generated crate warns on `unreachable_patterns`.
    let code = generate(
        r#"
        fsm Conflict {
            [*] --> Idle
            state Idle {
                Poke / handle_internally()
            }
            state Other
            Idle --> Other : Poke / handle_externally()
        }
    "#,
    );

    assert!(code.contains("self.context.handle_internally();"));
    assert_eq!(
        code.matches("(ConflictState::Idle, ConflictEvent::Poke)").count(),
        1,
        "duplicate match arm emitted for the same (state, event) pair:\n{code}"
    );
}

#[test]
fn test_guarded_internal_transition_leaves_external_reachable() {
    // With a guard the internal arm can fall through, so the external
    // transition is still reachable and must be kept.
    let code = generate(
        r#"
        fsm Guarded {
            [*] --> Idle
            state Idle {
                Poke [ready] / handle_internally()
            }
            state Other
            Idle --> Other : Poke / handle_externally()
        }
    "#,
    );

    assert_eq!(
        code.matches("(GuardedState::Idle, GuardedEvent::Poke)").count(),
        2,
        "guarded internal transition should not suppress the external arm:\n{code}"
    );
    assert!(code.contains("fn ready(&self) -> bool;"));
    assert!(code.contains("self.context.handle_externally();"));
}

#[test]
fn test_trait_methods_are_deduplicated() {
    let code = generate(CONNECTION_MANAGER);

    // start_timer appears as an entry action in two different states.
    assert_eq!(
        code.matches("fn start_timer(&mut self);").count(),
        1,
        "actions trait declares the same method twice:\n{code}"
    );
}
