# Oxidate Pro - Premium Features

## Overview

Oxidate is available in two editions:

### Community Edition (MIT License)

- Visual FSM editor with egui
- DSL parser (.fsm files)
- Standard Rust code generation
- JSON/DOT export
- Full source code available

### Pro Edition (Commercial License)

Contact:
- Issues: https://github.com/JoseClaudioSJr/Oxidate/issues
- Discussions: https://github.com/JoseClaudioSJr/Oxidate/discussions

#### Embassy Code Generation

Generate async embedded code using the Active Object pattern:

- Event channels with `embassy-sync`
- Async state machine tasks
- Software timers with `embassy-time`
- ISR-safe event posting
- Events with typed payloads
- Run-to-completion semantics

```rust
// Generated Embassy code example
pub struct BlinkActiveObject<T: BlinkActions> {
    state: BlinkState,
    context: T,
}

impl<T: BlinkActions> BlinkActiveObject<T> {
    pub async fn run(&mut self, receiver: Receiver<...>) {
        self.init();
        loop {
            let event = receiver.receive().await;
            self.dispatch(event);
        }
    }
}
```

#### RTIC Code Generation

Generate real-time embedded code for RTIC framework:

- Event queues with `heapless::spsc::Queue`
- Priority-based task processing
- Guards with runtime conditions
- Extended state variables
- Fault handling patterns
- Events with payload

```rust
// Generated RTIC code example
pub struct MotorFsm<T: MotorActions, const N: usize = 8> {
    state: MotorState,
    event_queue: Queue<MotorEvent, N>,
    context: T,
}

impl<T: MotorActions, const N: usize> MotorFsm<T, N> {
    pub fn post(&mut self, event: MotorEvent) -> Result<(), MotorEvent> {
        self.event_queue.enqueue(event)
    }
    
    pub fn process_all(&mut self) -> u32 {
        // Process all queued events
    }
}
```

#### Additional Pro Features

- HSM (Hierarchical State Machines)
- Composite states
- History states
- Orthogonal regions
- Priority support
- Code templates customization

## Pricing

- **Individual Developer**: $99/year
- **Team (5 devs)**: $399/year
- **Enterprise**: Contact for pricing

## Getting Started with Pro

1. Contact us to purchase a license and receive download/access instructions:
    - https://github.com/JoseClaudioSJr/Oxidate/discussions
    - https://github.com/JoseClaudioSJr/Oxidate/issues
2. Download the Oxidate Pro binary/crate (link provided after purchase)
3. Generate code:

```bash
oxidate-pro generate --target embassy your_fsm.fsm
oxidate-pro generate --target rtic your_fsm.fsm
```

## Support

- Community Edition: https://github.com/JoseClaudioSJr/Oxidate/issues
- Pro Edition: https://github.com/JoseClaudioSJr/Oxidate/discussions
- Enterprise: Priority support + Slack channel

## FAQ

**Q: Can I use Community Edition commercially?**
A: Yes! MIT license allows commercial use for Standard Rust target.

**Q: What if I need Embassy/RTIC but can't afford Pro?**
A: Contact us for student/startup discounts.

**Q: Can I contribute to Community Edition?**
A: Yes! PRs welcome on GitHub.
