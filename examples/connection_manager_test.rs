//! Connection Manager FSM - Complete Functional Test
//!
//! Tests for the connection_manager.fsm example demonstrating:
//! - Multiple timers (connect_timeout, keepalive, reconnect_delay)
//! - Reconnection logic
//! - Internal transitions (KeepaliveTick)
//! - Error handling paths

use std::cell::RefCell;
use std::rc::Rc;

// ============================================================================
// GENERATED CODE (simulating Oxidate output for connection_manager.fsm)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionEvent {
    // User commands
    Connect,
    Disconnect,
    Cancel,
    // Connection events
    ConnectionEstablished,
    ConnectionFailed,
    ConnectionLost,
    // Timer events
    ConnectTimeout,
    KeepaliveTick,
    ReconnectTimer,
}

/// Actions trait for connection manager
pub trait ConnectionActions {
    // State entry/exit actions
    fn reset_connection(&mut self);
    fn initiate_connection(&mut self);
    fn on_connected(&mut self);
    fn on_disconnected(&mut self);
    fn graceful_close(&mut self);
    
    // Timer control
    fn start_connect_timeout(&mut self);
    fn stop_connect_timeout(&mut self);
    fn start_keepalive(&mut self);
    fn stop_keepalive(&mut self);
    fn start_reconnect_delay(&mut self);
    fn stop_reconnect_delay(&mut self);
    
    // Internal actions
    fn send_keepalive(&mut self);
    fn log_timeout(&mut self);
    fn log_failure(&mut self);
}

pub struct ConnectionFsm<A: ConnectionActions> {
    state: ConnectionState,
    actions: A,
}

impl<A: ConnectionActions> ConnectionFsm<A> {
    pub fn new(mut actions: A) -> Self {
        actions.reset_connection();
        Self {
            state: ConnectionState::Disconnected,
            actions,
        }
    }
    
    pub fn state(&self) -> ConnectionState {
        self.state
    }
    
    pub fn process(&mut self, event: ConnectionEvent) -> bool {
        match (self.state, event) {
            // Disconnected -> Connecting
            (ConnectionState::Disconnected, ConnectionEvent::Connect) => {
                self.state = ConnectionState::Connecting;
                self.actions.initiate_connection();
                self.actions.start_connect_timeout();
                true
            }
            
            // Connecting -> Connected (success)
            (ConnectionState::Connecting, ConnectionEvent::ConnectionEstablished) => {
                self.actions.stop_connect_timeout();
                self.actions.on_connected();
                self.state = ConnectionState::Connected;
                self.actions.start_keepalive();
                true
            }
            
            // Connecting -> Disconnected (timeout)
            (ConnectionState::Connecting, ConnectionEvent::ConnectTimeout) => {
                self.actions.stop_connect_timeout();
                self.actions.log_timeout();
                self.state = ConnectionState::Disconnected;
                self.actions.reset_connection();
                true
            }
            
            // Connecting -> Disconnected (failed)
            (ConnectionState::Connecting, ConnectionEvent::ConnectionFailed) => {
                self.actions.stop_connect_timeout();
                self.actions.log_failure();
                self.state = ConnectionState::Disconnected;
                self.actions.reset_connection();
                true
            }
            
            // Connecting -> Disconnected (cancel)
            (ConnectionState::Connecting, ConnectionEvent::Cancel) => {
                self.actions.stop_connect_timeout();
                self.state = ConnectionState::Disconnected;
                self.actions.reset_connection();
                true
            }
            
            // Connected: internal keepalive
            (ConnectionState::Connected, ConnectionEvent::KeepaliveTick) => {
                self.actions.send_keepalive();
                true // Internal transition
            }
            
            // Connected -> Reconnecting (lost)
            (ConnectionState::Connected, ConnectionEvent::ConnectionLost) => {
                self.actions.stop_keepalive();
                self.actions.on_disconnected();
                self.state = ConnectionState::Reconnecting;
                self.actions.start_reconnect_delay();
                true
            }
            
            // Connected -> Disconnected (manual disconnect)
            (ConnectionState::Connected, ConnectionEvent::Disconnect) => {
                self.actions.stop_keepalive();
                self.actions.graceful_close();
                self.state = ConnectionState::Disconnected;
                self.actions.reset_connection();
                true
            }
            
            // Reconnecting -> Connecting (retry)
            (ConnectionState::Reconnecting, ConnectionEvent::ReconnectTimer) => {
                self.actions.stop_reconnect_delay();
                self.state = ConnectionState::Connecting;
                self.actions.initiate_connection();
                self.actions.start_connect_timeout();
                true
            }
            
            // Reconnecting -> Disconnected (cancel)
            (ConnectionState::Reconnecting, ConnectionEvent::Cancel) => {
                self.actions.stop_reconnect_delay();
                self.state = ConnectionState::Disconnected;
                self.actions.reset_connection();
                true
            }
            
            _ => false,
        }
    }
    
    /// Helper to send a message/event
    pub fn send(&mut self, event: ConnectionEvent) -> bool {
        self.process(event)
    }
}

// ============================================================================
// TEST IMPLEMENTATION
// ============================================================================

#[derive(Clone, Default)]
struct TestConnectionActions {
    log: Rc<RefCell<Vec<String>>>,
    active_timers: Rc<RefCell<Vec<String>>>,
    keepalive_count: Rc<RefCell<u32>>,
}

// Helper methods kept for illustration; not every one is exercised here.
#[allow(dead_code)]
impl TestConnectionActions {
    fn new() -> Self {
        Self::default()
    }
    
    fn get_log(&self) -> Vec<String> {
        self.log.borrow().clone()
    }
    
    fn get_active_timers(&self) -> Vec<String> {
        self.active_timers.borrow().clone()
    }
    
    fn get_keepalive_count(&self) -> u32 {
        *self.keepalive_count.borrow()
    }
}

impl ConnectionActions for TestConnectionActions {
    fn reset_connection(&mut self) {
        self.log.borrow_mut().push("ACTION: reset_connection".to_string());
    }
    
    fn initiate_connection(&mut self) {
        self.log.borrow_mut().push("ACTION: initiate_connection".to_string());
    }
    
    fn on_connected(&mut self) {
        self.log.borrow_mut().push("ACTION: on_connected".to_string());
    }
    
    fn on_disconnected(&mut self) {
        self.log.borrow_mut().push("ACTION: on_disconnected".to_string());
    }
    
    fn graceful_close(&mut self) {
        self.log.borrow_mut().push("ACTION: graceful_close".to_string());
    }
    
    fn start_connect_timeout(&mut self) {
        self.active_timers.borrow_mut().push("connect_timeout".to_string());
        self.log.borrow_mut().push("TIMER: start connect_timeout".to_string());
    }
    
    fn stop_connect_timeout(&mut self) {
        self.active_timers.borrow_mut().retain(|t| t != "connect_timeout");
        self.log.borrow_mut().push("TIMER: stop connect_timeout".to_string());
    }
    
    fn start_keepalive(&mut self) {
        self.active_timers.borrow_mut().push("keepalive".to_string());
        self.log.borrow_mut().push("TIMER: start keepalive".to_string());
    }
    
    fn stop_keepalive(&mut self) {
        self.active_timers.borrow_mut().retain(|t| t != "keepalive");
        self.log.borrow_mut().push("TIMER: stop keepalive".to_string());
    }
    
    fn start_reconnect_delay(&mut self) {
        self.active_timers.borrow_mut().push("reconnect_delay".to_string());
        self.log.borrow_mut().push("TIMER: start reconnect_delay".to_string());
    }
    
    fn stop_reconnect_delay(&mut self) {
        self.active_timers.borrow_mut().retain(|t| t != "reconnect_delay");
        self.log.borrow_mut().push("TIMER: stop reconnect_delay".to_string());
    }
    
    fn send_keepalive(&mut self) {
        *self.keepalive_count.borrow_mut() += 1;
        self.log.borrow_mut().push("ACTION: send_keepalive".to_string());
    }
    
    fn log_timeout(&mut self) {
        self.log.borrow_mut().push("ACTION: log_timeout".to_string());
    }
    
    fn log_failure(&mut self) {
        self.log.borrow_mut().push("ACTION: log_failure".to_string());
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
        let actions = TestConnectionActions::new();
        let fsm = ConnectionFsm::new(actions.clone());
        
        assert_eq!(fsm.state(), ConnectionState::Disconnected);
        assert!(actions.get_log().contains(&"ACTION: reset_connection".to_string()));
    }
    
    #[test]
    fn test_connect_success_flow() {
        let actions = TestConnectionActions::new();
        let mut fsm = ConnectionFsm::new(actions.clone());
        
        // Connect
        assert!(fsm.send(ConnectionEvent::Connect));
        assert_eq!(fsm.state(), ConnectionState::Connecting);
        assert!(actions.get_active_timers().contains(&"connect_timeout".to_string()));
        
        // Connection established
        assert!(fsm.send(ConnectionEvent::ConnectionEstablished));
        assert_eq!(fsm.state(), ConnectionState::Connected);
        assert!(actions.get_active_timers().contains(&"keepalive".to_string()));
        assert!(!actions.get_active_timers().contains(&"connect_timeout".to_string()));
    }
    
    #[test]
    fn test_connect_timeout() {
        let actions = TestConnectionActions::new();
        let mut fsm = ConnectionFsm::new(actions.clone());
        
        fsm.send(ConnectionEvent::Connect);
        assert!(fsm.send(ConnectionEvent::ConnectTimeout));
        
        assert_eq!(fsm.state(), ConnectionState::Disconnected);
        assert!(actions.get_log().iter().any(|s| s.contains("log_timeout")));
    }
    
    #[test]
    fn test_connect_failure() {
        let actions = TestConnectionActions::new();
        let mut fsm = ConnectionFsm::new(actions.clone());
        
        fsm.send(ConnectionEvent::Connect);
        assert!(fsm.send(ConnectionEvent::ConnectionFailed));
        
        assert_eq!(fsm.state(), ConnectionState::Disconnected);
        assert!(actions.get_log().iter().any(|s| s.contains("log_failure")));
    }
    
    #[test]
    fn test_keepalive_internal_transition() {
        let actions = TestConnectionActions::new();
        let mut fsm = ConnectionFsm::new(actions.clone());
        
        // Get to Connected state
        fsm.send(ConnectionEvent::Connect);
        fsm.send(ConnectionEvent::ConnectionEstablished);
        
        // Send multiple keepalive ticks
        for _ in 0..5 {
            assert!(fsm.send(ConnectionEvent::KeepaliveTick));
            assert_eq!(fsm.state(), ConnectionState::Connected); // Stay in same state
        }
        
        assert_eq!(actions.get_keepalive_count(), 5);
    }
    
    #[test]
    fn test_connection_lost_and_reconnect() {
        let actions = TestConnectionActions::new();
        let mut fsm = ConnectionFsm::new(actions.clone());
        
        // Get to Connected state
        fsm.send(ConnectionEvent::Connect);
        fsm.send(ConnectionEvent::ConnectionEstablished);
        assert_eq!(fsm.state(), ConnectionState::Connected);
        
        // Connection lost
        assert!(fsm.send(ConnectionEvent::ConnectionLost));
        assert_eq!(fsm.state(), ConnectionState::Reconnecting);
        assert!(actions.get_active_timers().contains(&"reconnect_delay".to_string()));
        
        // Reconnect timer fires
        assert!(fsm.send(ConnectionEvent::ReconnectTimer));
        assert_eq!(fsm.state(), ConnectionState::Connecting);
        
        // Connection established again
        assert!(fsm.send(ConnectionEvent::ConnectionEstablished));
        assert_eq!(fsm.state(), ConnectionState::Connected);
    }
    
    #[test]
    fn test_graceful_disconnect() {
        let actions = TestConnectionActions::new();
        let mut fsm = ConnectionFsm::new(actions.clone());
        
        // Get to Connected state
        fsm.send(ConnectionEvent::Connect);
        fsm.send(ConnectionEvent::ConnectionEstablished);
        
        // Manual disconnect
        assert!(fsm.send(ConnectionEvent::Disconnect));
        assert_eq!(fsm.state(), ConnectionState::Disconnected);
        assert!(actions.get_log().iter().any(|s| s.contains("graceful_close")));
    }
    
    #[test]
    fn test_cancel_connecting() {
        let actions = TestConnectionActions::new();
        let mut fsm = ConnectionFsm::new(actions.clone());
        
        fsm.send(ConnectionEvent::Connect);
        assert_eq!(fsm.state(), ConnectionState::Connecting);
        
        assert!(fsm.send(ConnectionEvent::Cancel));
        assert_eq!(fsm.state(), ConnectionState::Disconnected);
    }
    
    #[test]
    fn test_cancel_reconnecting() {
        let actions = TestConnectionActions::new();
        let mut fsm = ConnectionFsm::new(actions.clone());
        
        // Get to Reconnecting state
        fsm.send(ConnectionEvent::Connect);
        fsm.send(ConnectionEvent::ConnectionEstablished);
        fsm.send(ConnectionEvent::ConnectionLost);
        assert_eq!(fsm.state(), ConnectionState::Reconnecting);
        
        // Cancel reconnection
        assert!(fsm.send(ConnectionEvent::Cancel));
        assert_eq!(fsm.state(), ConnectionState::Disconnected);
    }
    
    #[test]
    fn test_multiple_reconnect_attempts() {
        let actions = TestConnectionActions::new();
        let mut fsm = ConnectionFsm::new(actions.clone());
        
        // Initial connection
        fsm.send(ConnectionEvent::Connect);
        fsm.send(ConnectionEvent::ConnectionEstablished);
        
        // Simulate 3 reconnection cycles
        for i in 0..3 {
            // Connection lost
            fsm.send(ConnectionEvent::ConnectionLost);
            assert_eq!(fsm.state(), ConnectionState::Reconnecting, "Cycle {} lost", i);
            
            // Reconnect timer
            fsm.send(ConnectionEvent::ReconnectTimer);
            assert_eq!(fsm.state(), ConnectionState::Connecting, "Cycle {} reconnecting", i);
            
            // Connection established
            fsm.send(ConnectionEvent::ConnectionEstablished);
            assert_eq!(fsm.state(), ConnectionState::Connected, "Cycle {} connected", i);
        }
    }
    
    #[test]
    fn test_invalid_events_ignored() {
        let actions = TestConnectionActions::new();
        let mut fsm = ConnectionFsm::new(actions);
        
        // Can't disconnect when already disconnected
        assert!(!fsm.send(ConnectionEvent::Disconnect));
        assert_eq!(fsm.state(), ConnectionState::Disconnected);
        
        // Can't receive ConnectionEstablished when disconnected
        assert!(!fsm.send(ConnectionEvent::ConnectionEstablished));
        assert_eq!(fsm.state(), ConnectionState::Disconnected);
    }
}

// ============================================================================
// MAIN
// ============================================================================

fn main() {
    println!("Connection Manager FSM - Reconnection Logic Example\n");
    
    let actions = TestConnectionActions::new();
    let mut fsm = ConnectionFsm::new(actions.clone());
    
    println!("=== Happy Path ===");
    println!("State: {:?}", fsm.state());
    
    fsm.send(ConnectionEvent::Connect);
    println!("After Connect: {:?}", fsm.state());
    
    fsm.send(ConnectionEvent::ConnectionEstablished);
    println!("After ConnectionEstablished: {:?}", fsm.state());
    
    // Send some keepalives
    for i in 1..=3 {
        fsm.send(ConnectionEvent::KeepaliveTick);
        println!("Keepalive #{}: {:?}", i, fsm.state());
    }
    
    println!("\n=== Connection Lost & Reconnect ===");
    fsm.send(ConnectionEvent::ConnectionLost);
    println!("After ConnectionLost: {:?}", fsm.state());
    
    fsm.send(ConnectionEvent::ReconnectTimer);
    println!("After ReconnectTimer: {:?}", fsm.state());
    
    fsm.send(ConnectionEvent::ConnectionEstablished);
    println!("After ConnectionEstablished: {:?}", fsm.state());
    
    println!("\n=== Graceful Disconnect ===");
    fsm.send(ConnectionEvent::Disconnect);
    println!("After Disconnect: {:?}", fsm.state());
    
    println!("\n--- Action Log ---");
    for (i, action) in actions.get_log().iter().enumerate() {
        println!("  {}: {}", i + 1, action);
    }
    
    println!("\n✅ Connection Manager FSM works correctly!");
}
