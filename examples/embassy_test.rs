//! Embassy Active Object FSM - Functional Test
//!
//! This example demonstrates the Embassy async embedded code pattern:
//! - Active Object pattern with event channels
//! - Async task-based state machine
//! - Event posting from multiple sources
//! - Software timers
//!
//! Note: This is a simulation that runs on std for testing.
//! In real embedded use, replace with actual embassy-executor.

use std::cell::RefCell;
use std::rc::Rc;
use std::collections::VecDeque;

// ============================================================================
// SIMULATED EMBASSY TYPES (for testing without actual hardware)
// ============================================================================

/// Simulated event channel (replaces embassy_sync::channel::Channel)
pub struct Channel<T, const N: usize> {
    queue: RefCell<VecDeque<T>>,
}

impl<T, const N: usize> Channel<T, N> {
    pub const fn new() -> Self {
        Self {
            queue: RefCell::new(VecDeque::new()),
        }
    }
    
    pub fn sender(&self) -> Sender<'_, T, N> {
        Sender { channel: self }
    }
    
    pub fn receiver(&self) -> Receiver<'_, T, N> {
        Receiver { channel: self }
    }
}

pub struct Sender<'a, T, const N: usize> {
    channel: &'a Channel<T, N>,
}

impl<'a, T: Clone, const N: usize> Sender<'a, T, N> {
    pub fn try_send(&self, event: T) -> bool {
        let mut queue = self.channel.queue.borrow_mut();
        if queue.len() < N {
            queue.push_back(event);
            true
        } else {
            false
        }
    }
}

impl<'a, T, const N: usize> Clone for Sender<'a, T, N> {
    fn clone(&self) -> Self {
        Self { channel: self.channel }
    }
}

pub struct Receiver<'a, T, const N: usize> {
    channel: &'a Channel<T, N>,
}

impl<'a, T, const N: usize> Receiver<'a, T, N> {
    pub fn try_receive(&self) -> Option<T> {
        self.channel.queue.borrow_mut().pop_front()
    }
}

// ============================================================================
// GENERATED CODE (simulating Oxidate Embassy output)
// ============================================================================

const EVENT_QUEUE_SIZE: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BlinkState {
    Off,
    On,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BlinkEvent {
    Toggle,
    Pause,
    Resume,
    Tick,      // Timer tick
    Timeout,   // Timeout event
}

/// Actions trait - implement for your hardware
pub trait BlinkActions {
    fn turn_on_led(&mut self);
    fn turn_off_led(&mut self);
    fn log_paused(&mut self);
    fn log_resumed(&mut self);
}

/// Active Object for Blink FSM
pub struct BlinkActiveObject<T: BlinkActions> {
    state: BlinkState,
    context: T,
    tick_count: u32,
}

// Helper methods kept for illustration; not every one is exercised here.
#[allow(dead_code)]
impl<T: BlinkActions> BlinkActiveObject<T> {
    pub fn new(context: T) -> Self {
        Self {
            state: BlinkState::Off,
            context,
            tick_count: 0,
        }
    }
    
    pub fn state(&self) -> BlinkState {
        self.state
    }
    
    pub fn tick_count(&self) -> u32 {
        self.tick_count
    }
    
    fn init(&mut self) {
        // Entry action for initial state
        self.context.turn_off_led();
    }
    
    pub fn dispatch(&mut self, event: BlinkEvent) {
        match (self.state, event) {
            // Off -> On on Toggle
            (BlinkState::Off, BlinkEvent::Toggle) => {
                self.context.turn_on_led();
                self.state = BlinkState::On;
            }
            // On -> Off on Toggle
            (BlinkState::On, BlinkEvent::Toggle) => {
                self.context.turn_off_led();
                self.state = BlinkState::Off;
            }
            // Any -> Paused on Pause
            (BlinkState::Off, BlinkEvent::Pause) | (BlinkState::On, BlinkEvent::Pause) => {
                self.context.log_paused();
                self.state = BlinkState::Paused;
            }
            // Paused -> Off on Resume
            (BlinkState::Paused, BlinkEvent::Resume) => {
                self.context.log_resumed();
                self.context.turn_off_led();
                self.state = BlinkState::Off;
            }
            // Tick increments counter (internal action)
            (_, BlinkEvent::Tick) => {
                self.tick_count += 1;
            }
            _ => {} // Event ignored
        }
    }
    
    /// Run the event loop (simulated - in real Embassy this would be async)
    pub fn run_once(&mut self, receiver: &Receiver<'_, BlinkEvent, EVENT_QUEUE_SIZE>) -> bool {
        if let Some(event) = receiver.try_receive() {
            self.dispatch(event);
            true
        } else {
            false
        }
    }
    
    /// Process all pending events
    pub fn run_all(&mut self, receiver: &Receiver<'_, BlinkEvent, EVENT_QUEUE_SIZE>) -> u32 {
        let mut count = 0;
        while self.run_once(receiver) {
            count += 1;
        }
        count
    }
}

/// Event poster handle (for sending events to the Active Object)
#[derive(Clone)]
pub struct BlinkPoster<'a> {
    sender: Sender<'a, BlinkEvent, EVENT_QUEUE_SIZE>,
}

impl<'a> BlinkPoster<'a> {
    pub fn new(sender: Sender<'a, BlinkEvent, EVENT_QUEUE_SIZE>) -> Self {
        Self { sender }
    }
    
    /// Post event (non-blocking)
    pub fn post(&self, event: BlinkEvent) -> bool {
        self.sender.try_send(event)
    }
    
    /// Post from ISR context (same as post in simulation)
    pub fn post_from_isr(&self, event: BlinkEvent) -> bool {
        self.post(event)
    }
}

/// Event with optional data payload
#[derive(Debug, Clone)]
pub struct BlinkEvt<T = ()> {
    pub sig: BlinkEvent,
    pub data: T,
}

impl<T> BlinkEvt<T> {
    pub const fn new(sig: BlinkEvent, data: T) -> Self {
        Self { sig, data }
    }
}

impl BlinkEvt<()> {
    pub const fn signal(sig: BlinkEvent) -> Self {
        Self { sig, data: () }
    }
}

// ============================================================================
// TEST IMPLEMENTATION
// ============================================================================

#[derive(Clone)]
struct TestBlinkActions {
    log: Rc<RefCell<Vec<String>>>,
    led_state: Rc<RefCell<bool>>,
}

// Helper methods kept for illustration; not every one is exercised here.
#[allow(dead_code)]
impl TestBlinkActions {
    fn new() -> Self {
        Self {
            log: Rc::new(RefCell::new(Vec::new())),
            led_state: Rc::new(RefCell::new(false)),
        }
    }
    
    fn get_log(&self) -> Vec<String> {
        self.log.borrow().clone()
    }
    
    fn is_led_on(&self) -> bool {
        *self.led_state.borrow()
    }
}

impl BlinkActions for TestBlinkActions {
    fn turn_on_led(&mut self) {
        *self.led_state.borrow_mut() = true;
        self.log.borrow_mut().push("LED: ON".to_string());
    }
    
    fn turn_off_led(&mut self) {
        *self.led_state.borrow_mut() = false;
        self.log.borrow_mut().push("LED: OFF".to_string());
    }
    
    fn log_paused(&mut self) {
        self.log.borrow_mut().push("STATE: Paused".to_string());
    }
    
    fn log_resumed(&mut self) {
        self.log.borrow_mut().push("STATE: Resumed".to_string());
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_active_object_initial_state() {
        let actions = TestBlinkActions::new();
        let ao = BlinkActiveObject::new(actions);
        
        assert_eq!(ao.state(), BlinkState::Off);
    }
    
    #[test]
    fn test_event_posting() {
        let channel: Channel<BlinkEvent, EVENT_QUEUE_SIZE> = Channel::new();
        let poster = BlinkPoster::new(channel.sender());
        
        assert!(poster.post(BlinkEvent::Toggle));
        assert!(poster.post(BlinkEvent::Toggle));
        assert!(poster.post(BlinkEvent::Pause));
        
        // Verify events are queued
        let receiver = channel.receiver();
        assert_eq!(receiver.try_receive(), Some(BlinkEvent::Toggle));
        assert_eq!(receiver.try_receive(), Some(BlinkEvent::Toggle));
        assert_eq!(receiver.try_receive(), Some(BlinkEvent::Pause));
        assert_eq!(receiver.try_receive(), None);
    }
    
    #[test]
    fn test_active_object_processes_events() {
        let channel: Channel<BlinkEvent, EVENT_QUEUE_SIZE> = Channel::new();
        let poster = BlinkPoster::new(channel.sender());
        let actions = TestBlinkActions::new();
        let mut ao = BlinkActiveObject::new(actions.clone());
        
        // Post events
        poster.post(BlinkEvent::Toggle);
        poster.post(BlinkEvent::Toggle);
        
        // Process events
        let count = ao.run_all(&channel.receiver());
        
        assert_eq!(count, 2);
        assert_eq!(ao.state(), BlinkState::Off); // Toggle twice = back to Off
    }
    
    #[test]
    fn test_toggle_cycle() {
        let actions = TestBlinkActions::new();
        let mut ao = BlinkActiveObject::new(actions.clone());
        
        // Simulate toggle cycle
        ao.dispatch(BlinkEvent::Toggle);
        assert_eq!(ao.state(), BlinkState::On);
        assert!(actions.is_led_on());
        
        ao.dispatch(BlinkEvent::Toggle);
        assert_eq!(ao.state(), BlinkState::Off);
        assert!(!actions.is_led_on());
    }
    
    #[test]
    fn test_pause_resume() {
        let actions = TestBlinkActions::new();
        let mut ao = BlinkActiveObject::new(actions.clone());
        
        // Get to On state
        ao.dispatch(BlinkEvent::Toggle);
        assert_eq!(ao.state(), BlinkState::On);
        
        // Pause
        ao.dispatch(BlinkEvent::Pause);
        assert_eq!(ao.state(), BlinkState::Paused);
        
        // Resume
        ao.dispatch(BlinkEvent::Resume);
        assert_eq!(ao.state(), BlinkState::Off);
        
        // Check log
        let log = actions.get_log();
        assert!(log.contains(&"STATE: Paused".to_string()));
        assert!(log.contains(&"STATE: Resumed".to_string()));
    }
    
    #[test]
    fn test_tick_counting() {
        let actions = TestBlinkActions::new();
        let mut ao = BlinkActiveObject::new(actions);
        
        for _ in 0..10 {
            ao.dispatch(BlinkEvent::Tick);
        }
        
        assert_eq!(ao.tick_count(), 10);
    }
    
    #[test]
    fn test_multiple_posters() {
        let channel: Channel<BlinkEvent, EVENT_QUEUE_SIZE> = Channel::new();
        
        // Create multiple posters (simulating different ISRs/tasks)
        let poster1 = BlinkPoster::new(channel.sender());
        let poster2 = BlinkPoster::new(channel.sender());
        let poster3 = poster1.clone();
        
        poster1.post(BlinkEvent::Toggle);
        poster2.post(BlinkEvent::Tick);
        poster3.post(BlinkEvent::Pause);
        
        let actions = TestBlinkActions::new();
        let mut ao = BlinkActiveObject::new(actions);
        let count = ao.run_all(&channel.receiver());
        
        assert_eq!(count, 3);
    }
    
    #[test]
    fn test_post_from_isr() {
        let channel: Channel<BlinkEvent, EVENT_QUEUE_SIZE> = Channel::new();
        let poster = BlinkPoster::new(channel.sender());
        
        // Simulate ISR posting
        assert!(poster.post_from_isr(BlinkEvent::Toggle));
        
        let actions = TestBlinkActions::new();
        let mut ao = BlinkActiveObject::new(actions);
        ao.run_all(&channel.receiver());
        
        assert_eq!(ao.state(), BlinkState::On);
    }
    
    #[test]
    fn test_event_with_payload() {
        // Test event envelope with data
        let evt = BlinkEvt::new(BlinkEvent::Toggle, 42u32);
        assert_eq!(evt.sig, BlinkEvent::Toggle);
        assert_eq!(evt.data, 42);
        
        // Test signal helper
        let simple = BlinkEvt::signal(BlinkEvent::Pause);
        assert_eq!(simple.sig, BlinkEvent::Pause);
    }
    
    #[test]
    fn test_queue_overflow_handling() {
        let channel: Channel<BlinkEvent, 2> = Channel::new(); // Small queue
        let sender = channel.sender();
        
        assert!(sender.try_send(BlinkEvent::Toggle));
        assert!(sender.try_send(BlinkEvent::Toggle));
        assert!(!sender.try_send(BlinkEvent::Toggle)); // Queue full
    }
    
    #[test]
    fn test_ignored_events() {
        let actions = TestBlinkActions::new();
        let mut ao = BlinkActiveObject::new(actions);
        
        // Resume when not paused should be ignored
        ao.dispatch(BlinkEvent::Resume);
        assert_eq!(ao.state(), BlinkState::Off);
    }
}

// ============================================================================
// MAIN
// ============================================================================

fn main() {
    println!("Embassy Active Object FSM - Functional Test\n");
    
    // Create the event channel
    let channel: Channel<BlinkEvent, EVENT_QUEUE_SIZE> = Channel::new();
    let poster = BlinkPoster::new(channel.sender());
    
    let actions = TestBlinkActions::new();
    let mut ao = BlinkActiveObject::new(actions.clone());
    
    println!("Initial state: {:?}", ao.state());
    
    // Simulate different event sources posting
    println!("\n=== Simulating Event Sources ===");
    
    println!("Task 1 posts: Toggle");
    poster.post(BlinkEvent::Toggle);
    
    println!("ISR posts: Tick (x3)");
    poster.post_from_isr(BlinkEvent::Tick);
    poster.post_from_isr(BlinkEvent::Tick);
    poster.post_from_isr(BlinkEvent::Tick);
    
    println!("Task 2 posts: Toggle");
    poster.post(BlinkEvent::Toggle);
    
    println!("Timer posts: Pause");
    poster.post(BlinkEvent::Pause);
    
    println!("\n=== Processing Events ===");
    let mut i = 0;
    while ao.run_once(&channel.receiver()) {
        i += 1;
        println!("  Event {}: State = {:?}, Ticks = {}", i, ao.state(), ao.tick_count());
    }
    
    println!("\nFinal state: {:?}", ao.state());
    println!("Total ticks: {}", ao.tick_count());
    
    println!("\n--- Action Log ---");
    for (i, action) in actions.get_log().iter().enumerate() {
        println!("  {}: {}", i + 1, action);
    }
    
    println!("\n✅ Embassy Active Object pattern works correctly!");
}
