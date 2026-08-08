//! RTIC Motor Controller FSM - Functional Test
//!
//! This example demonstrates the RTIC embedded code pattern that Oxidate
//! generates for real-time embedded systems:
//!
//! ## Features
//! - **Event Queue**: SPSC queue for ISR-to-task communication
//! - **Events with Payload**: Events can carry data (speed, sensor values, etc.)
//! - **Guards**: Conditional transitions based on runtime state
//! - **Extended State Variables**: Track runtime data like current speed
//! - **Fault Handling**: Overcurrent, overtemperature, and emergency stop
//!
//! ## Architecture
//! ```text
//! ISR (ADC, Encoder, Timer)
//!         │
//!         ▼ post(event)
//!   ┌─────────────┐
//!   │ Event Queue │  (heapless::spsc::Queue)
//!   └─────────────┘
//!         │
//!         ▼ process_one() / process_all()
//!   ┌─────────────┐
//!   │  Motor FSM  │  (state machine with guards/actions)
//!   └─────────────┘
//!         │
//!         ▼
//!   Hardware Actions (PWM, GPIO, etc.)
//! ```
//!
//! ## Note
//! This is a simulation that runs on `std` for testing.
//! In real embedded use, replace with actual RTIC app structure
//! and `heapless` crate for no-alloc queues.

use std::cell::RefCell;
use std::rc::Rc;

// ============================================================================
// SIMULATED HEAPLESS QUEUE (for testing without actual embedded)
// ============================================================================

/// Simulated SPSC Queue (replaces heapless::spsc::Queue)
pub struct Queue<T, const N: usize> {
    buffer: RefCell<Vec<T>>,
}

impl<T, const N: usize> Queue<T, N> {
    pub const fn new() -> Self {
        Self {
            buffer: RefCell::new(Vec::new()),
        }
    }
    
    pub fn enqueue(&mut self, item: T) -> Result<(), T> {
        let mut buf = self.buffer.borrow_mut();
        if buf.len() < N {
            buf.push(item);
            Ok(())
        } else {
            Err(item)
        }
    }
    
    pub fn dequeue(&mut self) -> Option<T> {
        let mut buf = self.buffer.borrow_mut();
        if buf.is_empty() {
            None
        } else {
            Some(buf.remove(0))
        }
    }
    
    pub fn len(&self) -> usize {
        self.buffer.borrow().len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.buffer.borrow().is_empty()
    }
}

// ============================================================================
// GENERATED CODE (simulating Oxidate RTIC output)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MotorState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Fault,
}

/// Event signal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MotorSignal {
    Start,
    Stop,
    EmergencyStop,
    StartupComplete,
    ShutdownComplete,
    OverCurrent,
    OverTemp,
    Reset,
    Tick,
    // New signals with associated data
    SetSpeed,
    SensorReading,
    SetPwm,
}

/// Event with payload
/// This allows events to carry data from ISRs to tasks
#[derive(Debug, Clone, Copy)]
pub struct MotorEvent {
    pub sig: MotorSignal,
    pub payload: EventPayload,
}

/// Payload for events that carry data.
/// Used to pass values from ISRs to the FSM (e.g., sensor readings, speed setpoints).
#[derive(Debug, Clone, Copy, Default)]
pub struct EventPayload {
    pub u16_val: u16,    // For speed, PWM values
    pub i16_val: i16,    // For signed values
    pub u32_val: u32,    // For sensor readings, timestamps
}

impl MotorEvent {
    /// Create event without payload
    pub const fn new(sig: MotorSignal) -> Self {
        Self {
            sig,
            payload: EventPayload { u16_val: 0, i16_val: 0, u32_val: 0 },
        }
    }
    
    /// Create event with u16 payload (speed, PWM)
    pub const fn with_u16(sig: MotorSignal, val: u16) -> Self {
        Self {
            sig,
            payload: EventPayload { u16_val: val, i16_val: 0, u32_val: 0 },
        }
    }
    
    /// Create event with i16 payload
    pub const fn with_i16(sig: MotorSignal, val: i16) -> Self {
        Self {
            sig,
            payload: EventPayload { u16_val: 0, i16_val: val, u32_val: 0 },
        }
    }
    
    /// Create event with u32 payload (sensor, timestamp)
    pub const fn with_u32(sig: MotorSignal, val: u32) -> Self {
        Self {
            sig,
            payload: EventPayload { u16_val: 0, i16_val: 0, u32_val: val },
        }
    }
}

// Convenience constructors for common events
impl MotorEvent {
    pub const START: Self = Self::new(MotorSignal::Start);
    pub const STOP: Self = Self::new(MotorSignal::Stop);
    pub const EMERGENCY_STOP: Self = Self::new(MotorSignal::EmergencyStop);
    pub const STARTUP_COMPLETE: Self = Self::new(MotorSignal::StartupComplete);
    pub const SHUTDOWN_COMPLETE: Self = Self::new(MotorSignal::ShutdownComplete);
    pub const OVERCURRENT: Self = Self::new(MotorSignal::OverCurrent);
    pub const OVERTEMP: Self = Self::new(MotorSignal::OverTemp);
    pub const RESET: Self = Self::new(MotorSignal::Reset);
    pub const TICK: Self = Self::new(MotorSignal::Tick);
    
    pub const fn set_speed(rpm: u16) -> Self {
        Self::with_u16(MotorSignal::SetSpeed, rpm)
    }
    
    pub const fn sensor_reading(value: u32) -> Self {
        Self::with_u32(MotorSignal::SensorReading, value)
    }
    
    pub const fn set_pwm(duty: u16) -> Self {
        Self::with_u16(MotorSignal::SetPwm, duty)
    }
}

/// Actions trait - implement for your hardware
pub trait MotorActions {
    // Guards
    fn is_safe_to_start(&self) -> bool;
    fn is_cooled_down(&self) -> bool;
    
    // Entry actions
    fn enable_power_stage(&mut self);
    fn disable_power_stage(&mut self);
    fn start_ramp_up(&mut self);
    fn start_ramp_down(&mut self);
    fn trigger_fault_alarm(&mut self);
    fn clear_fault_alarm(&mut self);
    
    // Transition actions
    fn log_event(&mut self, msg: &str);
    
    // Payload-based actions
    fn set_target_speed(&mut self, rpm: u16);
    fn get_target_speed(&self) -> u16;
    fn process_sensor(&mut self, value: u32);
    fn set_pwm_duty(&mut self, duty: u16);
}

/// Motor FSM for RTIC
pub struct MotorFsm<T: MotorActions, const N: usize = 8> {
    state: MotorState,
    event_queue: Queue<MotorEvent, N>,
    context: T,
    fault_count: u32,
    current_speed: u16,  // Extended state variable
}

impl<T: MotorActions, const N: usize> MotorFsm<T, N> {
    pub fn new(context: T) -> Self {
        Self {
            state: MotorState::Stopped,
            event_queue: Queue::new(),
            context,
            fault_count: 0,
            current_speed: 0,
        }
    }
    
    pub fn state(&self) -> MotorState {
        self.state
    }
    
    pub fn fault_count(&self) -> u32 {
        self.fault_count
    }
    
    pub fn queue_len(&self) -> usize {
        self.event_queue.len()
    }
    
    pub fn current_speed(&self) -> u16 {
        self.current_speed
    }
    
    pub fn target_speed(&self) -> u16 {
        self.context.get_target_speed()
    }
    
    /// Post event to queue.
    /// Call from ISR or higher-priority task to enqueue events for processing.
    #[inline]
    pub fn post(&mut self, event: MotorEvent) -> Result<(), MotorEvent> {
        self.event_queue.enqueue(event)
    }
    
    /// Post event without payload (convenience)
    #[inline]
    pub fn post_sig(&mut self, sig: MotorSignal) -> Result<(), MotorEvent> {
        self.post(MotorEvent::new(sig))
    }
    
    /// Process single event (call from RTIC task)
    pub fn process_one(&mut self) -> bool {
        if let Some(event) = self.event_queue.dequeue() {
            self.dispatch(event);
            true
        } else {
            false
        }
    }
    
    /// Process all queued events (call from RTIC task)
    pub fn process_all(&mut self) -> u32 {
        let mut count = 0;
        while let Some(event) = self.event_queue.dequeue() {
            self.dispatch(event);
            count += 1;
        }
        count
    }
    
    /// Direct dispatch for immediate event processing.
    /// Processes the event and executes state transitions with guards and actions.
    pub fn dispatch(&mut self, event: MotorEvent) {
        let old_state = self.state;
        let sig = event.sig;
        let payload = event.payload;
        
        match (self.state, sig) {
            // Stopped -> Starting (with guard)
            (MotorState::Stopped, MotorSignal::Start) => {
                if self.context.is_safe_to_start() {
                    self.context.log_event("Starting motor");
                    self.context.enable_power_stage();
                    self.context.start_ramp_up();
                    self.state = MotorState::Starting;
                } else {
                    self.context.log_event("Cannot start: not safe");
                }
            }
            
            // Starting -> Running
            (MotorState::Starting, MotorSignal::StartupComplete) => {
                self.context.log_event("Motor running");
                self.state = MotorState::Running;
            }
            
            // Running -> Stopping
            (MotorState::Running, MotorSignal::Stop) => {
                self.context.log_event("Stopping motor");
                self.context.start_ramp_down();
                self.current_speed = 0;
                self.state = MotorState::Stopping;
            }
            
            // Stopping -> Stopped
            (MotorState::Stopping, MotorSignal::ShutdownComplete) => {
                self.context.log_event("Motor stopped");
                self.context.disable_power_stage();
                self.state = MotorState::Stopped;
            }
            
            // ============================================================
            // EVENTS WITH PAYLOAD
            // ============================================================
            
            // SetSpeed with RPM payload - only when Running
            (MotorState::Running, MotorSignal::SetSpeed) => {
                let rpm = payload.u16_val;
                self.context.log_event(&format!("Set speed: {} RPM", rpm));
                self.context.set_target_speed(rpm);
                self.current_speed = rpm;
            }
            
            // SetPwm with duty cycle payload - only when Running
            (MotorState::Running, MotorSignal::SetPwm) => {
                let duty = payload.u16_val;
                self.context.log_event(&format!("Set PWM: {}%", duty));
                self.context.set_pwm_duty(duty);
            }
            
            // SensorReading with value - process in any state
            (_, MotorSignal::SensorReading) => {
                let value = payload.u32_val;
                self.context.process_sensor(value);
                
                // Check for overcurrent threshold (example: > 10000)
                if value > 10000 && self.state != MotorState::Fault && self.state != MotorState::Stopped {
                    self.context.log_event(&format!("Sensor overcurrent: {}", value));
                    self.context.disable_power_stage();
                    self.context.trigger_fault_alarm();
                    self.state = MotorState::Fault;
                    self.fault_count += 1;
                }
            }
            
            // ============================================================
            // FAULT EVENTS
            // ============================================================
            
            // Any -> Fault on OverCurrent/OverTemp
            (_, MotorSignal::OverCurrent) | (_, MotorSignal::OverTemp) => {
                if self.state != MotorState::Fault {
                    self.context.log_event("FAULT detected!");
                    self.context.disable_power_stage();
                    self.context.trigger_fault_alarm();
                    self.state = MotorState::Fault;
                    self.fault_count += 1;
                }
            }
            
            // Emergency stop from any running state
            (MotorState::Starting, MotorSignal::EmergencyStop) |
            (MotorState::Running, MotorSignal::EmergencyStop) |
            (MotorState::Stopping, MotorSignal::EmergencyStop) => {
                self.context.log_event("EMERGENCY STOP!");
                self.context.disable_power_stage();
                self.current_speed = 0;
                self.state = MotorState::Stopped;
            }
            
            // Fault -> Stopped (with guard)
            (MotorState::Fault, MotorSignal::Reset) => {
                if self.context.is_cooled_down() {
                    self.context.log_event("Fault cleared");
                    self.context.clear_fault_alarm();
                    self.state = MotorState::Stopped;
                } else {
                    self.context.log_event("Cannot reset: still hot");
                }
            }
            
            // Tick event (for timing, ignored for state)
            (_, MotorSignal::Tick) => {}
            
            // All other combinations ignored
            _ => {}
        }
        
        if old_state != self.state {
            self.context.log_event(&format!("{:?} -> {:?}", old_state, self.state));
        }
    }
}

// ============================================================================
// RTIC APP STRUCTURE SIMULATION
// ============================================================================

/// Simulated RTIC shared resources
pub struct SharedResources<T: MotorActions> {
    pub motor: MotorFsm<T, 16>,
}

/// Simulated RTIC local resources
pub struct LocalResources {
    pub tick_count: u32,
}

// ============================================================================
// TEST IMPLEMENTATION
// ============================================================================

#[derive(Clone)]
struct TestMotorActions {
    safe_to_start: Rc<RefCell<bool>>,
    cooled_down: Rc<RefCell<bool>>,
    log: Rc<RefCell<Vec<String>>>,
    power_enabled: Rc<RefCell<bool>>,
    alarm_active: Rc<RefCell<bool>>,
    target_speed: Rc<RefCell<u16>>,
    pwm_duty: Rc<RefCell<u16>>,
    last_sensor_value: Rc<RefCell<u32>>,
}

// Helper methods kept for illustration; not every one is exercised here.
#[allow(dead_code)]
impl TestMotorActions {
    fn new() -> Self {
        Self {
            safe_to_start: Rc::new(RefCell::new(true)),
            cooled_down: Rc::new(RefCell::new(true)),
            log: Rc::new(RefCell::new(Vec::new())),
            power_enabled: Rc::new(RefCell::new(false)),
            alarm_active: Rc::new(RefCell::new(false)),
            target_speed: Rc::new(RefCell::new(0)),
            pwm_duty: Rc::new(RefCell::new(0)),
            last_sensor_value: Rc::new(RefCell::new(0)),
        }
    }
    
    fn set_safe_to_start(&self, safe: bool) {
        *self.safe_to_start.borrow_mut() = safe;
    }
    
    fn set_cooled_down(&self, cooled: bool) {
        *self.cooled_down.borrow_mut() = cooled;
    }
    
    fn get_log(&self) -> Vec<String> {
        self.log.borrow().clone()
    }
    
    fn is_power_enabled(&self) -> bool {
        *self.power_enabled.borrow()
    }
    
    fn is_alarm_active(&self) -> bool {
        *self.alarm_active.borrow()
    }
    
    fn get_pwm_duty(&self) -> u16 {
        *self.pwm_duty.borrow()
    }
    
    fn get_last_sensor(&self) -> u32 {
        *self.last_sensor_value.borrow()
    }
}

impl MotorActions for TestMotorActions {
    fn is_safe_to_start(&self) -> bool {
        *self.safe_to_start.borrow()
    }
    
    fn is_cooled_down(&self) -> bool {
        *self.cooled_down.borrow()
    }
    
    fn enable_power_stage(&mut self) {
        *self.power_enabled.borrow_mut() = true;
        self.log.borrow_mut().push("HW: Power stage enabled".to_string());
    }
    
    fn disable_power_stage(&mut self) {
        *self.power_enabled.borrow_mut() = false;
        self.log.borrow_mut().push("HW: Power stage disabled".to_string());
    }
    
    fn start_ramp_up(&mut self) {
        self.log.borrow_mut().push("HW: Ramp up started".to_string());
    }
    
    fn start_ramp_down(&mut self) {
        self.log.borrow_mut().push("HW: Ramp down started".to_string());
    }
    
    fn trigger_fault_alarm(&mut self) {
        *self.alarm_active.borrow_mut() = true;
        self.log.borrow_mut().push("HW: Fault alarm ON".to_string());
    }
    
    fn clear_fault_alarm(&mut self) {
        *self.alarm_active.borrow_mut() = false;
        self.log.borrow_mut().push("HW: Fault alarm OFF".to_string());
    }
    
    fn log_event(&mut self, msg: &str) {
        self.log.borrow_mut().push(format!("LOG: {}", msg));
    }
    
    fn set_target_speed(&mut self, rpm: u16) {
        *self.target_speed.borrow_mut() = rpm;
        self.log.borrow_mut().push(format!("HW: Target speed set to {} RPM", rpm));
    }
    
    fn get_target_speed(&self) -> u16 {
        *self.target_speed.borrow()
    }
    
    fn process_sensor(&mut self, value: u32) {
        *self.last_sensor_value.borrow_mut() = value;
        self.log.borrow_mut().push(format!("HW: Sensor reading: {}", value));
    }
    
    fn set_pwm_duty(&mut self, duty: u16) {
        *self.pwm_duty.borrow_mut() = duty;
        self.log.borrow_mut().push(format!("HW: PWM duty set to {}%", duty));
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
        let actions = TestMotorActions::new();
        let fsm = MotorFsm::<_, 8>::new(actions);
        
        assert_eq!(fsm.state(), MotorState::Stopped);
        assert_eq!(fsm.fault_count(), 0);
        assert_eq!(fsm.current_speed(), 0);
    }
    
    #[test]
    fn test_happy_path_start_stop() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions.clone());
        
        // Start sequence using constant events
        fsm.dispatch(MotorEvent::START);
        assert_eq!(fsm.state(), MotorState::Starting);
        assert!(actions.is_power_enabled());
        
        fsm.dispatch(MotorEvent::STARTUP_COMPLETE);
        assert_eq!(fsm.state(), MotorState::Running);
        
        // Stop sequence
        fsm.dispatch(MotorEvent::STOP);
        assert_eq!(fsm.state(), MotorState::Stopping);
        
        fsm.dispatch(MotorEvent::SHUTDOWN_COMPLETE);
        assert_eq!(fsm.state(), MotorState::Stopped);
        assert!(!actions.is_power_enabled());
    }
    
    #[test]
    fn test_guard_prevents_unsafe_start() {
        let actions = TestMotorActions::new();
        actions.set_safe_to_start(false);
        
        let mut fsm = MotorFsm::<_, 8>::new(actions.clone());
        
        fsm.dispatch(MotorEvent::START);
        assert_eq!(fsm.state(), MotorState::Stopped); // Didn't start
        assert!(!actions.is_power_enabled());
    }
    
    #[test]
    fn test_overcurrent_fault() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions.clone());
        
        // Get to Running state
        fsm.dispatch(MotorEvent::START);
        fsm.dispatch(MotorEvent::STARTUP_COMPLETE);
        assert_eq!(fsm.state(), MotorState::Running);
        
        // Trigger overcurrent
        fsm.dispatch(MotorEvent::OVERCURRENT);
        assert_eq!(fsm.state(), MotorState::Fault);
        assert_eq!(fsm.fault_count(), 1);
        assert!(!actions.is_power_enabled());
        assert!(actions.is_alarm_active());
    }
    
    #[test]
    fn test_overtemp_fault() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions.clone());
        
        fsm.dispatch(MotorEvent::START);
        fsm.dispatch(MotorEvent::STARTUP_COMPLETE);
        
        fsm.dispatch(MotorEvent::OVERTEMP);
        assert_eq!(fsm.state(), MotorState::Fault);
    }
    
    #[test]
    fn test_fault_reset_with_guard() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions.clone());
        
        // Get to Fault state
        fsm.dispatch(MotorEvent::START);
        fsm.dispatch(MotorEvent::STARTUP_COMPLETE);
        fsm.dispatch(MotorEvent::OVERCURRENT);
        
        // Try reset while still hot
        actions.set_cooled_down(false);
        fsm.dispatch(MotorEvent::RESET);
        assert_eq!(fsm.state(), MotorState::Fault); // Still in fault
        
        // Reset after cooling
        actions.set_cooled_down(true);
        fsm.dispatch(MotorEvent::RESET);
        assert_eq!(fsm.state(), MotorState::Stopped);
        assert!(!actions.is_alarm_active());
    }
    
    #[test]
    fn test_emergency_stop() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions.clone());
        
        fsm.dispatch(MotorEvent::START);
        fsm.dispatch(MotorEvent::STARTUP_COMPLETE);
        assert_eq!(fsm.state(), MotorState::Running);
        
        fsm.dispatch(MotorEvent::EMERGENCY_STOP);
        assert_eq!(fsm.state(), MotorState::Stopped);
        assert!(!actions.is_power_enabled());
    }
    
    #[test]
    fn test_event_queue_posting() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions);
        
        // Post multiple events (simulating ISR) using post_sig for simple signals
        assert!(fsm.post_sig(MotorSignal::Start).is_ok());
        assert!(fsm.post_sig(MotorSignal::StartupComplete).is_ok());
        assert!(fsm.post_sig(MotorSignal::Stop).is_ok());
        
        assert_eq!(fsm.queue_len(), 3);
        
        // Process all
        let count = fsm.process_all();
        assert_eq!(count, 3);
        assert_eq!(fsm.state(), MotorState::Stopping);
    }
    
    #[test]
    fn test_queue_overflow() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 2>::new(actions); // Small queue
        
        assert!(fsm.post(MotorEvent::START).is_ok());
        assert!(fsm.post(MotorEvent::STOP).is_ok());
        assert!(fsm.post(MotorEvent::RESET).is_err()); // Queue full
    }
    
    #[test]
    fn test_process_one_at_a_time() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions);
        
        fsm.post(MotorEvent::START).unwrap();
        fsm.post(MotorEvent::STARTUP_COMPLETE).unwrap();
        
        assert!(fsm.process_one());
        assert_eq!(fsm.state(), MotorState::Starting);
        
        assert!(fsm.process_one());
        assert_eq!(fsm.state(), MotorState::Running);
        
        assert!(!fsm.process_one()); // Queue empty
    }
    
    #[test]
    fn test_multiple_faults() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions.clone());
        
        // First fault
        fsm.dispatch(MotorEvent::START);
        fsm.dispatch(MotorEvent::STARTUP_COMPLETE);
        fsm.dispatch(MotorEvent::OVERCURRENT);
        assert_eq!(fsm.fault_count(), 1);
        
        // Reset and run again
        fsm.dispatch(MotorEvent::RESET);
        fsm.dispatch(MotorEvent::START);
        fsm.dispatch(MotorEvent::STARTUP_COMPLETE);
        
        // Second fault
        fsm.dispatch(MotorEvent::OVERTEMP);
        assert_eq!(fsm.fault_count(), 2);
    }
    
    #[test]
    fn test_ignored_events() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions);
        
        // Stop when already stopped
        fsm.dispatch(MotorEvent::STOP);
        assert_eq!(fsm.state(), MotorState::Stopped);
        
        // StartupComplete when stopped
        fsm.dispatch(MotorEvent::STARTUP_COMPLETE);
        assert_eq!(fsm.state(), MotorState::Stopped);
    }
    
    // ========================================================================
    // PAYLOAD TESTS (events with data)
    // ========================================================================
    
    #[test]
    fn test_set_speed_with_payload() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions.clone());
        
        // Get to Running state
        fsm.dispatch(MotorEvent::START);
        fsm.dispatch(MotorEvent::STARTUP_COMPLETE);
        assert_eq!(fsm.state(), MotorState::Running);
        
        // Set speed using event with payload
        fsm.dispatch(MotorEvent::set_speed(1500));
        assert_eq!(fsm.current_speed(), 1500);
        assert_eq!(fsm.target_speed(), 1500);
        
        // Change speed
        fsm.dispatch(MotorEvent::set_speed(3000));
        assert_eq!(fsm.current_speed(), 3000);
        
        // Log should show the speed changes
        let log = actions.get_log();
        assert!(log.iter().any(|s| s.contains("1500 RPM")));
        assert!(log.iter().any(|s| s.contains("3000 RPM")));
    }
    
    #[test]
    fn test_pwm_duty_with_payload() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions.clone());
        
        // Get to Running state
        fsm.dispatch(MotorEvent::START);
        fsm.dispatch(MotorEvent::STARTUP_COMPLETE);
        
        // Set PWM duty cycle
        fsm.dispatch(MotorEvent::set_pwm(75));
        assert_eq!(actions.get_pwm_duty(), 75);
        
        let log = actions.get_log();
        assert!(log.iter().any(|s| s.contains("PWM duty set to 75%")));
    }
    
    #[test]
    fn test_sensor_reading_triggers_fault() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions.clone());
        
        // Get to Running state
        fsm.dispatch(MotorEvent::START);
        fsm.dispatch(MotorEvent::STARTUP_COMPLETE);
        
        // Normal sensor reading (below threshold)
        fsm.dispatch(MotorEvent::sensor_reading(5000));
        assert_eq!(fsm.state(), MotorState::Running);
        assert_eq!(actions.get_last_sensor(), 5000);
        
        // Overcurrent sensor reading (above 10000 threshold)
        fsm.dispatch(MotorEvent::sensor_reading(15000));
        assert_eq!(fsm.state(), MotorState::Fault);
        assert_eq!(fsm.fault_count(), 1);
        
        let log = actions.get_log();
        assert!(log.iter().any(|s| s.contains("Sensor overcurrent: 15000")));
    }
    
    #[test]
    fn test_event_queue_with_payloads() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 16>::new(actions.clone());
        
        // Queue events with and without payloads (like ISR posting)
        fsm.post(MotorEvent::START).unwrap();
        fsm.post(MotorEvent::STARTUP_COMPLETE).unwrap();
        fsm.post(MotorEvent::set_speed(1000)).unwrap();
        fsm.post(MotorEvent::set_pwm(50)).unwrap();
        fsm.post(MotorEvent::sensor_reading(2000)).unwrap();
        fsm.post(MotorEvent::set_speed(2000)).unwrap();
        
        assert_eq!(fsm.queue_len(), 6);
        
        // Process all
        let count = fsm.process_all();
        assert_eq!(count, 6);
        assert_eq!(fsm.state(), MotorState::Running);
        assert_eq!(fsm.current_speed(), 2000);
        assert_eq!(actions.get_pwm_duty(), 50);
        assert_eq!(actions.get_last_sensor(), 2000);
    }
    
    #[test]
    fn test_payload_ignored_in_wrong_state() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions.clone());
        
        // Try to set speed while Stopped (should be ignored)
        fsm.dispatch(MotorEvent::set_speed(1500));
        assert_eq!(fsm.current_speed(), 0);
        assert_eq!(fsm.state(), MotorState::Stopped);
        
        // Try to set PWM while Stopped
        fsm.dispatch(MotorEvent::set_pwm(100));
        assert_eq!(actions.get_pwm_duty(), 0);
    }
    
    #[test]
    fn test_isr_style_event_posting() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions.clone());
        
        // Start motor
        fsm.dispatch(MotorEvent::START);
        fsm.dispatch(MotorEvent::STARTUP_COMPLETE);
        
        // Simulate ADC ISR posting sensor readings
        for i in 0..5 {
            let reading = 1000 + i * 500;
            fsm.post(MotorEvent::sensor_reading(reading as u32)).unwrap();
        }
        
        // Simulate encoder ISR posting speed updates
        fsm.post(MotorEvent::set_speed(1200)).unwrap();
        
        assert_eq!(fsm.queue_len(), 6);
        
        // Process in main task
        fsm.process_all();
        assert_eq!(fsm.current_speed(), 1200);
        assert_eq!(actions.get_last_sensor(), 3000); // Last reading
    }
    
    #[test]
    fn test_speed_resets_on_stop() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions.clone());
        
        // Start and set speed
        fsm.dispatch(MotorEvent::START);
        fsm.dispatch(MotorEvent::STARTUP_COMPLETE);
        fsm.dispatch(MotorEvent::set_speed(2500));
        assert_eq!(fsm.current_speed(), 2500);
        
        // Stop resets speed
        fsm.dispatch(MotorEvent::STOP);
        assert_eq!(fsm.current_speed(), 0);
    }
    
    #[test]
    fn test_emergency_stop_resets_speed() {
        let actions = TestMotorActions::new();
        let mut fsm = MotorFsm::<_, 8>::new(actions.clone());
        
        // Start and set speed
        fsm.dispatch(MotorEvent::START);
        fsm.dispatch(MotorEvent::STARTUP_COMPLETE);
        fsm.dispatch(MotorEvent::set_speed(3000));
        
        // Emergency stop resets speed immediately
        fsm.dispatch(MotorEvent::EMERGENCY_STOP);
        assert_eq!(fsm.current_speed(), 0);
        assert_eq!(fsm.state(), MotorState::Stopped);
    }
}

// ============================================================================
// MAIN
// ============================================================================

fn main() {
    println!("RTIC Motor Controller FSM - Functional Test");
    println!("============================================\n");
    println!("Demonstrating events with payloads for embedded systems\n");
    
    let actions = TestMotorActions::new();
    let mut fsm = MotorFsm::<_, 16>::new(actions.clone());
    
    println!("=== Normal Operation ===");
    println!("Initial state: {:?}", fsm.state());
    
    // Simulate RTIC tasks posting events
    println!("\n[Hardware task posts: Start]");
    fsm.post(MotorEvent::START).unwrap();
    fsm.process_all();
    println!("State: {:?}", fsm.state());
    
    println!("\n[Timer ISR posts: StartupComplete]");
    fsm.post(MotorEvent::STARTUP_COMPLETE).unwrap();
    fsm.process_all();
    println!("State: {:?}", fsm.state());
    
    // ========================================================================
    // Events with payloads
    // ========================================================================

    println!("\n=== Events with Payload ===");
    println!("\n[Encoder ISR posts: SetSpeed(1500 RPM)]");
    fsm.post(MotorEvent::set_speed(1500)).unwrap();
    fsm.process_all();
    println!("Current speed: {} RPM", fsm.current_speed());
    
    println!("\n[PWM task posts: SetPwm(75%)]");
    fsm.post(MotorEvent::set_pwm(75)).unwrap();
    fsm.process_all();
    println!("PWM duty: {}%", actions.get_pwm_duty());
    
    println!("\n[ADC ISR posts: SensorReading(5000)]");
    fsm.post(MotorEvent::sensor_reading(5000)).unwrap();
    fsm.process_all();
    println!("Last sensor: {}", actions.get_last_sensor());
    println!("State: {:?} (still running, below threshold)", fsm.state());
    
    println!("\n[Speed change: SetSpeed(2500 RPM)]");
    fsm.post(MotorEvent::set_speed(2500)).unwrap();
    fsm.process_all();
    println!("Current speed: {} RPM", fsm.current_speed());
    
    // ========================================================================
    // Batch ISR events
    // ========================================================================
    
    println!("\n=== Batch ISR Events ===");
    println!("\n[Multiple ISRs posting at once...]");
    fsm.post(MotorEvent::sensor_reading(6000)).unwrap();
    fsm.post(MotorEvent::set_speed(2800)).unwrap();
    fsm.post(MotorEvent::sensor_reading(7000)).unwrap();
    fsm.post(MotorEvent::TICK).unwrap();
    println!("Queue length: {}", fsm.queue_len());
    
    let processed = fsm.process_all();
    println!("Processed {} events", processed);
    println!("Final speed: {} RPM", fsm.current_speed());
    
    // ========================================================================
    // Fault from sensor reading
    // ========================================================================
    
    println!("\n=== Fault from Sensor Payload ===");
    println!("\n[ADC ISR posts: SensorReading(15000) - overcurrent!]");
    fsm.post(MotorEvent::sensor_reading(15000)).unwrap();
    fsm.process_all();
    println!("State: {:?}", fsm.state());
    println!("Fault count: {}", fsm.fault_count());
    println!("Alarm active: {}", actions.is_alarm_active());
    
    println!("\n[Operator resets after cooling]");
    fsm.post(MotorEvent::RESET).unwrap();
    fsm.process_all();
    println!("State: {:?}", fsm.state());
    
    println!("\n=== Event Log ===");
    for (i, entry) in actions.get_log().iter().enumerate() {
        println!("  {}: {}", i + 1, entry);
    }
    
    println!("\n✅ RTIC FSM with event payloads works correctly!");
}
