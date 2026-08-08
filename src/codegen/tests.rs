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
    generate_rust_code(&fsms[0]).expect("Should generate successfully")
}

/// Generation is expected to be rejected; returns the validation errors.
fn expect_errors(source: &str) -> Vec<String> {
    let fsms = parse_fsm(source).expect("Should parse successfully");
    generate_rust_code(&fsms[0]).expect_err("Should have been rejected")
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

#[test]
fn test_final_state_target_becomes_enum_variant() {
    // `Working --> [*]` used to emit `self.state = TerminalState::[*];`,
    // which is not valid Rust.
    let code = generate(
        r#"
        fsm Terminal {
            [*] --> Working
            state Working
            Working --> [*] : Done
        }
    "#,
    );

    assert!(
        code.contains("    Final,\n"),
        "final pseudo-state missing from the state enum:\n{code}"
    );
    assert!(
        code.contains("self.state = TerminalState::Final;"),
        "transition to [*] not resolved to the Final variant:\n{code}"
    );
    assert!(
        !code.contains("[*]"),
        "raw [*] leaked into generated code:\n{code}"
    );
}

#[test]
fn test_final_variant_omitted_when_unreachable() {
    // A machine that never terminates should not carry a dead variant.
    let code = generate(
        r#"
        fsm Forever {
            [*] --> Running
            state Running
            state Paused
            Running --> Paused : Pause
            Paused --> Running : Resume
        }
    "#,
    );

    assert!(
        !code.contains("Final"),
        "Final emitted for a machine with no transition to [*]:\n{code}"
    );
}

#[test]
fn test_final_variant_does_not_collide_with_user_state() {
    // A state may legitimately be called `Final`; the synthesised variant has
    // to step aside instead of producing a duplicate.
    let code = generate(
        r#"
        fsm Clash {
            [*] --> Final
            state Final
            state Work
            Final --> Work : Go
            Work --> [*] : Done
        }
    "#,
    );

    assert!(code.contains("    Final,\n"));
    assert!(
        code.contains("    Final_,\n"),
        "synthesised variant collided with the user's state:\n{code}"
    );
    assert!(code.contains("self.state = ClashState::Final_;"));
}

#[test]
fn test_guard_expression_becomes_valid_identifier() {
    // `[attempts > 3]` used to emit `fn attempts_>_3()`, which is not even
    // valid Rust syntax, so the whole generated file failed to parse.
    let code = generate(
        r#"
        fsm DoorLock {
            [*] --> Locked
            state Locked
            state Alarming
            Locked --> Alarming : InvalidCode [attempts > 3]
        }
    "#,
    );

    assert!(
        code.contains("fn attempts_gt_3(&self) -> bool;"),
        "guard expression not sanitised into an identifier:\n{code}"
    );
    assert!(
        !code.contains('>') || !code.contains("fn attempts_>"),
        "raw operator leaked into a method name:\n{code}"
    );
    // The original expression is kept as documentation.
    assert!(code.contains("/// Guard: `attempts > 3`"));
}

#[test]
fn test_guard_is_called_not_field_accessed() {
    // Guards are declared as trait methods, so the match arm has to call them.
    // Emitting `self.context.authorized` was a field access on a type with no
    // such field.
    let code = generate(
        r#"
        fsm DoorLock {
            [*] --> Alarming
            state Alarming
            state Locked
            Alarming --> Locked : AlarmReset [authorized]
        }
    "#,
    );

    assert!(
        code.contains("if self.context.authorized() =>"),
        "guard must be invoked as a method:\n{code}"
    );
}

#[test]
fn test_opposite_operators_do_not_collide() {
    // Stripping operators instead of spelling them out would map both guards
    // to `attempts_3` and silently merge two different conditions into one.
    let code = generate(
        r#"
        fsm Compare {
            [*] --> Idle
            state Idle
            state High
            state Low
            Idle --> High : Check [attempts > 3]
            Idle --> Low : Check [attempts < 3]
        }
    "#,
    );

    assert!(code.contains("fn attempts_gt_3(&self) -> bool;"));
    assert!(code.contains("fn attempts_lt_3(&self) -> bool;"));
}

#[test]
fn test_method_call_syntax_in_guard_is_sanitised() {
    let code = generate(
        r#"
        fsm Submit {
            [*] --> Waiting
            state Waiting
            state Done
            Waiting --> Done : Reply [response.is_success()]
        }
    "#,
    );

    assert!(
        code.contains("fn response_is_success(&self) -> bool;"),
        "dots and parens not sanitised:\n{code}"
    );
}

#[test]
fn test_guarded_transitions_are_emitted_before_unguarded() {
    // Arm order follows the DSL today, so an unguarded transition listed in
    // the middle used to swallow every guarded transition below it: the arm
    // compiled, but `[y]` was unreachable and its guard never ran.
    let code = generate(
        r#"
        fsm GuardOrder {
            [*] --> Idle
            state Idle
            state A
            state B
            Idle --> A : Ev [x]
            Idle --> B : Ev
            Idle --> A : Ev [y]
        }
    "#,
    );

    let x = code.find("self.context.x()").expect("guard x missing");
    let y = code.find("self.context.y()").expect("guard y missing");
    let unguarded = code
        .find("(GuardOrderState::Idle, GuardOrderEvent::Ev) => {")
        .expect("unguarded arm missing");

    assert!(
        x < unguarded && y < unguarded,
        "unguarded arm precedes a guarded one, making it unreachable:\n{code}"
    );
}

#[test]
fn test_wildcard_arm_omitted_when_match_is_exhaustive() {
    // A single state with a single event covers the whole (state, event)
    // space, so a trailing `_ => {}` is dead code and trips
    // `unreachable_patterns` under `-D warnings`.
    let code = generate(
        r#"
        fsm Exhaustive {
            [*] --> Only
            state Only
            Only --> Only : Tick
        }
    "#,
    );

    assert!(
        !code.contains("_ => {}"),
        "dead wildcard arm emitted for an exhaustive match:\n{code}"
    );
}

#[test]
fn test_wildcard_arm_kept_when_pairs_are_uncovered() {
    let code = generate(
        r#"
        fsm Partial {
            [*] --> Idle
            state Idle
            state Busy
            Idle --> Busy : Start
        }
    "#,
    );

    // (Busy, Start) has no arm, so the wildcard is load-bearing.
    assert!(
        code.contains("_ => {}"),
        "wildcard dropped while a (state, event) pair is uncovered:\n{code}"
    );
}

#[test]
fn test_guarded_arm_does_not_count_towards_exhaustiveness() {
    // A guard can evaluate false, so control falls through and the pair is
    // still reachable by the wildcard.
    let code = generate(
        r#"
        fsm OnlyGuarded {
            [*] --> Only
            state Only
            Only --> Only : Tick [ready]
        }
    "#,
    );

    assert!(
        code.contains("_ => {}"),
        "guarded arm wrongly treated as covering its pair:\n{code}"
    );
}

#[test]
fn test_event_enum_emitted_even_with_no_events() {
    // `process()` takes the event type as a parameter, so skipping the enum
    // left a reference to an undefined type.
    let code = generate(
        r#"
        fsm NoEvents {
            [*] --> Idle
            state Idle
            state Done
            Idle --> Done
        }
    "#,
    );

    assert!(
        code.contains("pub enum NoEventsEvent {"),
        "event enum missing while process() still references it:\n{code}"
    );
}

#[test]
fn test_missing_initial_state_is_rejected() {
    // Used to emit `state: YState::Unknown`, a variant that does not exist.
    let errors = expect_errors(
        r#"
        fsm Y {
            state A
            state B
            A --> B : Go
        }
    "#,
    );

    assert!(
        errors.iter().any(|e| e.contains("No initial state")),
        "expected a missing-initial-state error, got {errors:?}"
    );
}

#[test]
fn test_colliding_state_names_are_rejected() {
    let errors = expect_errors(
        r#"
        fsm CaseClash {
            [*] --> idle_state
            state idle_state
            state IdleState
            idle_state --> IdleState : Go
        }
    "#,
    );

    assert!(
        errors.iter().any(|e| e.contains("IdleState")),
        "expected a state variant collision error, got {errors:?}"
    );
}

#[test]
fn test_colliding_action_names_are_rejected() {
    let errors = expect_errors(
        r#"
        fsm ActionClash {
            [*] --> A
            state A {
                entry / doThing()
            }
            state B {
                entry / do_thing()
            }
            A --> B : Go
        }
    "#,
    );

    assert!(
        errors.iter().any(|e| e.contains("do_thing")),
        "expected a trait method collision error, got {errors:?}"
    );
}

#[test]
fn test_valid_fsm_is_not_rejected() {
    // Guard against the validation being over-eager.
    let fsms = parse_fsm(CONNECTION_MANAGER).expect("Should parse");
    assert!(
        generate_rust_code(&fsms[0]).is_ok(),
        "a valid FSM was rejected by validation"
    );
}

#[test]
fn test_generated_code_includes_a_test_module() {
    let code = generate(CONNECTION_MANAGER);

    assert!(
        code.contains("#[cfg(test)]\nmod generated_tests {"),
        "no test module emitted:\n{code}"
    );
    // Gated on cfg(test), so it costs nothing in a release build.
    assert!(code.contains("fn starts_in_initial_state()"));
}

#[test]
fn test_generated_tests_assert_action_order() {
    let code = generate(CONNECTION_MANAGER);

    // Connecting has exit stop_timer; the transition has on_connected; Connected
    // has entry start_timer. The assertion must list them in UML order.
    assert!(
        code.contains(r#"assert_eq!(machine.context().calls, ["stop_timer", "on_connected", "start_timer"]);"#),
        "expected an ordered action assertion:\n{code}"
    );
}

#[test]
fn test_generated_recorder_exposes_one_flag_per_guard() {
    let code = generate(
        r#"
        fsm Guards {
            [*] --> Idle
            state Idle
            state A
            state B
            Idle --> A : Check [ready]
            Idle --> B : Check [attempts > 3]
        }
    "#,
    );

    assert!(code.contains("        ready: bool,"));
    assert!(code.contains("        attempts_gt_3: bool,"));
    // Guards default to true so a guarded transition is exercised.
    assert!(code.contains("                ready: true,"));
}

#[test]
fn test_generated_test_disables_shadowing_guard() {
    // Two guarded arms on the same (state, event): the second is only reachable
    // once the first guard is false, and the generated test must arrange that.
    let code = generate(
        r#"
        fsm Shadow {
            [*] --> Idle
            state Idle
            state A
            state B
            Idle --> A : Check [first]
            Idle --> B : Check [second]
        }
    "#,
    );

    let test = code
        .split("fn idle_on_check_reaches_b()")
        .nth(1)
        .expect("no test generated for the shadowed transition");
    let test = test.split("    }").next().unwrap();

    assert!(
        test.contains("machine.context_mut().first = false;"),
        "shadowing guard not disabled:\n{test}"
    );
}

#[test]
fn test_unreachable_state_is_skipped_with_a_reason() {
    // Nothing leads to Orphan, so no test can drive the machine into it.
    let code = generate(
        r#"
        fsm Unreachable {
            [*] --> Idle
            state Idle
            state Orphan
            state Done
            Idle --> Done : Go
            Orphan --> Done : Go
        }
    "#,
    );

    assert!(
        code.contains("// Skipped: 'Orphan' is not reachable"),
        "expected a skip note explaining why:\n{code}"
    );
}

#[test]
fn test_internal_transition_gets_its_own_test() {
    let code = generate(CONNECTION_MANAGER);

    let test = code
        .split("fn connected_on_keepalive_tick_stays_put()")
        .nth(1)
        .expect("no test generated for the internal transition");
    let test = test.split("\n    }").next().unwrap();

    assert!(test.contains(r#"assert_eq!(machine.context().calls, ["send_keepalive"]);"#));
    // No exit or entry actions should run.
    assert!(!test.contains("stop_timer"));
    assert!(!test.contains("start_timer"));
}

#[test]
fn test_transition_shadowed_by_internal_gets_no_test() {
    // `generate_process_event` drops the external arm here, so a test for it
    // would assert behaviour the machine does not have.
    let code = generate(
        r#"
        fsm Shadowed {
            [*] --> Busy
            state Busy {
                Poke / handle_internally()
            }
            Busy --> Busy : Poke / handle_externally()
        }
    "#,
    );

    assert!(
        code.contains("// Skipped: an internal transition on 'Poke' takes precedence"),
        "expected a skip note for the shadowed transition:\n{code}"
    );
    assert!(
        !code.contains("handle_externally\"]"),
        "a test still asserts the shadowed action:\n{code}"
    );
    // The internal transition is covered instead.
    assert!(code.contains("fn busy_on_poke_stays_put()"));
}

#[test]
fn test_generated_file_documents_how_to_use_it() {
    let code = generate(CONNECTION_MANAGER);

    assert!(code.contains("//! # Usage"));
    // Real names, not generic boilerplate.
    assert!(code.contains("//! impl ConnectionManagerActions for Hardware {"));
    assert!(code.contains("//! let mut machine = ConnectionManager::new(Hardware::new());"));
    assert!(code.contains("//! assert_eq!(machine.state(), ConnectionManagerState::Disconnected);"));
    // `ignore` because Hardware is illustrative; a doctest would fail to compile.
    assert!(code.contains("//! ```ignore"));
}

#[test]
fn test_usage_doc_shows_a_real_transition() {
    let code = generate(CONNECTION_MANAGER);

    assert!(code.contains("//! machine.process(ConnectionManagerEvent::Connect);"));
    assert!(code.contains("//! assert_eq!(machine.state(), ConnectionManagerState::Connecting);"));
}

#[test]
fn test_usage_doc_warns_about_dropped_events() {
    // Worth stating: `process` returns (), so a dropped event is invisible.
    let code = generate(CONNECTION_MANAGER);
    assert!(code.contains("silently ignores an event with no transition"));
}
