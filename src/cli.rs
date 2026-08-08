//! Oxidate CLI - Command Line Interface for FSM parsing

use oxidate_fsm::parser::parse_fsm;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Oxidate CLI - FSM Parser");
        println!("Usage: oxidate-cli <file.fsm>");
        println!();
        println!("Example: oxidate-cli examples/traffic_light.fsm");
        return;
    }

    let filename = &args[1];
    
    match fs::read_to_string(filename) {
        Ok(content) => {
            match parse_fsm(&content) {
                Ok(fsms) => {
                    println!("✅ Successfully parsed {} FSM(s):", fsms.len());
                    for fsm in &fsms {
                        println!();
                        println!("  FSM: {}", fsm.name);
                        println!("  States: {}", fsm.states.len());
                        for state in &fsm.states {
                            println!("    - {} ({:?})", state.name, state.state_type);
                        }
                        println!("  Transitions: {}", fsm.transitions.len());
                        for t in &fsm.transitions {
                            println!("    {} --> {} : {}", t.source, t.target, t.label());
                        }
                        if let Some(ref initial) = fsm.initial_state {
                            println!("  Initial State: {}", initial);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("❌ Parse error: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("❌ Could not read file '{}': {}", filename, e);
        }
    }
}
