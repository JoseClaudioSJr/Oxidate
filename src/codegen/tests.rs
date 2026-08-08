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
    // Takes the timer name: `start_timer(connect_timeout)` and
    // `start_timer(keepalive)` must be distinguishable by the implementer.
    assert!(
        code.contains("fn start_timer(&mut self, arg1: &str);"),
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

    // start_timer appears as an entry action in two different states, with a
    // different argument each time. One method, called twice.
    assert_eq!(
        code.matches("fn start_timer(&mut self, arg1: &str);").count(),
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

    assert!(code.contains("/// # Usage"));
    // Real names, not generic boilerplate.
    assert!(code.contains("/// impl ConnectionManagerActions for Hardware {"));
    assert!(code.contains("/// let mut machine = ConnectionManager::new(Hardware::new());"));
    assert!(code.contains("/// assert_eq!(machine.state(), ConnectionManagerState::Disconnected);"));
    // `ignore` because Hardware is illustrative; a doctest would fail to compile.
    assert!(code.contains("/// ```ignore"));
}

#[test]
fn test_usage_doc_shows_a_real_transition() {
    let code = generate(CONNECTION_MANAGER);

    assert!(code.contains("/// machine.process(ConnectionManagerEvent::Connect);"));
    assert!(code.contains("/// assert_eq!(machine.state(), ConnectionManagerState::Connecting);"));
}

#[test]
fn test_usage_doc_warns_about_dropped_events() {
    // Worth stating: `process` returns (), so a dropped event is invisible.
    let code = generate(CONNECTION_MANAGER);
    assert!(code.contains("silently ignores an event with no transition"));
}

#[test]
fn test_action_named_after_a_keyword_is_rejected() {
    // Produced `fn match(&mut self);`, which is not valid Rust.
    let errors = expect_errors(
        r#"
        fsm Probe {
            [*] --> A
            state A
            state B
            A --> B : Go / match()
        }
    "#,
    );

    assert!(
        errors.iter().any(|e| e.contains("'match'") && e.contains("keyword")),
        "expected a keyword error, got {errors:?}"
    );
}

#[test]
fn test_self_is_rejected_everywhere_it_appears() {
    // `Self` survives both case conversions: to_pascal_case leaves it alone and
    // to_snake_case yields `self`. Both are reserved.
    for source in [
        r#"fsm Probe { [*] --> A state A state Self A --> Self : Go }"#,
        r#"fsm Probe { [*] --> A state A state B A --> B : Self }"#,
    ] {
        let errors = expect_errors(source);
        assert!(
            errors.iter().any(|e| e.contains("Self") && e.contains("keyword")),
            "expected Self to be rejected, got {errors:?}"
        );
    }
}

#[test]
fn test_fsm_named_after_the_type_parameter_is_rejected() {
    // `pub struct Ctx<Ctx: CtxActions>` shadows its own parameter.
    let errors = expect_errors(
        r#"
        fsm Ctx {
            [*] --> A
            state A
            state B
            A --> B : Go
        }
    "#,
    );

    assert!(
        errors.iter().any(|e| e.contains("type parameter")),
        "expected a type parameter collision, got {errors:?}"
    );
}

#[test]
fn test_single_letter_fsm_name_still_works() {
    // `fsm T` used to emit `impl<T: TActions> T<T>`. Renaming the generated
    // parameter to `Ctx` fixed it; this guards the fix.
    let code = generate(
        r#"
        fsm T {
            [*] --> A
            state A
            state B
            A --> B : Go
        }
    "#,
    );

    assert!(code.contains("pub struct T<Ctx: TActions> {"));
    assert!(code.contains("impl<Ctx: TActions> T<Ctx> {"));
}

#[test]
fn test_ordinary_names_are_not_rejected() {
    // The keyword check must not be over-eager: `Match` and `Type` are fine as
    // enum variants, since to_pascal_case capitalises them.
    let fsms = parse_fsm(
        r#"
        fsm Probe {
            [*] --> Idle
            state Idle
            state Match
            state Type
            Idle --> Match : Go / do_match()
            Match --> Type : Next
        }
    "#,
    )
    .expect("Should parse");

    assert!(
        generate_rust_code(&fsms[0]).is_ok(),
        "valid names were rejected"
    );
}

#[test]
fn test_generated_code_has_no_inner_doc_comments() {
    // An inner doc comment is illegal inside `include!`, which is how build.rs
    // output is normally consumed. The only one allowed is inside the generated
    // test module, which is a module of its own.
    let code = generate(CONNECTION_MANAGER);

    let offenders: Vec<&str> = code
        .lines()
        .filter(|line| line.trim_start().starts_with("//!"))
        .filter(|line| !line.contains("Regenerated whenever the FSM changes"))
        .collect();

    assert!(
        offenders.is_empty(),
        "inner doc comments outside the test module: {offenders:?}"
    );
}

#[test]
fn test_usage_doc_is_attached_to_the_struct() {
    let code = generate(CONNECTION_MANAGER);

    let usage = code.find("/// # Usage").expect("usage doc missing");
    let decl = code
        .find("pub struct ConnectionManager<")
        .expect("struct missing");

    assert!(usage < decl, "usage doc must precede the struct it documents");
    // Nothing but doc lines between them.
    assert!(
        code[usage..decl]
            .lines()
            .all(|l| l.trim().is_empty() || l.trim_start().starts_with("///")),
        "usage doc is detached from the struct"
    );
}

/// An internal transition and an external self-transition on the same event.
/// UML gives the internal one precedence, so the external one never runs.
const SHADOWED: &str = r#"
    fsm VendingMachine {
        [*] --> Idle
        state Idle { entry / display_welcome() }
        state AcceptingCoins {
            entry / show_balance()
            CoinInserted / add_to_balance()
        }
        Idle --> AcceptingCoins : CoinInserted
        AcceptingCoins --> AcceptingCoins : CoinInserted / add_coin()
        AcceptingCoins --> Idle : Cancel / return_coins()
    }
"#;

#[test]
fn test_shadowed_action_is_not_required_by_the_trait() {
    // `add_coin` belongs to the transition the internal one takes precedence
    // over. Declaring it forced the implementer to write a dead method.
    let code = generate(SHADOWED);

    let trait_block = code
        .split("pub trait VendingMachineActions {")
        .nth(1)
        .expect("trait missing");
    let trait_block = trait_block.split('}').next().unwrap();

    assert!(
        !trait_block.contains("add_coin"),
        "trait still requires an action that can never run:\n{trait_block}"
    );
    // The internal transition's own action is still needed.
    assert!(trait_block.contains("fn add_to_balance(&mut self);"));
    assert!(trait_block.contains("fn return_coins(&mut self);"));
}

#[test]
fn test_shadowed_transition_is_reported_in_the_output() {
    // Dropping it silently leaves the author wondering why nothing happens.
    let code = generate(SHADOWED);

    assert!(
        code.contains("// Note: 'AcceptingCoins --> AcceptingCoins")
            && code.contains("can never run"),
        "no note explaining the dropped transition:\n{code}"
    );
}

#[test]
fn test_shadowed_guard_is_not_required_either() {
    let code = generate(
        r#"
        fsm Probe {
            [*] --> Busy
            state Busy {
                Poke / handle_internally()
            }
            Busy --> Busy : Poke [ready] / handle_externally()
        }
    "#,
    );

    // The external arm is guarded, so it is *not* shadowed: a guarded internal
    // transition would be needed for that. Here the internal one is unguarded,
    // so it does take precedence and both the action and the guard go away.
    assert!(!code.contains("fn handle_externally(&mut self);"));
    assert!(!code.contains("fn ready(&self) -> bool;"));
}

#[test]
fn test_a_guarded_internal_transition_does_not_shadow() {
    // It can evaluate false, leaving the external transition reachable, so its
    // action must stay in the trait.
    let code = generate(
        r#"
        fsm Probe {
            [*] --> Busy
            state Busy {
                Poke [maybe] / handle_internally()
            }
            state Done
            Busy --> Done : Poke / handle_externally()
        }
    "#,
    );

    assert!(code.contains("fn handle_externally(&mut self);"));
    assert!(!code.contains("// Note:"), "nothing should be reported as dropped");
}

#[test]
fn test_unreachable_state_is_reported_in_the_output() {
    // The machine can never be in it, so every arm the user writes for it is
    // dead. Usually a modelling slip rather than intent.
    let code = generate(
        r#"
        fsm Orphan {
            [*] --> A
            state A
            state B
            state Ghost
            A --> B : Go
            B --> A : Back
        }
    "#,
    );

    assert!(
        code.contains("// Warning: state 'Ghost' is unreachable"),
        "no warning for the unreachable state:\n{code}"
    );
}

#[test]
fn test_trap_state_is_reported_in_the_output() {
    let code = generate(
        r#"
        fsm Trap {
            [*] --> A
            state A
            state Stuck
            A --> Stuck : Go
        }
    "#,
    );

    assert!(
        code.contains("// Warning: state 'Stuck' has no way out"),
        "no warning for the trap state:\n{code}"
    );
}

#[test]
fn test_a_state_routed_to_final_is_not_a_trap() {
    // Terminating deliberately is not the same as forgetting a way out.
    let code = generate(
        r#"
        fsm Fine {
            [*] --> A
            state A
            state B
            A --> B : Go
            B --> [*] : Done
        }
    "#,
    );

    assert!(!code.contains("// Warning:"), "sound machine warned about:\n{code}");
}

#[test]
fn test_a_sound_machine_produces_no_warnings() {
    let code = generate(CONNECTION_MANAGER);
    assert!(!code.contains("// Warning:"), "unexpected warnings:\n{code}");
}

#[test]
fn test_action_parameters_reach_the_trait_and_the_call_site() {
    // Parameters used to parse and then be discarded, so `start_timer(keepalive)`
    // and `start_timer(watchdog)` collapsed into one parameterless method and the
    // implementer could not tell which timer to start.
    let code = generate(
        r#"
        fsm Probe {
            [*] --> Idle
            state Idle
            state Waiting { entry / start_timer(response_timeout) }
            state Connected { entry / start_timer(keepalive) }
            state Failed
            Idle --> Waiting : Start
            Waiting --> Connected : Ok
            Connected --> Failed : Reset / reset_with_code(0)
        }
    "#,
    );

    assert!(code.contains("fn start_timer(&mut self, arg1: &str);"));
    assert!(code.contains(r#"self.context.start_timer("response_timeout");"#));
    assert!(code.contains(r#"self.context.start_timer("keepalive");"#));

    // A whole number stays a number.
    assert!(code.contains("fn reset_with_code(&mut self, arg1: i64);"));
    assert!(code.contains("self.context.reset_with_code(0);"));
}

#[test]
fn test_string_literal_parameters_are_accepted() {
    // Documented in DSL_REFERENCE.md and previously rejected by the grammar.
    let code = generate(
        r#"
        fsm Probe {
            [*] --> Idle
            state Idle
            state Logging { entry / log_message("Entered logging state") }
            Idle --> Logging : Go
        }
    "#,
    );

    assert!(code.contains("fn log_message(&mut self, arg1: &str);"));
    assert!(code.contains(r#"self.context.log_message("Entered logging state");"#));
}

#[test]
fn test_negative_numbers_are_accepted() {
    let code = generate(
        r#"
        fsm Probe {
            [*] --> A
            state A
            state B
            A --> B : Go / set_offset(-1)
        }
    "#,
    );

    assert!(code.contains("fn set_offset(&mut self, arg1: i64);"));
    assert!(code.contains("self.context.set_offset(-1);"));
}

#[test]
fn test_inconsistent_arity_is_rejected() {
    // One trait method cannot take a different number of arguments per call.
    let errors = expect_errors(
        r#"
        fsm Probe {
            [*] --> A
            state A { entry / log(one) }
            state B { entry / log(one, two) }
            A --> B : Go
        }
    "#,
    );

    assert!(
        errors.iter().any(|e| e.contains("argument")),
        "expected an arity error, got {errors:?}"
    );
}

#[test]
fn test_a_position_used_with_both_kinds_becomes_a_string() {
    // A single non-integer use decides the position: `&str` accepts both.
    let code = generate(
        r#"
        fsm Probe {
            [*] --> Idle
            state Idle
            state A { entry / mark(1) }
            state B { entry / mark(start) }
            Idle --> A : Go
            A --> B : Next
        }
    "#,
    );

    assert!(code.contains("fn mark(&mut self, arg1: &str);"));
    assert!(code.contains(r#"self.context.mark("1");"#));
    assert!(code.contains(r#"self.context.mark("start");"#));
}

#[test]
fn test_start_runs_the_initial_state_entry_actions() {
    // Entry actions are emitted in the transition arms that enter a state, and
    // nothing transitions into the initial one — `new` only assigns the field.
    // So `entry / display_welcome()` on the initial state never ran.
    let code = generate(
        r#"
        fsm Probe {
            [*] --> Idle
            state Idle { entry / display_welcome() entry / arm(watchdog) }
            state Busy
            Idle --> Busy : Go
        }
    "#,
    );

    let start = code
        .split("pub fn start(&mut self) {")
        .nth(1)
        .expect("no start method");
    let start = start.split("\n    }").next().unwrap();

    assert!(start.contains("self.context.display_welcome();"));
    assert!(start.contains(r#"self.context.arm("watchdog");"#));
}

#[test]
fn test_new_has_no_side_effects() {
    // Construction stays free of side effects: on an embedded target the machine
    // is often built before the peripherals its actions drive are ready.
    let code = generate(
        r#"
        fsm Probe {
            [*] --> Idle
            state Idle { entry / display_welcome() }
            state Busy
            Idle --> Busy : Go
        }
    "#,
    );

    let new = code
        .split("pub fn new(context: Ctx) -> Self {")
        .nth(1)
        .expect("no constructor");
    let new = new.split("\n    }").next().unwrap();

    assert!(
        !new.contains("self.context."),
        "constructor calls an action:\n{new}"
    );
}

#[test]
fn test_start_is_emitted_even_with_no_entry_actions() {
    // Uniform calling pattern: the caller should not have to know whether this
    // particular machine happens to need it.
    let code = generate(
        r#"
        fsm Probe {
            [*] --> Idle
            state Idle
            state Busy
            Idle --> Busy : Go
        }
    "#,
    );

    assert!(code.contains("pub fn start(&mut self) {"));
    assert!(code.contains("// The initial state has no entry actions."));
}

#[test]
fn test_usage_doc_shows_start() {
    let code = generate(CONNECTION_MANAGER);
    assert!(code.contains("/// machine.start();"));
}

/// Two choice points, one with `[else]` and one without.
const CHOICES: &str = r#"
    fsm Form {
        [*] --> Editing
        state Editing { entry / clear() }
        state Validating
        state Submitting { entry / send() }
        state Failed
        Editing --> Validating : Submit
        Validating --> <<Verdict>> : Checked
        Submitting --> <<Outcome>> : Replied

        choice Verdict {
            [all_fields_valid] -> Submitting
            [else] -> Editing / highlight_errors()
        }

        choice Outcome {
            [ok] -> Editing
            [retryable] -> Submitting / increment_retry()
        }
    }
"#;

#[test]
fn test_choice_point_expands_into_a_guard_chain() {
    // UML evaluates a choice after the incoming transition's actions have run,
    // taking the first branch whose guard holds.
    let code = generate(CHOICES);

    let arm = code
        .split("(FormState::Validating, FormEvent::Checked) => {")
        .nth(1)
        .expect("no arm for the transition into the choice point");
    let arm = arm.split("\n            (").next().unwrap();

    assert!(arm.contains("if self.context.all_fields_valid() {"));
    assert!(arm.contains("self.state = FormState::Submitting;"));
    assert!(arm.contains("} else {"));
    // The [else] branch's own action runs before the state change.
    let highlight = arm.find("highlight_errors").expect("else action missing");
    let editing = arm.find("self.state = FormState::Editing;").expect("else target");
    assert!(highlight < editing, "branch action must precede the state change");
}

#[test]
fn test_choice_branch_runs_the_target_entry_actions() {
    let code = generate(CHOICES);

    let arm = code
        .split("(FormState::Validating, FormEvent::Checked) => {")
        .nth(1)
        .unwrap();
    let arm = arm.split("\n            (").next().unwrap();

    let set_state = arm.find("self.state = FormState::Submitting;").unwrap();
    let entry = arm.find("self.context.send();").expect("entry action missing");
    assert!(set_state < entry, "entry action must follow the state change");
}

#[test]
fn test_choice_without_else_leaves_the_state_alone() {
    // Inventing a destination the author did not write would be worse than
    // staying put, but it is easy to overlook, so the output says so.
    let code = generate(CHOICES);

    let arm = code
        .split("(FormState::Submitting, FormEvent::Replied) => {")
        .nth(1)
        .unwrap();
    let arm = arm.split("\n            (").next().unwrap();

    assert!(arm.contains("if self.context.ok() {"));
    assert!(arm.contains("} else if self.context.retryable() {"));
    assert!(
        arm.contains("No [else] branch"),
        "missing note about the absent fallback:\n{arm}"
    );
}

#[test]
fn test_choice_guards_and_actions_reach_the_trait() {
    let code = generate(CHOICES);

    assert!(code.contains("fn all_fields_valid(&self) -> bool;"));
    assert!(code.contains("fn retryable(&self) -> bool;"));
    assert!(code.contains("fn highlight_errors(&mut self);"));
    assert!(code.contains("fn increment_retry(&mut self);"));
    // `[else]` is not a guard.
    assert!(!code.contains("fn else"));
}

#[test]
fn test_undefined_choice_point_is_rejected() {
    let errors = expect_errors(
        r#"
        fsm Probe {
            [*] --> A
            state A
            A --> <<Missing>> : Go
        }
    "#,
    );

    assert!(
        errors.iter().any(|e| e.contains("Missing")),
        "expected an undefined choice point error, got {errors:?}"
    );
}

#[test]
fn test_transition_into_a_choice_gets_a_note_instead_of_a_test() {
    // The destination depends on run-time guards, so no single target can be
    // asserted.
    let code = generate(CHOICES);

    assert!(
        code.contains("leads to choice point 'Verdict'"),
        "expected a note explaining the skipped test:\n{code}"
    );
}
