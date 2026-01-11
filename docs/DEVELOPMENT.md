# Oxidate Development Guide

Guide for developers who want to contribute to or extend Oxidate.

---

## Prerequisites

- **Rust** 1.70+ (install via [rustup](https://rustup.rs/))
- **Node.js** 18+ (for Dagre layout engine)
- **cargo-bundle** (optional, for macOS packaging)
- **cargo-deb** (optional, for Linux packaging)

---

## Getting Started

```bash
# Clone the repository
git clone https://github.com/joseclaudiosilva/oxidate.git
cd oxidate

# Install JS layout dependencies
cd tools/dagre-svg-demo && npm install && cd ../..

# Run in development mode
cargo run

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run
```

---

## Project Layout

```
oxidate/
├── src/
│   ├── main.rs              # GUI entry point
│   ├── cli.rs               # CLI entry point
│   ├── lib.rs               # Library exports
│   ├── fsm/mod.rs           # Core FSM types
│   ├── parser/
│   │   ├── mod.rs           # Parser implementation
│   │   └── fsm.pest         # PEG grammar
│   └── codegen/mod.rs       # Code generators
├── tools/
│   ├── dagre-svg-demo/      # Node.js Dagre wrapper
│   │   ├── src/layout_json.mjs
│   │   └── package.json
│   ├── gen_icon.py          # Icon generator script
│   └── package/             # Packaging scripts
├── assets/
│   ├── oxidate.icns         # macOS icon
│   ├── oxidate.png          # PNG icon (512x512)
│   └── linux/
│       └── oxidate.desktop  # Linux desktop entry
├── docs/
│   ├── RELEASING.md         # Release guide
│   ├── DSL_REFERENCE.md     # DSL syntax docs
│   └── ARCHITECTURE.md      # Technical design
├── Cargo.toml
└── README.md
```

---

## Development Workflow

### 1. Making Changes to the Parser

The grammar is in `src/parser/fsm.pest`. After editing:

```bash
# The grammar is compiled at build time via pest_derive
cargo build

# Test parsing
cargo run --bin oxidate-cli -- test.fsm
```

**Tips:**
- Use `WHITESPACE` and `COMMENT` rules for automatic handling
- Test edge cases in the CLI before GUI
- Add new rules incrementally

### 2. Adding New FSM Features

1. **Update data structures** in `src/fsm/mod.rs`
2. **Update parser** in `src/parser/mod.rs` to populate new fields
3. **Update grammar** in `src/parser/fsm.pest` if needed
4. **Update code generators** in `src/codegen/mod.rs`
5. **Update GUI** in `src/main.rs` for visualization

### 3. Modifying the GUI

The GUI uses egui immediate mode. Key sections in `main.rs`:

```rust
// Editor panel
fn render_editor(&mut self, ui: &mut egui::Ui) { ... }

// Visualization panel
fn render_visualization(&mut self, ui: &mut egui::Ui) { ... }

// State rendering
fn draw_state(&self, painter: &egui::Painter, state: &State, ...) { ... }

// Transition rendering
fn draw_transition(&self, painter: &egui::Painter, ...) { ... }
```

**Hot tips:**
- Use `ctx.request_repaint()` for animation
- `egui::Painter` is the low-level drawing API
- Check `egui::Response` for interaction handling

### 4. Working with Layout

The layout engine lives in `tools/dagre-svg-demo/src/layout_json.mjs`.

**To modify layout behavior:**
1. Edit the Node.js script
2. Test with: `echo '{"graph":{},"nodes":[],"edges":[]}' | node tools/dagre-svg-demo/src/layout_json.mjs`
3. Update JSON schema in `src/main.rs` if needed

**Layout input/output structs:**
```rust
// In src/main.rs
struct JsLayoutInput { graph: JsGraphConfig, nodes: Vec<JsNodeIn>, edges: Vec<JsEdgeIn> }
struct JsLayoutOutput { nodes: HashMap<String, JsNodeOut>, edges: Vec<JsEdgeOut> }
```

---

## Testing

### Unit Tests

```bash
cargo test
```

### Manual Testing

1. **Parser:** Use the CLI to test FSM files
2. **GUI:** Run the app and try various DSL inputs
3. **Code generation:** Export and compile generated code

### Test FSM Files

Create test files in an `examples/` directory:

```
// examples/simple.fsm
fsm Simple {
    [*] --> A
    state A
    state B
    A --> B : go
}
```

---

## Debugging

### Logging

```bash
# Enable debug logging
RUST_LOG=debug cargo run

# Filter to specific module
RUST_LOG=oxidate::parser=debug cargo run
```

### Layout Issues

```bash
# Check if Node.js is found
OXIDATE_NODE=/path/to/node cargo run

# Override Dagre directory
OXIDATE_DAGRE_DIR=/path/to/tools/dagre-svg-demo cargo run
```

### Parser Debugging

Add debug prints to `src/parser/mod.rs`:

```rust
fn parse_transition(pair: pest::iterators::Pair<Rule>) -> Transition {
    eprintln!("Parsing transition: {:?}", pair.as_str());
    // ...
}
```

---

## Code Style

### Rust

- Follow standard Rust conventions
- Use `cargo fmt` before committing
- Run `cargo clippy` for lints

### Naming Conventions

| Type | Convention | Example |
|------|------------|---------|
| Types | `PascalCase` | `FsmDefinition` |
| Functions | `snake_case` | `parse_fsm()` |
| Constants | `SCREAMING_SNAKE` | `MAX_STATES` |
| Modules | `snake_case` | `codegen` |

---

## Adding a New Code Generation Target

1. Add variant to `CodegenTarget` enum:
   ```rust
   pub enum CodegenTarget {
       Standard,
       Embassy,
       Rtic,
       NewTarget,  // Add here
   }
   ```

2. Add generator function:
   ```rust
   fn generate_new_target_code(fsm: &FsmDefinition) -> String {
       // Implementation
   }
   ```

3. Wire up in `generate_rust_code_with_target()`:
   ```rust
   CodegenTarget::NewTarget => generate_new_target_code(fsm),
   ```

4. Add UI option in `render_toolbar()` if needed.

---

## Building Releases

### Development Build

```bash
cargo build
```

### Release Build

```bash
cargo build --release
```

### macOS App Bundle

```bash
cargo install cargo-bundle
cargo bundle --release
# Output: target/release/bundle/osx/Oxidate.app
```

### Linux .deb Package

```bash
cargo install cargo-deb
cargo build --release
cargo deb
# Output: target/debian/oxidate_*.deb
```

### Portable Build

```bash
chmod +x tools/package/make-portable.sh
./tools/package/make-portable.sh
# Output: dist/oxidate-<version>-<target>/
```

---

## Common Issues

### "Failed to spawn Node.js"

**Cause:** Node.js not found on PATH.

**Fix:**
```bash
# Install Node.js
brew install node  # macOS
apt install nodejs # Debian/Ubuntu

# Or set explicit path
export OXIDATE_NODE=/path/to/node
```

### "Dagre layout script not found"

**Cause:** `tools/dagre-svg-demo` not initialized or not bundled.

**Fix:**
```bash
cd tools/dagre-svg-demo && npm install
```

### Layout produces empty output

**Cause:** Dagre crashed or JSON mismatch.

**Debug:**
```bash
# Test Dagre directly
echo '{"graph":{"rankdir":"TB"},"nodes":[{"id":"A","width":100,"height":50}],"edges":[]}' | \
  node tools/dagre-svg-demo/src/layout_json.mjs
```

### Compilation for wrong target

**Cause:** Global `.cargo/config` sets a different default target.

**Fix:**
```bash
cargo build --target aarch64-apple-darwin  # or your native target
```

If you want a repo-local override (without committing a machine-specific target),
copy and edit:

```bash
cp .cargo/config.toml.example .cargo/config.toml
```

---

## Resources

- [egui documentation](https://docs.rs/egui)
- [pest book](https://pest.rs/book/)
- [Dagre wiki](https://github.com/dagrejs/dagre/wiki)
- [UML State Machine specification](https://www.omg.org/spec/UML/)
