# Oxidate Architecture

Technical architecture and design decisions for the Oxidate FSM framework.

---

## Overview

Oxidate is a Rust application with three main components:

1. **Parser** — Converts DSL text into an AST (`FsmDefinition`)
2. **GUI** — Interactive editor and visualizer (egui/eframe)
3. **Code Generator** — Produces Rust code from the AST

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   DSL Text   │────▶│    Parser    │────▶│FsmDefinition │
└──────────────┘     │   (pest)     │     │    (AST)     │
                     └──────────────┘     └──────┬───────┘
                                                 │
                     ┌───────────────────────────┼───────────────────────────┐
                     │                           │                           │
                     ▼                           ▼                           ▼
              ┌──────────────┐           ┌──────────────┐           ┌──────────────┐
              │     GUI      │           │    Layout    │           │   Codegen    │
              │   (egui)     │           │   (Dagre)    │           │   (Rust)     │
              └──────────────┘           └──────────────┘           └──────────────┘
```

---

## Module Structure

```
src/
├── main.rs          # GUI application entry point
├── cli.rs           # CLI tool entry point
├── lib.rs           # Library re-exports
├── fsm/
│   └── mod.rs       # Core data structures (FsmDefinition, State, Transition, etc.)
├── parser/
│   ├── mod.rs       # pest parser implementation
│   └── fsm.pest     # Grammar definition
└── codegen/
    └── mod.rs       # Code generation for Standard/Embassy/RTIC
```

---

## Core Data Structures

### FsmDefinition

The central AST node representing a complete state machine:

```rust
pub struct FsmDefinition {
    pub name: String,
    pub description: Option<String>,
    pub initial_state: Option<String>,
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
    pub events: Vec<Event>,
    pub choice_points: Vec<ChoicePoint>,
    pub timers: Vec<Timer>,
}
```

### State

```rust
pub struct State {
    pub name: String,
    pub description: Option<String>,
    pub state_type: StateType,           // Normal, Initial, Final, Choice
    pub entry_actions: Vec<Action>,
    pub exit_actions: Vec<Action>,
    pub internal_transitions: Vec<InternalTransition>,
    pub timer_starts: Vec<String>,
    pub timer_stops: Vec<String>,
    pub substates: Vec<State>,           // For hierarchical FSMs (future)
}
```

### Transition

```rust
pub struct Transition {
    pub source: String,
    pub target: String,
    pub event: Option<Event>,
    pub guard: Option<Guard>,
    pub action: Option<Action>,
}
```

### Supporting Types

```rust
pub struct Event { pub name: String }
pub struct Guard { pub expression: String }
pub struct Action { pub name: String, pub params: Vec<String> }

pub struct Timer {
    pub name: String,
    pub duration_ms: u32,
    pub event: Event,
    pub periodic: bool,
    pub auto_start_states: Vec<String>,
}

pub struct ChoicePoint {
    pub name: String,
    pub branches: Vec<ChoiceBranch>,
}
```

---

## Parser (pest)

The DSL parser uses [pest](https://pest.rs/), a PEG parser generator.

### Grammar Highlights (`fsm.pest`)

```pest
file = { SOI ~ fsm_definition* ~ EOI }

fsm_definition = { "fsm" ~ identifier ~ "{" ~ fsm_body ~ "}" }

transition = {
    source ~ arrow ~ target ~ (":" ~ transition_label)?
}

transition_label = {
    event? ~ guard? ~ action?
}

guard = { "[" ~ guard_expr ~ "]" }
action = { "/" ~ action_call }
```

### Parsing Flow

1. **Lexing** — pest tokenizes input according to grammar rules
2. **AST Construction** — `parse_fsm()` walks the parse tree
3. **Validation** — `FsmDefinition::validate()` checks semantic correctness

```rust
pub fn parse_fsm(input: &str) -> Result<Vec<FsmDefinition>, ParseError> {
    let pairs = FsmParser::parse(Rule::file, input)?;
    let mut fsms = Vec::new();
    
    for pair in pairs {
        if let Rule::fsm_definition = pair.as_rule() {
            fsms.push(parse_fsm_definition(pair)?);
        }
    }
    
    Ok(fsms)
}
```

---

## GUI Architecture

The GUI uses [egui](https://github.com/emilk/egui) in immediate mode.

### Main Application State

```rust
struct OxidateApp {
    // Editor
    dsl_text: String,
    parse_result: Option<Result<Vec<FsmDefinition>, String>>,
    
    // Visualization
    state_positions: HashMap<String, egui::Pos2>,
    edge_routes: HashMap<String, Vec<egui::Pos2>>,
    zoom: f32,
    pan: egui::Vec2,
    
    // Layout
    layout_config: LayoutConfig,
    layout_dirty: bool,
    
    // Simulation
    sim: SimulationState,
    debug_mode: bool,
    
    // Code generation
    codegen_target: CodegenTarget,
    generated_code: String,
}
```

### Update Loop

```rust
impl eframe::App for OxidateApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. Top toolbar
        self.render_toolbar(ctx);
        
        // 2. Left panel: DSL editor
        egui::SidePanel::left("editor").show(ctx, |ui| {
            self.render_editor(ui);
        });
        
        // 3. Central panel: Visualization
        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_visualization(ui);
        });
        
        // 4. Handle layout if needed
        if self.layout_dirty {
            self.run_layout();
        }
        
        // 5. Continuous repaint for simulation animation
        if self.debug_mode && self.sim.is_animating() {
            ctx.request_repaint();
        }
    }
}
```

---

## Layout Engine

### Design Decision: External Layout

We use the [`dagre`](https://crates.io/crates/dagre) crate, a pure-Rust port of
[dagre.js](https://github.com/dagrejs/dagre), cross-validated against it on 20
reference graphs. Earlier versions shelled out to Node.js to run dagre.js itself;
that required Node at runtime and an `npm install` step, and meant `cargo install`
did not produce a working GUI. Historical rationale for the old approach:

1. **Maturity** — Dagre is battle-tested for DAG layouts
2. **Quality** — Produces professional-looking hierarchical layouts
3. **Edge routing** — Computes orthogonal/polyline edge routes

### Layout Pipeline

```
┌───────────────┐     ┌───────────────┐     ┌───────────────┐
│ FsmDefinition │────▶│  dagre::Graph │────▶│  dagre (Rust) │
│               │     │   Input       │     │  (Dagre)      │
└───────────────┘     │   (JSON)      │     └───────┬───────┘
                      └───────────────┘             │
                                                    │ JSON
                                                    ▼
┌───────────────┐     ┌───────────────┐     ┌───────────────┐
│   Renderer    │◀────│  JsLayout     │◀────│   stdout      │
│   (egui)      │     │  Output       │     │               │
└───────────────┘     └───────────────┘     └───────────────┘
```

### JSON Protocol

**Input:**
```json
{
  "graph": { "rankdir": "TB", "nodesep": 60, "ranksep": 80 },
  "nodes": [
    { "id": "Idle", "width": 100, "height": 50 },
    { "id": "Running", "width": 120, "height": 50 }
  ],
  "edges": [
    { "v": "Idle", "w": "Running", "name": "e0", "label_width": 60, "label_height": 20 }
  ]
}
```

**Output:**
```json
{
  "nodes": {
    "Idle": { "x": 150, "y": 50, "width": 100, "height": 50 },
    "Running": { "x": 150, "y": 180, "width": 120, "height": 50 }
  },
  "edges": [
    { "v": "Idle", "w": "Running", "points": [{"x":150,"y":75}, {"x":150,"y":155}] }
  ]
}
```

### Key Constraint

> **The renderer draws ONLY what the layout engine provides.**

No heuristic edge routing in the renderer. All edge routes come from Dagre. This ensures:
- Consistent layouts
- No edge-node overlaps
- Professional appearance

---

## Code Generation

### Target Architectures

| Target | Use Case | Features |
|--------|----------|----------|
| `Standard` | Desktop/server apps | `std`, sync |
| `Embassy` | Async embedded | `no_std`, `async`, Embassy executor |
| `RTIC` | Real-time embedded | `no_std`, RTIC task model |

### Generated Code Structure

```rust
// State enum
pub enum MyFsmState {
    Idle,
    Running,
    // ...
}

// Event enum
pub enum MyFsmEvent {
    Start,
    Stop,
    // ...
}

// FSM struct
pub struct MyFsm<T: MyFsmActions> {
    state: MyFsmState,
    actions: T,
}

// Action trait (user implements)
pub trait MyFsmActions {
    fn on_enter_idle(&mut self);
    fn on_exit_running(&mut self);
    fn do_something(&mut self);
}

// Transition logic
impl<T: MyFsmActions> MyFsm<T> {
    pub fn handle_event(&mut self, event: MyFsmEvent) {
        match (self.state, event) {
            (MyFsmState::Idle, MyFsmEvent::Start) => {
                self.actions.on_exit_idle();
                self.actions.initialize();
                self.state = MyFsmState::Running;
                self.actions.on_enter_running();
            }
            // ...
        }
    }
}
```

---

## Simulation System

### State

```rust
struct SimulationState {
    current_state: Option<String>,
    history: Vec<SimHistoryEntry>,
    
    // Animation
    animating_transition: Option<AnimatingTransition>,
    anim_progress: f32,
    
    // Auto-run
    auto_run: bool,
    auto_period_s: f32,
    last_auto_step: Instant,
}
```

### Animation

When a transition fires:

1. Record the edge polyline from layout
2. Start animation timer
3. Each frame: interpolate marker position along polyline
4. At completion: update `current_state`

```rust
fn animate_marker(&mut self, ctx: &egui::Context) {
    if let Some(ref mut anim) = self.sim.animating_transition {
        let dt = ctx.input(|i| i.stable_dt);
        anim.progress += dt / ANIM_DURATION;
        
        if anim.progress >= 1.0 {
            self.sim.current_state = Some(anim.target.clone());
            self.sim.animating_transition = None;
        }
        
        ctx.request_repaint();
    }
}
```

---

## Cross-Platform Packaging

### Resource Discovery

The app locates the Dagre backend through a priority list:

```rust
fn dagre_demo_dir() -> PathBuf {
    // 1. Environment override
    if let Ok(dir) = std::env::var("OXIDATE_DAGRE_DIR") { ... }
    
    // 2. macOS bundle: Contents/Resources/tools/dagre-svg-demo
    // 3. Generic resources: <exe>/resources/tools/dagre-svg-demo
    // 4. Sibling: <exe>/tools/dagre-svg-demo
    // 5. AppImage: <AppDir>/usr/share/oxidate/tools/dagre-svg-demo
    // 6. Dev fallback: CARGO_MANIFEST_DIR/tools/dagre-svg-demo
}
```

### Package Formats

| Platform | Format | Tool |
|----------|--------|------|
| macOS | `.app` bundle | `cargo-bundle` |
| Linux | `.deb` | `cargo-deb` |
| Linux | Portable folder | Manual / script |
| Windows | `.zip` portable | Manual / script |

---

## Future Considerations

### Potential Improvements

1. **Pure-Rust Layout** — Eliminate Node.js dependency with a Rust Dagre port
2. **Hierarchical States** — Nested state machines (partially supported in AST)
3. **History States** — Remember sub-state on re-entry
4. **Parallel States** — Orthogonal regions
5. **SCXML Import/Export** — Industry standard interchange
6. **Live Code Reload** — Hot-reload generated code in simulation

### Performance Notes

- Layout is synchronous (blocks UI during Dagre call)
- Could be moved to async/background thread for large FSMs
- Current implementation handles ~100 states smoothly
