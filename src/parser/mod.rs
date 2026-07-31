//! FSM Parser Module
//! Parses Mermaid-like DSL into FSM data structures

use pest::Parser;
use pest_derive::Parser;
use thiserror::Error;

use crate::fsm::{
    Action, ChoiceBranch, ChoicePoint, Event, FsmDefinition, Guard, State, StateType, Timer,
    TimerMode, Transition, TransitionKind,
};

#[cfg(test)]
mod tests;

#[derive(Parser)]
#[grammar = "parser/fsm.pest"]
pub struct FsmParser;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Parse error: {0}")]
    PestError(#[from] pest::error::Error<Rule>),
    #[error("Invalid syntax at line {line}: {message}")]
    SyntaxError { line: usize, message: String },
    #[error("Unknown state reference: {0}")]
    UnknownState(String),
}

pub type ParseResult<T> = Result<T, ParseError>;

/// Parse FSM DSL source code into FSM definitions
pub fn parse_fsm(source: &str) -> ParseResult<Vec<FsmDefinition>> {
    let pairs = FsmParser::parse(Rule::file, source)?;
    let mut fsms = Vec::new();

    for pair in pairs {
        match pair.as_rule() {
            Rule::file => {
                for inner in pair.into_inner() {
                    if inner.as_rule() == Rule::fsm_definition {
                        fsms.push(parse_fsm_definition(inner)?);
                    }
                }
            }
            Rule::fsm_definition => {
                fsms.push(parse_fsm_definition(pair)?);
            }
            Rule::EOI => {}
            _ => {}
        }
    }

    Ok(fsms)
}

fn parse_fsm_definition(pair: pest::iterators::Pair<Rule>) -> ParseResult<FsmDefinition> {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();

    let mut fsm = FsmDefinition::new(name);

    for item in inner {
        match item.as_rule() {
            Rule::fsm_body => {
                parse_fsm_body(item, &mut fsm)?;
            }
            _ => {}
        }
    }

    Ok(fsm)
}

fn parse_fsm_body(pair: pest::iterators::Pair<Rule>, fsm: &mut FsmDefinition) -> ParseResult<()> {
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::fsm_item => {
                parse_fsm_item(item, fsm)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn parse_fsm_item(pair: pest::iterators::Pair<Rule>, fsm: &mut FsmDefinition) -> ParseResult<()> {
    let inner = pair.into_inner().next().unwrap();

    match inner.as_rule() {
        Rule::initial_state => {
            let mut inner_iter = inner.into_inner();
            // Skip arrow, get identifier
            let state_name = inner_iter.last().unwrap().as_str();
            fsm.initial_state = Some(state_name.to_string());

            // Ensure the initial state exists
            if !fsm.states.iter().any(|s| s.name == state_name) {
                fsm.states.push(State::new(state_name, StateType::Simple));
            }
        }
        Rule::timer_def => {
            let timer = parse_timer_def(inner)?;
            fsm.timers.push(timer);
        }
        Rule::choice_def => {
            let choice = parse_choice_def(inner)?;
            fsm.choice_points.push(choice);
        }
        Rule::state_simple | Rule::state_with_body => {
            let state = parse_state_definition(inner)?;
            // Update existing or add new
            if let Some(existing) = fsm.states.iter_mut().find(|s| s.name == state.name) {
                existing.description = state.description;
                existing.entry_actions.extend(state.entry_actions);
                existing.exit_actions.extend(state.exit_actions);
                existing.internal_transitions = state.internal_transitions;
            } else {
                fsm.states.push(state);
            }
        }
        Rule::transition => {
            let transition = parse_transition(inner)?;

            // Ensure source and target states exist (unless it's a choice point target)
            if transition.source != "[*]" && !transition.source.starts_with("<<") {
                if !fsm.states.iter().any(|s| s.name == transition.source) {
                    fsm.states
                        .push(State::new(&transition.source, StateType::Simple));
                }
            }
            if transition.target != "[*]" && !transition.target.starts_with("<<") {
                if !fsm.states.iter().any(|s| s.name == transition.target) {
                    fsm.states
                        .push(State::new(&transition.target, StateType::Simple));
                }
            }

            fsm.transitions.push(transition);
        }
        _ => {}
    }

    Ok(())
}

// ============================================================================
// TIMER PARSING
// ============================================================================

fn parse_timer_def(pair: pest::iterators::Pair<Rule>) -> ParseResult<Timer> {
    let mut inner = pair.into_inner();

    let name = inner.next().unwrap().as_str().to_string();
    let duration_ms: u32 = inner.next().unwrap().as_str().parse().unwrap_or(1000);
    
    // Skip arrow token if present
    let mut event_name = inner.next().unwrap().as_str().to_string();
    if event_name == "->" || event_name == "-->" {
        event_name = inner.next().unwrap().as_str().to_string();
    }

    let mode = if let Some(mode_pair) = inner.next() {
        match mode_pair.as_str() {
            "periodic" => TimerMode::Periodic,
            _ => TimerMode::OneShot,
        }
    } else {
        TimerMode::OneShot
    };

    Ok(Timer {
        name,
        duration_ms,
        event: Event { name: event_name },
        mode,
        auto_start_state: None,
    })
}

// ============================================================================
// CHOICE POINT PARSING
// ============================================================================

fn parse_choice_def(pair: pest::iterators::Pair<Rule>) -> ParseResult<ChoicePoint> {
    let mut inner = pair.into_inner();

    let name = inner.next().unwrap().as_str().to_string();
    let mut choice = ChoicePoint::new(&name);

    for branch_pair in inner {
        if branch_pair.as_rule() == Rule::choice_branch {
            let branch = parse_choice_branch(branch_pair)?;
            choice.branches.push(branch);
        }
    }

    Ok(choice)
}

fn parse_choice_branch(pair: pest::iterators::Pair<Rule>) -> ParseResult<ChoiceBranch> {
    let mut inner = pair.into_inner();

    // Guard
    let guard_pair = inner.next().unwrap();
    let guard_expr = guard_pair
        .into_inner()
        .next()
        .unwrap()
        .as_str()
        .trim()
        .to_string();

    // Skip arrow if present
    let mut next = inner.next().unwrap();
    if next.as_str() == "->" || next.as_str() == "-->" {
        next = inner.next().unwrap();
    }
    
    // Target state
    let target = next.as_str().to_string();

    // Optional action
    let action = if let Some(action_pair) = inner.next() {
        // Skip arrow if it appears before action
        if action_pair.as_str() == "->" || action_pair.as_str() == "-->" {
            if let Some(real_action) = inner.next() {
                let action_body = real_action.into_inner().next().unwrap();
                Some(parse_action_call(action_body)?)
            } else {
                None
            }
        } else {
            let action_body = action_pair.into_inner().next().unwrap();
            Some(parse_action_call(action_body)?)
        }
    } else {
        None
    };

    Ok(ChoiceBranch {
        guard: Guard {
            expression: guard_expr,
        },
        target,
        action,
    })
}

fn parse_state_definition(pair: pest::iterators::Pair<Rule>) -> ParseResult<State> {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();

    let mut state = State::new(&name, StateType::Simple);

    for item in inner {
        match item.as_rule() {
            Rule::description => {
                state.description = Some(item.as_str().trim().to_string());
            }
            Rule::state_body_item => {
                parse_state_body_item(item, &mut state)?;
            }
            Rule::entry_action => {
                let action = parse_action_call(item.into_inner().next().unwrap())?;
                state.entry_actions.push(action);
            }
            Rule::exit_action => {
                let action = parse_action_call(item.into_inner().next().unwrap())?;
                state.exit_actions.push(action);
            }
            Rule::internal_action => {
                let mut action_inner = item.into_inner();
                let event_name = action_inner.next().unwrap().as_str().to_string();
                let action = parse_action_call(action_inner.next().unwrap())?;

                let transition = Transition {
                    source: state.name.clone(),
                    target: state.name.clone(),
                    event: Some(Event { name: event_name }),
                    guard: None,
                    action: Some(action),
                    kind: TransitionKind::Internal,
                };
                state.internal_transitions.push(transition);
            }
            _ => {}
        }
    }

    Ok(state)
}

fn parse_state_body_item(pair: pest::iterators::Pair<Rule>, state: &mut State) -> ParseResult<()> {
    let action_item = pair.into_inner().next().unwrap();
    match action_item.as_rule() {
        Rule::entry_action => {
            let action = parse_action_call(action_item.into_inner().next().unwrap())?;
            state.entry_actions.push(action);
        }
        Rule::exit_action => {
            let action = parse_action_call(action_item.into_inner().next().unwrap())?;
            state.exit_actions.push(action);
        }
        Rule::timer_start => {
            // Add timer start to entry actions
            let timer_name = action_item.into_inner().next().unwrap().as_str().to_string();
            let action = Action {
                name: format!("start_timer_{}", timer_name),
                params: vec![timer_name],
            };
            state.entry_actions.push(action);
        }
        Rule::timer_stop => {
            // Add timer stop to exit actions
            let timer_name = action_item.into_inner().next().unwrap().as_str().to_string();
            let action = Action {
                name: format!("stop_timer_{}", timer_name),
                params: vec![timer_name],
            };
            state.exit_actions.push(action);
        }
        Rule::internal_transition => {
            // Internal transition with optional guard: event [guard] / action
            let mut inner = action_item.into_inner();
            let event_name = inner.next().unwrap().as_str().to_string();

            let mut guard: Option<Guard> = None;
            let mut action: Option<Action> = None;

            for item in inner {
                match item.as_rule() {
                    Rule::guard => {
                        let expr = item.into_inner().next().unwrap().as_str().trim();
                        guard = Some(Guard {
                            expression: expr.to_string(),
                        });
                    }
                    Rule::action_call => {
                        action = Some(parse_action_call(item)?);
                    }
                    _ => {}
                }
            }

            let transition = Transition {
                source: state.name.clone(),
                target: state.name.clone(),
                event: Some(Event { name: event_name }),
                guard,
                action,
                kind: TransitionKind::Internal,
            };
            state.internal_transitions.push(transition);
        }
        Rule::internal_action => {
            let mut inner = action_item.into_inner();
            let event_name = inner.next().unwrap().as_str().to_string();
            let action = parse_action_call(inner.next().unwrap())?;

            let transition = Transition {
                source: state.name.clone(),
                target: state.name.clone(),
                event: Some(Event { name: event_name }),
                guard: None,
                action: Some(action),
                kind: TransitionKind::Internal,
            };
            state.internal_transitions.push(transition);
        }
        _ => {}
    }
    Ok(())
}

#[allow(dead_code)]
fn parse_state_body(pair: pest::iterators::Pair<Rule>, state: &mut State) -> ParseResult<()> {
    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::state_body_item => {
                parse_state_body_item(item, state)?;
            }
            _ => {}
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn parse_hierarchical_state(pair: pest::iterators::Pair<Rule>) -> ParseResult<State> {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();

    let mut state = State::new(&name, StateType::Composite);
    state.sub_fsm = Some(FsmDefinition::new(format!("{}_sub", name)));

    for item in inner {
        match item.as_rule() {
            Rule::fsm_item => {
                if let Some(ref mut sub_fsm) = state.sub_fsm {
                    parse_fsm_item(item, sub_fsm)?;
                }
            }
            _ => {}
        }
    }

    Ok(state)
}

fn parse_transition(pair: pest::iterators::Pair<Rule>) -> ParseResult<Transition> {
    let mut inner = pair.into_inner();

    let source = inner.next().unwrap().as_str().to_string();
    let _arrow = inner.next(); // Skip arrow

    // Parse target - may be a choice target <<choice_name>>
    let target_pair = inner.next().unwrap();
    let target = if target_pair.as_rule() == Rule::choice_target {
        // Extract name from <<name>>
        let choice_name = target_pair.into_inner().next().unwrap().as_str();
        format!("<<{}>>", choice_name)
    } else {
        target_pair.as_str().to_string()
    };

    let mut transition = Transition {
        source,
        target,
        event: None,
        guard: None,
        action: None,
        kind: TransitionKind::External,
    };

    // Parse optional transition label
    if let Some(label) = inner.next() {
        for item in label.into_inner() {
            match item.as_rule() {
                Rule::event => {
                    transition.event = Some(Event {
                        name: item.as_str().to_string(),
                    });
                }
                Rule::guard => {
                    let expr = item.into_inner().next().unwrap().as_str().trim();
                    transition.guard = Some(Guard {
                        expression: expr.to_string(),
                    });
                }
                Rule::action => {
                    let action_body = item.into_inner().next().unwrap();
                    transition.action = Some(parse_action_call(action_body)?);
                }
                _ => {}
            }
        }
    }

    Ok(transition)
}

fn parse_action_call(pair: pest::iterators::Pair<Rule>) -> ParseResult<Action> {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();

    let mut params = Vec::new();
    if let Some(params_pair) = inner.next() {
        for param in params_pair.into_inner() {
            params.push(param.as_str().to_string());
        }
    }

    Ok(Action { name, params })
}

#[allow(dead_code)]
fn parse_action_body(pair: pest::iterators::Pair<Rule>) -> ParseResult<Action> {
    parse_action_call(pair)
}
