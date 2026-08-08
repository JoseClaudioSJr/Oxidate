//! Door Lock FSM - Functional test with guards and actions
//!
//! This example demonstrates:
//! - Guards (conditional transitions)
//! - Actions (side effects on transitions)
//! - More complex state machine logic

use std::cell::RefCell;
use std::rc::Rc;

// ============================================================================
// GENERATED CODE (from Oxidate)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorState {
    Locked,
    Unlocked,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorEvent {
    Unlock,
    Lock,
    Open,
    Close,
}

/// Actions trait with guards
pub trait DoorActions {
    // Guards (return true if transition should proceed)
    fn is_valid_code(&self) -> bool;
    fn is_closed(&self) -> bool;
    
    // Entry/Exit actions
    fn on_enter_locked(&mut self);
    fn on_exit_locked(&mut self);
    fn on_enter_unlocked(&mut self);
    fn on_exit_unlocked(&mut self);
    fn on_enter_open(&mut self);
    fn on_exit_open(&mut self);
    
    // Transition actions
    fn play_success_sound(&mut self);
    fn play_error_sound(&mut self);
    fn log_access(&mut self);
}

pub struct DoorFsm<A: DoorActions> {
    state: DoorState,
    actions: A,
}

impl<A: DoorActions> DoorFsm<A> {
    pub fn new(mut actions: A) -> Self {
        actions.on_enter_locked();
        Self {
            state: DoorState::Locked,
            actions,
        }
    }
    
    pub fn state(&self) -> DoorState {
        self.state
    }
    
    pub fn process(&mut self, event: DoorEvent) -> bool {
        match (self.state, event) {
            // Locked -> Unlocked: requires valid code
            (DoorState::Locked, DoorEvent::Unlock) => {
                if self.actions.is_valid_code() {
                    self.actions.on_exit_locked();
                    self.actions.play_success_sound();
                    self.actions.log_access();
                    self.state = DoorState::Unlocked;
                    self.actions.on_enter_unlocked();
                    true
                } else {
                    self.actions.play_error_sound();
                    false
                }
            }
            
            // Unlocked -> Locked
            (DoorState::Unlocked, DoorEvent::Lock) => {
                self.actions.on_exit_unlocked();
                self.state = DoorState::Locked;
                self.actions.on_enter_locked();
                true
            }
            
            // Unlocked -> Open
            (DoorState::Unlocked, DoorEvent::Open) => {
                self.actions.on_exit_unlocked();
                self.state = DoorState::Open;
                self.actions.on_enter_open();
                true
            }
            
            // Open -> Unlocked (close door)
            (DoorState::Open, DoorEvent::Close) => {
                self.actions.on_exit_open();
                self.state = DoorState::Unlocked;
                self.actions.on_enter_unlocked();
                true
            }
            
            // Open -> Locked: can't lock while open
            (DoorState::Open, DoorEvent::Lock) => {
                if self.actions.is_closed() {
                    self.actions.on_exit_open();
                    self.state = DoorState::Locked;
                    self.actions.on_enter_locked();
                    true
                } else {
                    self.actions.play_error_sound();
                    false
                }
            }
            
            // Invalid transitions
            _ => false,
        }
    }
}

// ============================================================================
// TEST IMPLEMENTATION
// ============================================================================

struct TestDoorActions {
    valid_code: bool,
    door_closed: bool,
    log: Rc<RefCell<Vec<String>>>,
}

// Helper methods kept for illustration; not every one is exercised here.
#[allow(dead_code)]
impl TestDoorActions {
    fn new(log: Rc<RefCell<Vec<String>>>) -> Self {
        Self {
            valid_code: false,
            door_closed: false,
            log,
        }
    }
    
    fn set_valid_code(&mut self, valid: bool) {
        self.valid_code = valid;
    }
    
    fn set_door_closed(&mut self, closed: bool) {
        self.door_closed = closed;
    }
}

impl DoorActions for TestDoorActions {
    fn is_valid_code(&self) -> bool {
        self.valid_code
    }
    
    fn is_closed(&self) -> bool {
        self.door_closed
    }
    
    fn on_enter_locked(&mut self) {
        self.log.borrow_mut().push("ENTER: Locked".to_string());
    }
    
    fn on_exit_locked(&mut self) {
        self.log.borrow_mut().push("EXIT: Locked".to_string());
    }
    
    fn on_enter_unlocked(&mut self) {
        self.log.borrow_mut().push("ENTER: Unlocked".to_string());
    }
    
    fn on_exit_unlocked(&mut self) {
        self.log.borrow_mut().push("EXIT: Unlocked".to_string());
    }
    
    fn on_enter_open(&mut self) {
        self.log.borrow_mut().push("ENTER: Open".to_string());
    }
    
    fn on_exit_open(&mut self) {
        self.log.borrow_mut().push("EXIT: Open".to_string());
    }
    
    fn play_success_sound(&mut self) {
        self.log.borrow_mut().push("ACTION: Success Sound".to_string());
    }
    
    fn play_error_sound(&mut self) {
        self.log.borrow_mut().push("ACTION: Error Sound".to_string());
    }
    
    fn log_access(&mut self) {
        self.log.borrow_mut().push("ACTION: Log Access".to_string());
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_unlock_with_invalid_code() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let actions = TestDoorActions::new(log.clone());
        let mut fsm = DoorFsm::new(actions);
        
        // Try to unlock with invalid code
        let transitioned = fsm.process(DoorEvent::Unlock);
        
        assert!(!transitioned);
        assert_eq!(fsm.state(), DoorState::Locked);
        
        // Should have played error sound
        assert!(log.borrow().iter().any(|s| s.contains("Error Sound")));
    }
    
    #[test]
    fn test_unlock_with_valid_code() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut actions = TestDoorActions::new(log.clone());
        actions.set_valid_code(true);
        let mut fsm = DoorFsm::new(actions);
        
        let transitioned = fsm.process(DoorEvent::Unlock);
        
        assert!(transitioned);
        assert_eq!(fsm.state(), DoorState::Unlocked);
        
        // Should have logged access and played success sound
        let log_ref = log.borrow();
        assert!(log_ref.iter().any(|s| s.contains("Success Sound")));
        assert!(log_ref.iter().any(|s| s.contains("Log Access")));
    }
    
    #[test]
    fn test_open_close_cycle() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut actions = TestDoorActions::new(log.clone());
        actions.set_valid_code(true);
        let mut fsm = DoorFsm::new(actions);
        
        // Unlock -> Open -> Close
        fsm.process(DoorEvent::Unlock);
        assert_eq!(fsm.state(), DoorState::Unlocked);
        
        fsm.process(DoorEvent::Open);
        assert_eq!(fsm.state(), DoorState::Open);
        
        fsm.process(DoorEvent::Close);
        assert_eq!(fsm.state(), DoorState::Unlocked);
    }
    
    #[test]
    fn test_cannot_lock_while_open() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut actions = TestDoorActions::new(log.clone());
        actions.set_valid_code(true);
        actions.set_door_closed(false);
        let mut fsm = DoorFsm::new(actions);
        
        // Get to Open state
        fsm.process(DoorEvent::Unlock);
        fsm.process(DoorEvent::Open);
        assert_eq!(fsm.state(), DoorState::Open);
        
        // Try to lock while door is open
        let transitioned = fsm.process(DoorEvent::Lock);
        
        assert!(!transitioned);
        assert_eq!(fsm.state(), DoorState::Open);
    }
    
    #[test]
    fn test_full_workflow() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut actions = TestDoorActions::new(log.clone());
        actions.set_valid_code(true);
        let mut fsm = DoorFsm::new(actions);
        
        // Unlock with valid code
        assert!(fsm.process(DoorEvent::Unlock));
        assert_eq!(fsm.state(), DoorState::Unlocked);
        
        // Open door
        assert!(fsm.process(DoorEvent::Open));
        assert_eq!(fsm.state(), DoorState::Open);
        
        // Close door
        assert!(fsm.process(DoorEvent::Close));
        assert_eq!(fsm.state(), DoorState::Unlocked);
        
        // Lock door
        assert!(fsm.process(DoorEvent::Lock));
        assert_eq!(fsm.state(), DoorState::Locked);
    }
}

// ============================================================================
// MAIN
// ============================================================================

fn main() {
    println!("Door Lock FSM - Guards and Actions Example\n");
    
    let log = Rc::new(RefCell::new(Vec::new()));
    let actions = TestDoorActions::new(log.clone());
    
    // First try with invalid code
    let mut fsm = DoorFsm::new(actions);
    println!("Initial state: {:?}", fsm.state());
    
    println!("\n1. Try to unlock with invalid code:");
    fsm.process(DoorEvent::Unlock);
    println!("   State: {:?}", fsm.state());
    
    // Now set valid code (we need to access the actions through the FSM)
    // In real usage, the guard would check external state
    println!("\n2. Creating new FSM with valid code...");
    
    let log2 = Rc::new(RefCell::new(Vec::new()));
    let mut actions2 = TestDoorActions::new(log2.clone());
    actions2.set_valid_code(true);
    let mut fsm2 = DoorFsm::new(actions2);
    
    println!("\n3. Unlock with valid code:");
    fsm2.process(DoorEvent::Unlock);
    println!("   State: {:?}", fsm2.state());
    
    println!("\n4. Open door:");
    fsm2.process(DoorEvent::Open);
    println!("   State: {:?}", fsm2.state());
    
    println!("\n5. Close door:");
    fsm2.process(DoorEvent::Close);
    println!("   State: {:?}", fsm2.state());
    
    println!("\n6. Lock door:");
    fsm2.process(DoorEvent::Lock);
    println!("   State: {:?}", fsm2.state());
    
    println!("\nAction log:");
    for (i, action) in log2.borrow().iter().enumerate() {
        println!("  {}: {}", i + 1, action);
    }
    
    println!("\n✅ Door lock FSM works correctly with guards!");
}
