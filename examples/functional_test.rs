//! Functional test example for Oxidate-generated FSM code
//! 
//! This example demonstrates:
//! 1. A complete traffic light FSM definition
//! 2. The generated Rust code from Oxidate
//! 3. An Actions trait implementation
//! 4. A test that exercises all state transitions

use std::cell::RefCell;
use std::rc::Rc;

// ============================================================================
// GENERATED CODE (from Oxidate)
// This is what Oxidate generates from a traffic_light.fsm file
// ============================================================================

/// States of the TrafficLight FSM
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficLightState {
    Red,
    Green,
    Yellow,
}

/// Events that can trigger transitions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficLightEvent {
    TimerExpired,
}

/// Actions trait - implement this to provide behavior
pub trait TrafficLightActions {
    /// Called when entering Red state
    fn on_enter_red(&mut self);
    /// Called when exiting Red state
    fn on_exit_red(&mut self);
    /// Called when entering Green state
    fn on_enter_green(&mut self);
    /// Called when exiting Green state
    fn on_exit_green(&mut self);
    /// Called when entering Yellow state
    fn on_enter_yellow(&mut self);
    /// Called when exiting Yellow state
    fn on_exit_yellow(&mut self);
}

/// The Traffic Light State Machine
pub struct TrafficLightFsm<A: TrafficLightActions> {
    state: TrafficLightState,
    actions: A,
}

impl<A: TrafficLightActions> TrafficLightFsm<A> {
    /// Create a new FSM with the given actions handler
    pub fn new(mut actions: A) -> Self {
        // Execute entry action for initial state
        actions.on_enter_red();
        Self {
            state: TrafficLightState::Red,
            actions,
        }
    }
    
    /// Get current state
    pub fn state(&self) -> TrafficLightState {
        self.state
    }
    
    /// Process an event and transition if applicable
    pub fn process(&mut self, event: TrafficLightEvent) -> bool {
        match (self.state, event) {
            // Red -> Green on TimerExpired
            (TrafficLightState::Red, TrafficLightEvent::TimerExpired) => {
                self.actions.on_exit_red();
                self.state = TrafficLightState::Green;
                self.actions.on_enter_green();
                true
            }
            // Green -> Yellow on TimerExpired
            (TrafficLightState::Green, TrafficLightEvent::TimerExpired) => {
                self.actions.on_exit_green();
                self.state = TrafficLightState::Yellow;
                self.actions.on_enter_yellow();
                true
            }
            // Yellow -> Red on TimerExpired
            (TrafficLightState::Yellow, TrafficLightEvent::TimerExpired) => {
                self.actions.on_exit_yellow();
                self.state = TrafficLightState::Red;
                self.actions.on_enter_red();
                true
            }
        }
    }
}

// ============================================================================
// USER IMPLEMENTATION
// This is what the user would write to use the generated FSM
// ============================================================================

/// Test implementation that records all actions
#[derive(Default)]
struct TestTrafficLightActions {
    log: Rc<RefCell<Vec<String>>>,
}

impl TestTrafficLightActions {
    fn new(log: Rc<RefCell<Vec<String>>>) -> Self {
        Self { log }
    }
}

impl TrafficLightActions for TestTrafficLightActions {
    fn on_enter_red(&mut self) {
        self.log.borrow_mut().push("ENTER: Red".to_string());
    }
    
    fn on_exit_red(&mut self) {
        self.log.borrow_mut().push("EXIT: Red".to_string());
    }
    
    fn on_enter_green(&mut self) {
        self.log.borrow_mut().push("ENTER: Green".to_string());
    }
    
    fn on_exit_green(&mut self) {
        self.log.borrow_mut().push("EXIT: Green".to_string());
    }
    
    fn on_enter_yellow(&mut self) {
        self.log.borrow_mut().push("ENTER: Yellow".to_string());
    }
    
    fn on_exit_yellow(&mut self) {
        self.log.borrow_mut().push("EXIT: Yellow".to_string());
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_initial_state() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let actions = TestTrafficLightActions::new(log.clone());
        let fsm = TrafficLightFsm::new(actions);
        
        assert_eq!(fsm.state(), TrafficLightState::Red);
        assert_eq!(log.borrow().len(), 1);
        assert_eq!(log.borrow()[0], "ENTER: Red");
    }
    
    #[test]
    fn test_full_cycle() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let actions = TestTrafficLightActions::new(log.clone());
        let mut fsm = TrafficLightFsm::new(actions);
        
        // Initial state
        assert_eq!(fsm.state(), TrafficLightState::Red);
        
        // Red -> Green
        let transitioned = fsm.process(TrafficLightEvent::TimerExpired);
        assert!(transitioned);
        assert_eq!(fsm.state(), TrafficLightState::Green);
        
        // Green -> Yellow
        let transitioned = fsm.process(TrafficLightEvent::TimerExpired);
        assert!(transitioned);
        assert_eq!(fsm.state(), TrafficLightState::Yellow);
        
        // Yellow -> Red
        let transitioned = fsm.process(TrafficLightEvent::TimerExpired);
        assert!(transitioned);
        assert_eq!(fsm.state(), TrafficLightState::Red);
        
        // Verify action log
        let expected = vec![
            "ENTER: Red",
            "EXIT: Red", "ENTER: Green",
            "EXIT: Green", "ENTER: Yellow",
            "EXIT: Yellow", "ENTER: Red",
        ];
        
        let binding = log.borrow();
        let actual: Vec<&str> = binding.iter().map(|s| s.as_str()).collect();
        assert_eq!(actual, expected);
    }
    
    #[test]
    fn test_multiple_cycles() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let actions = TestTrafficLightActions::new(log.clone());
        let mut fsm = TrafficLightFsm::new(actions);
        
        // Run 3 complete cycles
        for _ in 0..3 {
            fsm.process(TrafficLightEvent::TimerExpired); // Red -> Green
            fsm.process(TrafficLightEvent::TimerExpired); // Green -> Yellow
            fsm.process(TrafficLightEvent::TimerExpired); // Yellow -> Red
        }
        
        // Should be back at Red
        assert_eq!(fsm.state(), TrafficLightState::Red);
        
        // Initial enter + 3 cycles * (2 actions per transition * 3 transitions)
        // = 1 + 3 * 6 = 19 log entries
        assert_eq!(log.borrow().len(), 19);
    }
}

// ============================================================================
// MAIN
// ============================================================================

fn main() {
    println!("Oxidate FSM Functional Test Example\n");
    println!("This demonstrates generated FSM code with:");
    println!("  - State enum: TrafficLightState");
    println!("  - Event enum: TrafficLightEvent");
    println!("  - Actions trait: TrafficLightActions");
    println!("  - FSM struct: TrafficLightFsm\n");
    
    // Create FSM
    let log = Rc::new(RefCell::new(Vec::new()));
    let actions = TestTrafficLightActions::new(log.clone());
    let mut fsm = TrafficLightFsm::new(actions);
    
    println!("Initial state: {:?}", fsm.state());
    
    // Run through states
    for i in 1..=6 {
        fsm.process(TrafficLightEvent::TimerExpired);
        println!("After event {}: {:?}", i, fsm.state());
    }
    
    println!("\nAction log:");
    for (i, action) in log.borrow().iter().enumerate() {
        println!("  {}: {}", i + 1, action);
    }
    
    println!("\n✅ All transitions worked correctly!");
}
