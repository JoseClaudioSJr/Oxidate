//! Oxidate GUI - FSM Visualizer
//! Interactive GUI for creating and visualizing Finite State Machines

use eframe::egui;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use oxidate_fsm::fsm::{self, FsmDefinition, StateType};
use oxidate_fsm::parser::parse_fsm;
use oxidate_fsm::codegen::{generate_rust_code_with_target, CodegenTarget};

/// Renders validation errors as a Rust comment block, so the code panel shows
/// what is wrong instead of silently keeping the previous FSM's output.
fn format_codegen_errors(fsm_name: &str, errors: &[String]) -> String {
    let mut out = format!("// Cannot generate code for FSM '{}':\n", fsm_name);
    for error in errors {
        out.push_str(&format!("//   - {}\n", error));
    }
    out
}


fn oxidate_icon() -> egui::IconData {
    // Simple generated icon (64x64): dark background + orange "oxidation" ring.
    // Avoids external assets and works cross-platform.
    let w: u32 = 64;
    let h: u32 = 64;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let cx = (w as f32 - 1.0) * 0.5;
    let cy = (h as f32 - 1.0) * 0.5;
    let r_outer = 26.0;
    let r_inner = 18.0;

    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let d = (dx * dx + dy * dy).sqrt();

            // Base background.
            let mut r = 20u8;
            let mut g = 24u8;
            let mut b = 30u8;
            let mut a = 255u8;

            // Ring with a subtle vertical gradient.
            if d >= r_inner && d <= r_outer {
                let t = ((y as f32) / (h as f32 - 1.0)).clamp(0.0, 1.0);
                let rr = (240.0 - 40.0 * t) as u8;
                let gg = (140.0 - 30.0 * t) as u8;
                let bb = (40.0 - 10.0 * t) as u8;
                r = rr;
                g = gg;
                b = bb;
            }

            // Inner fill slightly lighter than background.
            if d < r_inner {
                r = 34;
                g = 40;
                b = 52;
            }

            // Soft outer alpha edge.
            if d > r_outer {
                let falloff = (d - r_outer).clamp(0.0, 2.0);
                a = (255.0 * (1.0 - falloff / 2.0)) as u8;
            }

            let idx = ((y * w + x) * 4) as usize;
            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = a;
        }
    }

    egui::IconData { rgba, width: w, height: h }
}



#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayoutDirection {
    TB,
    LR,
}

#[derive(Clone, Debug)]
struct LayoutConfig {
    direction: LayoutDirection,
    nodesep: f32,
    ranksep: f32,
    edgesep: f32,
    marginx: f32,
    marginy: f32,
    edge_label_font_size: f32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            direction: LayoutDirection::TB,
            nodesep: 60.0,
            ranksep: 90.0,
            edgesep: 20.0,
            marginx: 40.0,
            marginy: 40.0,
            edge_label_font_size: 12.0,
        }
    }
}

#[derive(Clone, Debug)]
struct LayoutedEdge {
    v: String,
    w: String,
    /// If this edge is part of a concrete transition, this is that transition's index in `FsmDefinition::transitions`.
    transition_index: Option<usize>,
    points: Vec<egui::Pos2>,
    transition_type: TransitionType,
}

#[derive(Clone, Debug)]
struct LayoutedLabel {
    pos: egui::Pos2,
    text: String,
}

#[derive(Clone, Debug, Default)]
struct LayoutedDiagram {
    edges: Vec<LayoutedEdge>,
    labels: Vec<LayoutedLabel>,
}

fn main() -> eframe::Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("Oxidate - FSM Visualizer")
            .with_icon(oxidate_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "Oxidate",
        options,
        Box::new(|cc| Ok(Box::new(OxidateApp::new(cc)))),
    )
}

struct OxidateApp {
    /// Source code editor content (all FSMs combined)
    source_code: String,
    /// Individual FSM source codes (extracted from source_code)
    fsm_sources: Vec<String>,
    /// Generated Rust code
    generated_code: String,
    /// Parsed FSM definitions
    fsms: Vec<FsmDefinition>,
    /// Parse error message
    error_message: Option<String>,
    /// Selected FSM index
    selected_fsm: usize,
    /// State positions for visualization (calculated automatically)
    state_positions: HashMap<String, egui::Pos2>,
    /// Latest engine-computed layout (nodes are mirrored into state_positions)
    layout: Option<LayoutedDiagram>,
    /// Layout configuration (engine parameters)
    layout_config: LayoutConfig,
    /// Whether we must recompute layout using the engine
    layout_dirty: bool,
    /// Show code panel
    show_code_panel: bool,
    /// Show generated code panel
    show_generated_panel: bool,
    /// Zoom level
    zoom: f32,
    /// Pan offset
    pan_offset: egui::Vec2,
    /// Code generation target
    codegen_target: CodegenTarget,
    /// New FSM dialog state
    show_new_fsm_dialog: bool,
    /// New FSM name input
    new_fsm_name: String,

    /// Debug/simulation mode
    sim: Simulator,
}

#[derive(Clone, Debug)]
struct Simulator {
    enabled: bool,
    running: bool,
    speed: f32,

    current_state: Option<String>,
    queued_events: std::collections::VecDeque<String>,
    event_input: String,

    auto_tick: bool,
    /// Comma-separated event names, cycled one per tick. A single name repeats
    /// forever, which only drives machines whose transitions all share an event.
    auto_event: String,
    /// What `suggest_event_sequence` last offered. Lets Reset refresh the
    /// suggestion when the user has not typed their own sequence.
    suggested_events: String,
    auto_cursor: usize,
    auto_period_s: f32,
    auto_accum_s: f32,

    last_frame: Option<Instant>,
    last_fired: Option<SimFired>,
    log: Vec<String>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // deserialisation targets; not every field is read
struct SimFired {
    transition_index: Option<usize>,
    from: String,
    to: String,
    label: String,
    started_at: Instant,
    duration_s: f32,
}

impl Default for Simulator {
    fn default() -> Self {
        Self {
            enabled: false,
            running: false,
            speed: 1.0,
            current_state: None,
            queued_events: std::collections::VecDeque::new(),
            event_input: String::new(),
            auto_tick: false,
            // Matches examples/traffic_light.fsm, so Auto + Run cycles out of
            // the box. The previous default, "timer_expired", matched nothing
            // in any shipped example.
            auto_event: "TimerExpired".to_string(),
            suggested_events: "TimerExpired".to_string(),
            auto_cursor: 0,
            auto_period_s: 1.0,
            auto_accum_s: 0.0,
            last_frame: None,
            last_fired: None,
            log: Vec::new(),
        }
    }
}

impl OxidateApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            source_code: DEFAULT_FSM_CODE.to_string(),
            fsm_sources: Vec::new(),
            generated_code: String::new(),
            fsms: Vec::new(),
            error_message: None,
            selected_fsm: 0,
            state_positions: HashMap::new(),
            layout: None,
            layout_config: LayoutConfig::default(),
            layout_dirty: true,
            show_code_panel: true,
            show_generated_panel: true,
            zoom: 1.0,
            pan_offset: egui::Vec2::ZERO,
            // Standard is the only target that generates working code; the
            // others emit a stub, so defaulting to one of them meant a new
            // user's very first output was a placeholder.
            codegen_target: CodegenTarget::Standard,
            show_new_fsm_dialog: false,
            new_fsm_name: String::new(),
            sim: Simulator::default(),
        };
        // Parse the default example on startup
        app.parse_source();
        app
    }

    fn parse_source(&mut self) {
        // Extract individual FSM source blocks
        self.extract_fsm_sources();
        
        match parse_fsm(&self.source_code) {
            Ok(fsms) => {
                self.fsms = fsms;
                self.error_message = None;
                if !self.fsms.is_empty() {
                    self.selected_fsm = 0; // Reset to first FSM
                    // IMPORTANT: layout is engine-driven. Defer computation to `update()`
                    // so we can measure fonts for accurate label sizes.
                    self.layout_dirty = true;
                    // Generate code for the selected FSM
                    self.regenerate_code();

                    // Reset simulator to align with the newly parsed FSM.
                    self.sim.current_state = None;
                    self.sim.queued_events.clear();
                    self.sim.last_fired = None;
                    self.sim.log.clear();
                } else {
                    self.generated_code = "// No FSMs parsed".to_string();
                }
            }
            Err(e) => {
                self.error_message = Some(e.to_string());
                self.generated_code = format!("// Parse error: {}", e);
            }
        }
    }
    
    /// Extract individual FSM source code blocks from the combined source
    fn extract_fsm_sources(&mut self) {
        self.fsm_sources.clear();
        
        let mut current_block = String::new();
        let mut brace_count = 0;
        let mut in_fsm = false;
        let mut pending_comments = String::new();
        
        for line in self.source_code.lines() {
            let trimmed = line.trim();
            
            // Collect comments before FSM
            if !in_fsm && (trimmed.starts_with("//") || trimmed.is_empty()) {
                pending_comments.push_str(line);
                pending_comments.push('\n');
                continue;
            }
            
            // Start of FSM block
            if trimmed.starts_with("fsm ") {
                in_fsm = true;
                current_block = pending_comments.clone();
                pending_comments.clear();
            }
            
            if in_fsm {
                current_block.push_str(line);
                current_block.push('\n');
                
                // Count braces
                brace_count += line.chars().filter(|&c| c == '{').count() as i32;
                brace_count -= line.chars().filter(|&c| c == '}').count() as i32;
                
                // End of FSM block
                if brace_count == 0 && current_block.contains('{') {
                    self.fsm_sources.push(current_block.trim().to_string());
                    current_block.clear();
                    in_fsm = false;
                }
            } else {
                pending_comments.clear();
            }
        }
        
        // Handle any remaining block
        if !current_block.trim().is_empty() {
            self.fsm_sources.push(current_block.trim().to_string());
        }
    }
    
    /// Update source_code from individual fsm_sources
    fn rebuild_source_code(&mut self) {
        self.source_code = self.fsm_sources.join("\n\n");
    }
    
    fn regenerate_code(&mut self) {
        if let Some(fsm) = self.fsms.get(self.selected_fsm) {
            self.generated_code = generate_rust_code_with_target(fsm, self.codegen_target)
                .unwrap_or_else(|errors| format_codegen_errors(&fsm.name, &errors));
        } else {
            self.generated_code = format!("// No FSM at index {}", self.selected_fsm);
        }
    }

    fn mark_layout_dirty(&mut self) {
        self.layout_dirty = true;
    }

    fn measure_text(ctx: &egui::Context, text: &str, font_size: f32) -> egui::Vec2 {
        let font_id = egui::FontId::proportional(font_size);
        ctx.fonts(|fonts| {
            let galley = fonts.layout_no_wrap(text.to_owned(), font_id, egui::Color32::WHITE);
            galley.size()
        })
    }


    /// Walks the machine from its initial state and returns the events needed to
    /// drive it, as a comma-separated list for the Auto field.
    ///
    /// Derived from the FSM rather than hardcoded per example, so a machine the
    /// user just wrote gets a working sequence too. Prefers transitions it has
    /// not taken yet, which tends to cover the whole graph before repeating.
    fn suggest_event_sequence(fsm: &FsmDefinition) -> String {
        let Some(initial) = fsm.initial_state.clone() else {
            return String::new();
        };

        let mut sequence: Vec<String> = Vec::new();
        let mut taken: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut current = initial.clone();

        // Bounded: a machine with a trap state would otherwise never terminate.
        for _ in 0..24 {
            let outgoing: Vec<(usize, &fsm::Transition)> = fsm
                .transitions
                .iter()
                .enumerate()
                .filter(|(_, t)| t.source == current && t.event.is_some())
                .collect();

            if outgoing.is_empty() {
                break;
            }

            // An untaken transition first; otherwise fall back to the first one
            // so the walk keeps moving instead of dead-ending.
            let (index, transition) = outgoing
                .iter()
                .find(|(i, _)| !taken.contains(i))
                .copied()
                .unwrap_or(outgoing[0]);

            taken.insert(index);
            if let Some(event) = &transition.event {
                sequence.push(event.name.clone());
            }
            current = transition.target.clone();

            // Back where we started having used every transition: a full cycle.
            if current == initial && taken.len() == fsm.transitions.len() {
                break;
            }
        }

        sequence.join(", ")
    }


    /// Pure-Rust layout, via the `dagre` crate — a port of dagre.js.
    ///
    /// Replaces the previous path, which spawned Node to run dagre.js and
    /// parsed its JSON back. Same algorithm, no subprocess, nothing to ship
    /// alongside the binary.
    ///
    /// Edge labels are given a size on the graph, so the layout reserves room
    /// for them rather than leaving them to land on top of a state box.
    fn compute_layout_native(&mut self, fsm: &FsmDefinition) {
        use dagre::graph::{Graph, GraphOptions};
        use dagre::layout::types::LabelPos;
        use dagre::{layout, EdgeLabel, LayoutOptions, NodeLabel, RankDir};

        let mut g: Graph<NodeLabel, EdgeLabel> = Graph::with_options(GraphOptions {
            directed: true,
            multigraph: true,
            compound: false,
        });

        for state in &fsm.states {
            let size = estimate_state_size(state);
            let mut node = NodeLabel::default();
            node.width = size.x as f64;
            node.height = size.y as f64;
            g.set_node(state.name.as_str(), Some(node));
        }

        // The initial pseudo-state renders as a small filled circle.
        let initial = fsm.initial_state.clone();
        if initial.is_some() {
            let mut node = NodeLabel::default();
            node.width = 16.0;
            node.height = 16.0;
            g.set_node("[*]", Some(node));
        }

        // dagre needs a distinct name per parallel edge; the index doubles as
        // the way back to the transition once layout has run.
        let mut edge_keys: Vec<(usize, String, String, String)> = Vec::new();

        for (index, transition) in fsm.transitions.iter().enumerate() {
            if transition.source == "[*]" {
                continue; // handled as the initial pseudo-edge below
            }
            if g.node(transition.source.as_str()).is_none()
                || g.node(transition.target.as_str()).is_none()
            {
                continue; // endpoint not laid out (e.g. a choice point)
            }

            // Break long labels across lines: the event on one line, the guard
            // on the next. Narrower labels leave the layout more room to work
            // with, and dagre needs the size to reserve space for them.
            let text = format_label_text(&transition.label());
            let mut label = EdgeLabel::default();
            // `EdgeLabel::default()` uses LabelPos::Right, which makes dagre add
            // `labeloffset` to the edge width and then shift the label sideways.
            // The route detours through that inflated point, producing a visible
            // kink beside every label. Centred, the label sits on the line and
            // the route stays straight; the label's opaque background hides the
            // segment underneath.
            label.labelpos = LabelPos::Center;
            if !text.is_empty() {
                let font = self.layout_config.edge_label_font_size as f64;
                let widest = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
                let line_count = text.lines().count().max(1);
                label.width = widest as f64 * font * 0.55;
                label.height = line_count as f64 * font * 1.6;
            }

            let name = format!("tr_{index}");
            g.set_edge(
                transition.source.as_str(),
                transition.target.as_str(),
                Some(label),
                Some(name.as_str()),
            );
            edge_keys.push((
                index,
                transition.source.clone(),
                transition.target.clone(),
                name,
            ));
        }

        if let Some(target) = initial.as_ref() {
            if g.node(target.as_str()).is_some() {
                g.set_edge(
                    "[*]",
                    target.as_str(),
                    Some(EdgeLabel::default()),
                    Some("__initial"),
                );
            }
        }

        layout(
            &mut g,
            Some(LayoutOptions {
                rankdir: match self.layout_config.direction {
                    LayoutDirection::TB => RankDir::TB,
                    LayoutDirection::LR => RankDir::LR,
                },
                nodesep: self.layout_config.nodesep as f64,
                ranksep: self.layout_config.ranksep as f64,
                edgesep: self.layout_config.edgesep as f64,
                marginx: self.layout_config.marginx as f64,
                marginy: self.layout_config.marginy as f64,
                ..Default::default()
            }),
        );

        // The renderer positions everything relative to the canvas centre, so
        // shift the whole diagram to be centred on the origin.
        let mut raw: std::collections::HashMap<String, egui::Pos2> =
            std::collections::HashMap::new();
        let mut min = egui::pos2(f32::MAX, f32::MAX);
        let mut max = egui::pos2(f32::MIN, f32::MIN);

        let mut record = |name: &str, n: &NodeLabel| {
            if let (Some(x), Some(y)) = (n.x, n.y) {
                let p = egui::pos2(x as f32, y as f32);
                raw.insert(name.to_string(), p);
                min = egui::pos2(min.x.min(p.x), min.y.min(p.y));
                max = egui::pos2(max.x.max(p.x), max.y.max(p.y));
            }
        };
        for state in &fsm.states {
            if let Some(n) = g.node(state.name.as_str()) {
                record(&state.name, n);
            }
        }
        if initial.is_some() {
            if let Some(n) = g.node("[*]") {
                record("[*]", n);
            }
        }
        drop(record);

        if raw.is_empty() {
            self.state_positions.clear();
            self.layout = Some(LayoutedDiagram::default());
            return;
        }
        let centre = egui::vec2((min.x + max.x) * 0.5, (min.y + max.y) * 0.5);

        self.state_positions.clear();
        for (name, p) in &raw {
            self.state_positions.insert(name.clone(), *p - centre);
        }

        // The boxes as drawn, in the same centred space as the routes. Used to
        // clip edge endpoints onto the border instead of trusting the numbers
        // to line up.
        let mut boxes: std::collections::HashMap<String, egui::Rect> =
            std::collections::HashMap::new();
        for state in &fsm.states {
            if let Some(&p) = raw.get(&state.name) {
                boxes.insert(
                    state.name.clone(),
                    egui::Rect::from_center_size(p - centre, estimate_state_size(state)),
                );
            }
        }
        if let Some(&p) = raw.get("[*]") {
            boxes.insert(
                "[*]".to_string(),
                egui::Rect::from_center_size(p - centre, egui::vec2(16.0, 16.0)),
            );
        }

        let mut edges: Vec<LayoutedEdge> = Vec::new();
        let mut labels: Vec<LayoutedLabel> = Vec::new();
        let mut self_loops = 0usize;
        // (index into `edges`, text, forced position for self-loops)
        let mut pending_labels: Vec<(usize, String, Option<egui::Pos2>)> = Vec::new();

        for (index, source, target, name) in &edge_keys {
            let Some(e) = g.edge(source.as_str(), target.as_str(), Some(name)) else {
                continue;
            };

            let raw_points: Vec<egui::Pos2> = e
                .points
                .iter()
                .map(|p| egui::pos2(p.x as f32, p.y as f32) - centre)
                .collect();
            let transition = &fsm.transitions[*index];
            let text = format_label_text(&transition.label());

            // Only 0/90/180 degree segments: no diagonals.
            let mut points = orthogonalise(&raw_points, self.layout_config.direction, *index);
            let mut self_loop_label: Option<egui::Pos2> = None;

            // A self-edge gets its own loop: dagre's geometry for these does not
            // survive being squared off.
            if source == target {
                if let Some(&rect) = boxes.get(source) {
                    // The loop has to enclose its label, so its width depends on
                    // the label's.
                    let label_w =
                        label_box_width(&text, self.layout_config.edge_label_font_size);
                    let reach =
                        rect.right() + 20.0 + label_w + self_loops as f32 * 18.0;
                    points = self_loop_route(rect, reach);
                    self_loop_label = Some(egui::pos2(
                        rect.right() + 10.0 + label_w * 0.5,
                        rect.center().y,
                    ));
                    self_loops += 1;
                }
            } else if let (Some(&from), Some(&to)) = (boxes.get(source), boxes.get(target)) {
                points = clip_route_to_boxes(&points, from, to);
            }

            if points.len() < 2 {
                continue;
            }

            // "Reverse" means the target sits earlier along the rank direction,
            // which is what the renderer colours differently.
            let is_reverse = match (raw.get(source), raw.get(target)) {
                (Some(a), Some(b)) => match self.layout_config.direction {
                    LayoutDirection::TB => b.y < a.y,
                    LayoutDirection::LR => b.x < a.x,
                },
                _ => false,
            };

            // Positioned once the routes are final: see below.
            pending_labels.push((edges.len(), text, self_loop_label));

            edges.push(LayoutedEdge {
                v: source.clone(),
                w: target.clone(),
                transition_index: Some(*index),
                points,
                transition_type: classify_transition(transition, is_reverse),
            });
        }

        if let Some(target) = initial.as_ref() {
            if let Some(e) = g.edge("[*]", target.as_str(), Some(&"__initial".to_string())) {
                let raw_points: Vec<egui::Pos2> = e
                    .points
                    .iter()
                    .map(|p| egui::pos2(p.x as f32, p.y as f32) - centre)
                    .collect();
                let mut points = orthogonalise(&raw_points, self.layout_config.direction, 0);
                if let (Some(&from), Some(&to)) = (boxes.get("[*]"), boxes.get(target.as_str())) {
                    points = clip_route_to_boxes(&points, from, to);
                }
                if points.len() >= 2 {
                    edges.push(LayoutedEdge {
                        v: "[*]".to_string(),
                        w: target.clone(),
                        transition_index: None,
                        points,
                        transition_type: TransitionType::Forward,
                    });
                }
            }
        }

        // Even out where edges meet each box, then anchor every label on the
        // geometry that actually got drawn.
        distribute_endpoints(&mut edges, &boxes);
        separate_overlapping_runs(&mut edges);

        // A self-loop's label position is fixed by the loop's own geometry; the
        // rest are chosen from candidates below.
        let mut owners: Vec<usize> = Vec::new();
        let mut fixed: Vec<bool> = Vec::new();
        for (edge_index, text, forced) in pending_labels {
            if text.is_empty() {
                continue;
            }
            let pos = forced.unwrap_or_else(|| label_anchor(&edges[edge_index].points));
            labels.push(LayoutedLabel { pos, text });
            owners.push(edge_index);
            fixed.push(forced.is_some());
        }

        let routes: Vec<Vec<egui::Pos2>> = edges.iter().map(|e| e.points.clone()).collect();
        place_labels(
            &mut labels,
            &routes,
            &owners,
            &fixed,
            &boxes,
            self.layout_config.edge_label_font_size,
        );

        self.layout = Some(LayoutedDiagram { edges, labels });
    }

    
    
    /// Create new FSMs with the given names (comma or space separated)
    fn create_new_fsms(&mut self, names_input: &str) {
        // Parse names - split by comma, semicolon, or newline
        let names: Vec<&str> = names_input
            .split(|c| c == ',' || c == ';' || c == '\n')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && s.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false))
            .collect();
        
        if names.is_empty() {
            return;
        }
        
        let mut all_fsms = String::new();
        
        for (idx, name) in names.iter().enumerate() {
            // Clean name - remove spaces, keep only valid identifier chars
            let clean_name: String = name
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            
            if clean_name.is_empty() {
                continue;
            }
            
            // Make first char uppercase (PascalCase)
            let pascal_name = {
                let mut chars = clean_name.chars();
                match chars.next() {
                    None => continue,
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            };
            
            if idx > 0 {
                all_fsms.push_str("\n\n");
            }
            
            all_fsms.push_str(&format!(r#"// {name} State Machine

fsm {name} {{
    [*] -> Idle

    state Idle {{
        entry / initialize
    }}
    
    state Active {{
        entry / on_activate
        exit / on_deactivate
    }}
    
    state Error {{
        entry / handle_error
    }}

    Idle -> Active : start
    Active -> Idle : stop
    Active -> Error : fault
    Error -> Idle : reset
}}"#, name = pascal_name));
        }
        
        if !all_fsms.is_empty() {
            all_fsms.insert_str(0, "// State Machines - Created with Oxidate FSM Visualizer\n\n");
            self.source_code = all_fsms;
            self.parse_source();
        }
    }
    
    /// Add a new FSM to existing code
    fn add_new_fsm(&mut self, name: &str) {
        let clean_name: String = name
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        
        if clean_name.is_empty() {
            return;
        }
        
        let pascal_name = {
            let mut chars = clean_name.chars();
            match chars.next() {
                None => return,
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        };
        
        let new_fsm = format!(r#"

// {name} State Machine

fsm {name} {{
    [*] -> Idle

    state Idle {{
        entry / initialize
    }}
    
    state Active {{
        entry / on_activate
        exit / on_deactivate
    }}

    Idle -> Active : start
    Active -> Idle : stop
}}"#, name = pascal_name);
        
        self.source_code.push_str(&new_fsm);
        self.parse_source();
    }
    
    /// Export all FSMs to a folder with autogen files
    fn export_all_fsms_to_folder(&self, folder: &std::path::Path) {
        use std::io::Write;
        
        // Create autogen subfolder
        let autogen_folder = folder.join("autogen");
        let _ = std::fs::create_dir_all(&autogen_folder);
        
        // Generate mod.rs for autogen
        let mut mod_content = String::from("//! Auto-generated FSM code\n//! DO NOT EDIT - Generated by Oxidate\n\n");
        
        for fsm in &self.fsms {
            let snake_name = to_snake_case(&fsm.name);
            
            // Generate code for each target. A definition that fails
            // validation is skipped rather than written out broken.
            let code = match generate_rust_code_with_target(fsm, self.codegen_target) {
                Ok(code) => code,
                Err(errors) => {
                    eprintln!("{}", format_codegen_errors(&fsm.name, &errors));
                    continue;
                }
            };
            
            // Write the FSM file
            let filename = format!("{}.rs", snake_name);
            let filepath = autogen_folder.join(&filename);
            
            if let Ok(mut file) = std::fs::File::create(&filepath) {
                let header = format!(
                    "//! Auto-generated code for {} FSM\n//! DO NOT EDIT - Generated by Oxidate\n//! Target: {:?}\n\n",
                    fsm.name, self.codegen_target
                );
                let _ = file.write_all(header.as_bytes());
                let _ = file.write_all(code.as_bytes());
            }
            
            // Add to mod.rs
            mod_content.push_str(&format!("pub mod {};\n", snake_name));
        }
        
        // Also export pub use statements
        mod_content.push_str("\n// Re-exports\n");
        for fsm in &self.fsms {
            let snake_name = to_snake_case(&fsm.name);
            mod_content.push_str(&format!("pub use {}::*;\n", snake_name));
        }
        
        // Write mod.rs
        let mod_path = autogen_folder.join("mod.rs");
        let _ = std::fs::write(&mod_path, mod_content);
        
        // Write a README
        let readme = format!(
            "# Auto-generated FSM Code\n\n\
            Generated by Oxidate FSM Visualizer\n\n\
            ## Files\n\n\
            - `mod.rs` - Module declarations\n\
            {}\n\n\
            ## Usage\n\n\
            Add to your `lib.rs` or `main.rs`:\n\n\
            ```rust\n\
            mod autogen;\n\
            use autogen::*;\n\
            ```\n\n\
            ## Target: {:?}\n",
            self.fsms.iter()
                .map(|f| format!("- `{}.rs` - {} state machine", to_snake_case(&f.name), f.name))
                .collect::<Vec<_>>()
                .join("\n"),
            self.codegen_target
        );
        let readme_path = autogen_folder.join("README.md");
        let _ = std::fs::write(&readme_path, readme);
    }

    fn sim_reset_to_initial(&mut self, fsm: &FsmDefinition) {
        self.sim.queued_events.clear();
        self.sim.auto_accum_s = 0.0;
        self.sim.auto_cursor = 0;
        self.sim.last_fired = None;
        self.sim.last_frame = None;

        // Offer a sequence that actually drives *this* machine. Only when the
        // field is untouched, so a sequence the user typed is never clobbered.
        let suggested = Self::suggest_event_sequence(fsm);
        if !suggested.is_empty()
            && (self.sim.auto_event.trim().is_empty()
                || self.sim.auto_event == self.sim.suggested_events)
        {
            self.sim.auto_event = suggested.clone();
        }
        self.sim.suggested_events = suggested;

        if let Some(initial) = &fsm.initial_state {
            self.sim.current_state = Some(initial.clone());
            self.sim.log.push(format!("reset → {initial}"));
        } else if let Some(first) = fsm.states.first() {
            self.sim.current_state = Some(first.name.clone());
            self.sim.log.push(format!("reset → {} (fallback)", first.name));
        } else {
            self.sim.current_state = None;
            self.sim.log.push("reset → <no states>".to_string());
        }
    }

    fn sim_post_event(&mut self, event_name: impl Into<String>) {
        let name = event_name.into();
        if name.trim().is_empty() {
            return;
        }
        self.sim.queued_events.push_back(name);
    }

    fn sim_step(&mut self, fsm: &FsmDefinition) {
        if self.sim.current_state.is_none() {
            self.sim_reset_to_initial(fsm);
        }
        let Some(event) = self.sim.queued_events.pop_front() else {
            return;
        };
        let Some(current) = self.sim.current_state.clone() else {
            return;
        };

        // Try external transitions first (from the FSM transition list).
        if let Some((t_idx, t)) = fsm
            .transitions
            .iter()
            .enumerate()
            .find(|(_, t)| t.source == current && t.event.as_ref().is_some_and(|e| e.name == event))
        {
            let label = t.label();
            self.sim.log.push(format!("{current} --{event}--> {}", t.target));
            let started_at = Instant::now();
            self.sim.last_fired = Some(SimFired {
                transition_index: Some(t_idx),
                from: current.clone(),
                to: t.target.clone(),
                label,
                started_at,
                duration_s: (0.7 / self.sim.speed.max(0.05)).clamp(0.15, 3.0),
            });
            self.sim.current_state = Some(t.target.clone());
            return;
        }

        // Then internal transitions (stay in state; no edge animation).
        if let Some(state) = fsm.states.iter().find(|s| s.name == current) {
            if let Some(internal) = state
                .internal_transitions
                .iter()
                .find(|t| t.event.as_ref().is_some_and(|e| e.name == event))
            {
                let label = internal.label();
                self.sim.log.push(format!("{current} --{event}--> {current} (internal)"));
                let started_at = Instant::now();
                self.sim.last_fired = Some(SimFired {
                    transition_index: None,
                    from: current.clone(),
                    to: current.clone(),
                    label,
                    started_at,
                    duration_s: (0.4 / self.sim.speed.max(0.05)).clamp(0.10, 2.0),
                });
                return;
            }
        }

        self.sim.log.push(format!("{current}: no transition for event '{event}'"));
    }

    fn polyline_point_at(points: &[egui::Pos2], t: f32) -> Option<egui::Pos2> {
        if points.len() < 2 {
            return None;
        }
        let mut lengths: Vec<f32> = Vec::with_capacity(points.len() - 1);
        let mut total = 0.0f32;
        for i in 0..points.len() - 1 {
            let d = points[i].distance(points[i + 1]);
            lengths.push(d);
            total += d;
        }
        if total <= 0.0001 {
            return Some(points[0]);
        }
        let mut target = (t.clamp(0.0, 1.0)) * total;
        for i in 0..lengths.len() {
            let seg = lengths[i];
            if target <= seg {
                let a = points[i];
                let b = points[i + 1];
                let alpha = if seg <= 0.0001 { 0.0 } else { target / seg };
                return Some(egui::pos2(
                    a.x + (b.x - a.x) * alpha,
                    a.y + (b.y - a.y) * alpha,
                ));
            }
            target -= seg;
        }
        Some(*points.last().unwrap())
    }

    fn sim_route_for_transition(layout: &LayoutedDiagram, transition_index: usize, from: &str, to: &str) -> Option<Vec<egui::Pos2>> {
        let tr_node = format!("__tr_{transition_index}");
        let a = layout
            .edges
            .iter()
            .find(|e| e.transition_index == Some(transition_index) && e.v == from && e.w == tr_node);
        let b = layout
            .edges
            .iter()
            .find(|e| e.transition_index == Some(transition_index) && e.v == tr_node && e.w == to);

        match (a, b) {
            (Some(a), Some(b)) => {
                let mut pts = a.points.clone();
                if let Some(first_b) = b.points.first().copied() {
                    if pts.last().copied().is_some_and(|last| last.distance(first_b) < 0.01) {
                        pts.pop();
                    }
                }
                pts.extend_from_slice(&b.points);
                Some(pts)
            }
            _ => {
                // Fallback: pick the longest segment we can find for that transition.
                layout
                    .edges
                    .iter()
                    .filter(|e| e.transition_index == Some(transition_index))
                    .max_by(|a, b| a.points.len().cmp(&b.points.len()))
                    .map(|e| e.points.clone())
            }
        }
    }
}

/// Convert PascalCase to snake_case
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

impl eframe::App for OxidateApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("➕ New FSM...").clicked() {
                        self.show_new_fsm_dialog = true;
                        self.new_fsm_name = "MyStateMachine".to_string();
                        ui.close_menu();
                    }
                    if ui.button("New from Template").clicked() {
                        self.source_code = DEFAULT_FSM_CODE.to_string();
                        self.parse_source();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("📂 Open...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("FSM", &["fsm", "txt"])
                            .pick_file()
                        {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                self.source_code = content;
                                self.parse_source();
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("💾 Save...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("FSM", &["fsm"])
                            .save_file()
                        {
                            let _ = std::fs::write(&path, &self.source_code);
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.menu_button("📤 Export Code", |ui| {
                        if ui.button("📄 Export Current FSM...").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Rust", &["rs"])
                                .save_file()
                            {
                                let _ = std::fs::write(&path, &self.generated_code);
                            }
                            ui.close_menu();
                        }
                        if ui.button("📁 Export All FSMs to Folder...").clicked() {
                            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                self.export_all_fsms_to_folder(&folder);
                            }
                            ui.close_menu();
                        }
                    });
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                
                ui.menu_button("View", |ui| {
                    if ui.checkbox(&mut self.show_code_panel, "DSL Editor").clicked() {
                        ui.close_menu();
                    }
                    if ui.checkbox(&mut self.show_generated_panel, "Generated Code").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Reset Zoom").clicked() {
                        self.zoom = 1.0;
                        self.pan_offset = egui::Vec2::ZERO;
                        ui.close_menu();
                    }
                });

                ui.menu_button("Examples", |ui| {
                    if ui.button("Traffic Light").clicked() {
                        self.source_code = TRAFFIC_LIGHT_EXAMPLE.to_string();
                        self.parse_source();
                        ui.close_menu();
                    }
                    if ui.button("Door Lock").clicked() {
                        self.source_code = DOOR_LOCK_EXAMPLE.to_string();
                        self.parse_source();
                        ui.close_menu();
                    }
                    if ui.button("Vending Machine").clicked() {
                        self.source_code = VENDING_MACHINE_EXAMPLE.to_string();
                        self.parse_source();
                        ui.close_menu();
                    }
                });
            });
        });

        // New FSM Dialog
        if self.show_new_fsm_dialog {
            egui::Window::new("➕ Create New State Machines")
                .collapsible(false)
                .resizable(true)
                .default_width(400.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Enter FSM names (one per line or comma-separated):");
                    ui.add_space(5.0);
                    
                    ui.add(
                        egui::TextEdit::multiline(&mut self.new_fsm_name)
                            .desired_width(380.0)
                            .desired_rows(4)
                            .hint_text("MotorController\nSensorManager\nCommunicationHandler")
                    );
                    
                    ui.add_space(5.0);
                    ui.small("💡 Use PascalCase. Examples: DoorLock, TrafficLight, RobotArm");
                    ui.small("📝 Each name creates a separate state machine file");
                    
                    ui.separator();
                    
                    ui.horizontal(|ui| {
                        if ui.button("✓ Create New (Replace)").clicked() {
                            if !self.new_fsm_name.is_empty() {
                                let names = self.new_fsm_name.clone();
                                self.create_new_fsms(&names);
                                self.show_new_fsm_dialog = false;
                            }
                        }
                        if ui.button("➕ Add to Existing").clicked() {
                            if !self.new_fsm_name.is_empty() {
                                // Add each FSM to existing code
                                let names_str = self.new_fsm_name.clone();
                                let names: Vec<&str> = names_str
                                    .split(|c| c == ',' || c == ';' || c == '\n')
                                    .map(|s| s.trim())
                                    .filter(|s| !s.is_empty())
                                    .collect();
                                for name in names {
                                    self.add_new_fsm(name);
                                }
                                self.show_new_fsm_dialog = false;
                            }
                        }
                        if ui.button("✗ Cancel").clicked() {
                            self.show_new_fsm_dialog = false;
                        }
                    });
                });
        }

        // Engine-driven layout recomputation (FSM → Graph → Dagre → Renderer)
        if self.layout_dirty {
            if let Some(fsm) = self.fsms.get(self.selected_fsm).cloned() {
                self.compute_layout_native(&fsm);
                // Keep parse errors (if any) intact; only clear layout-related errors.
                if let Some(msg) = &self.error_message {
                    if msg.starts_with("Layout error:") {
                        self.error_message = None;
                    }
                }
            }
            self.layout_dirty = false;
        }

        // Left panel: Code editor
        if self.show_code_panel {
            egui::SidePanel::left("code_panel")
                .default_width(400.0)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.heading("FSM Definition");
                    
                    ui.horizontal(|ui| {
                        if ui.button("▶ Parse & Visualize").clicked() {
                            // Before parsing, update source_code from current fsm_source
                            if self.selected_fsm < self.fsm_sources.len() {
                                self.rebuild_source_code();
                            }
                            self.parse_source();
                        }
                        
                        if ui.button("➕ Add FSM").clicked() {
                            self.show_new_fsm_dialog = true;
                            self.new_fsm_name = "NewMachine".to_string();
                        }
                    });
                    
                    // FSM tabs
                    if !self.fsm_sources.is_empty() {
                        ui.separator();
                        let mut new_selection: Option<usize> = None;
                        
                        // Collect names first to avoid borrow issues
                        let tab_names: Vec<String> = (0..self.fsm_sources.len())
                            .map(|i| {
                                self.fsms.get(i)
                                    .map(|f| f.name.clone())
                                    .unwrap_or_else(|| format!("FSM {}", i + 1))
                            })
                            .collect();
                        
                        ui.horizontal_wrapped(|ui| {
                            for (i, name) in tab_names.iter().enumerate() {
                                let selected = i == self.selected_fsm;
                                if ui.selectable_label(selected, name).clicked() {
                                    new_selection = Some(i);
                                }
                            }
                        });
                        
                        if let Some(i) = new_selection {
                            if i != self.selected_fsm {
                                // Save current edit before switching
                                // Update source_code from all fsm_sources
                                self.rebuild_source_code();
                                self.selected_fsm = i;
                                self.mark_layout_dirty();
                                self.regenerate_code();
                            }
                        }
                    }
                    
                    ui.separator();

                    // Error display
                    if let Some(ref error) = self.error_message {
                        ui.colored_label(egui::Color32::RED, format!("❌ {}", error));
                        ui.separator();
                    }

                    // Code editor - show only selected FSM
                    if self.selected_fsm < self.fsm_sources.len() {
                        // Show file indicator
                        if let Some(fsm) = self.fsms.get(self.selected_fsm) {
                            ui.horizontal(|ui| {
                                ui.colored_label(egui::Color32::LIGHT_BLUE, "📝");
                                ui.label(format!("{}.fsm", to_snake_case(&fsm.name)));
                                ui.colored_label(egui::Color32::GRAY, format!("({}/{})", self.selected_fsm + 1, self.fsm_sources.len()));
                            });
                        }
                        
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            let response = ui.add(
                                egui::TextEdit::multiline(&mut self.fsm_sources[self.selected_fsm])
                                    .font(egui::TextStyle::Monospace)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(30)
                            );
                            
                            // Auto-parse on edit (with delay would be better, but this works)
                            if response.changed() {
                                // Update the combined source
                                self.rebuild_source_code();
                            }
                        });
                    } else {
                        // Fallback: edit full source
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.source_code)
                                    .font(egui::TextStyle::Monospace)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(30)
                            );
                        });
                    }
                });
        }

        // Right panel: Generated Code
        if self.show_generated_panel {
            egui::SidePanel::right("generated_panel")
                .default_width(450.0)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.heading("Generated Rust Code");
                    
                    // Target selector
                    ui.horizontal(|ui| {
                        ui.label("Target:");
                        let prev_target = self.codegen_target;
                        egui::ComboBox::from_id_salt("target_selector")
                            .selected_text(match self.codegen_target {
                                CodegenTarget::Standard => "🖥 Standard (std)",
                                CodegenTarget::Embassy => "🔌 Embassy (async embedded)",
                                CodegenTarget::Rtic => "⚡ RTIC (interrupt-driven)",
                            })
                            .show_ui(ui, |ui| {
                                for (target, label) in [
                                    (CodegenTarget::Standard, "🖥 Standard (std)"),
                                    (CodegenTarget::Embassy, "🔌 Embassy (async embedded)"),
                                    (CodegenTarget::Rtic, "⚡ RTIC (interrupt-driven)"),
                                ] {
                                    // Selectable either way, so the target can be
                                    // inspected — but marked, so the stub it emits
                                    // is not mistaken for a generation failure.
                                    let text = if target.is_available() {
                                        label.to_string()
                                    } else {
                                        format!("{label}  · Pro")
                                    };
                                    let item = ui.selectable_value(
                                        &mut self.codegen_target,
                                        target,
                                        text,
                                    );
                                    if let Some(message) = target.upgrade_message() {
                                        item.on_hover_text(message);
                                    }
                                }
                            });
                        if self.codegen_target != prev_target {
                            self.regenerate_code();
                        }
                    });

                    if let Some(message) = self.codegen_target.upgrade_message() {
                        ui.label(
                            egui::RichText::new(message)
                                .small()
                                .color(egui::Color32::from_rgb(220, 180, 90)),
                        );
                    }
                    
                    ui.separator();
                    
                    ui.horizontal(|ui| {
                        if ui.button("📋 Copy").clicked() {
                            ui.output_mut(|o| o.copied_text = self.generated_code.clone());
                        }
                        if ui.button("💾 Save...").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Rust", &["rs"])
                                .save_file()
                            {
                                let _ = std::fs::write(&path, &self.generated_code);
                            }
                        }
                    });
                    
                    // Target info
                    ui.separator();
                    match self.codegen_target {
                        CodegenTarget::Standard => {
                            ui.colored_label(egui::Color32::LIGHT_BLUE, "Standard Rust with std library");
                        }
                        CodegenTarget::Embassy => {
                            ui.colored_label(egui::Color32::LIGHT_GREEN, "🎯 Active Objects for Embassy");
                            ui.small("• Async/await for embedded");
                            ui.small("• Event queue with channel");
                            ui.small("• no_std compatible");
                        }
                        CodegenTarget::Rtic => {
                            ui.colored_label(egui::Color32::YELLOW, "⚡ RTIC v2 - Real-Time Interrupt-driven");
                            ui.small("• Hardware interrupt tasks");
                            ui.small("• Zero-cost abstractions");
                            ui.small("• heapless queue");
                        }
                    }
                    
                    ui.separator();
                    
                    // FSM Tabs - show each FSM in its own tab
                    let mut tab_changed = false;
                    if self.fsms.len() > 1 {
                        let mut new_selection: Option<usize> = None;
                        ui.horizontal_wrapped(|ui| {
                            for i in 0..self.fsms.len() {
                                let selected = i == self.selected_fsm;
                                let name = &self.fsms[i].name;
                                if ui.selectable_label(selected, name).clicked() {
                                    new_selection = Some(i);
                                }
                            }
                        });
                        if let Some(i) = new_selection {
                            if i != self.selected_fsm {
                                self.selected_fsm = i;
                                tab_changed = true;
                            }
                        }
                        ui.separator();
                    }
                    
                    // Regenerate code if tab changed
                    if tab_changed {
                        self.mark_layout_dirty();
                        if let Some(fsm) = self.fsms.get(self.selected_fsm) {
                            self.generated_code = generate_rust_code_with_target(fsm, self.codegen_target)
                .unwrap_or_else(|errors| format_codegen_errors(&fsm.name, &errors));
                        }
                    }
                    
                    if self.generated_code.is_empty() {
                        ui.colored_label(egui::Color32::GRAY, "No code generated yet.\nParse an FSM to generate code.");
                    } else {
                        // Show current FSM name with clear indicator
                        if let Some(fsm) = self.fsms.get(self.selected_fsm) {
                            ui.horizontal(|ui| {
                                ui.colored_label(egui::Color32::LIGHT_GREEN, "📄");
                                ui.colored_label(egui::Color32::WHITE, format!("{}.rs", to_snake_case(&fsm.name)));
                                ui.colored_label(egui::Color32::GRAY, format!("({} of {})", self.selected_fsm + 1, self.fsms.len()));
                            });
                        }
                        
                        // Check if the generated code header matches the selected FSM
                        let expected_header = format!("//! Auto-generated FSM: {}", 
                            self.fsms.get(self.selected_fsm).map(|f| f.name.as_str()).unwrap_or(""));
                        if !self.generated_code.contains(&expected_header) {
                            // Force regenerate if mismatch
                            if let Some(fsm) = self.fsms.get(self.selected_fsm) {
                                self.generated_code = generate_rust_code_with_target(fsm, self.codegen_target)
                .unwrap_or_else(|errors| format_codegen_errors(&fsm.name, &errors));
                            }
                        }
                        
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.generated_code.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(40)
                            );
                        });
                    }
                });
        }

        // Main panel: FSM Diagram
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("State Diagram");
            
            // Toolbar (wrapped so it doesn't disappear when panels are narrow)
            ui.horizontal_wrapped(|ui| {
                let sim_enabled_before = self.sim.enabled;
                ui.checkbox(&mut self.sim.enabled, "Debug sim");
                if sim_enabled_before != self.sim.enabled {
                    self.sim.running = false;
                    self.sim.last_frame = None;
                    self.sim.last_fired = None;
                }

                ui.separator();

                // Zoom controls
                if ui.button("➖").clicked() {
                    self.zoom = (self.zoom - 0.1).max(0.3);
                }
                ui.label(format!("{:.0}%", self.zoom * 100.0));
                if ui.button("➕").clicked() {
                    self.zoom = (self.zoom + 0.1).min(3.0);
                }
                ui.separator();
                
                if let Some(fsm) = self.fsms.get(self.selected_fsm) {
                    ui.label(format!(
                        "States: {} | Transitions: {}",
                        fsm.states.len(),
                        fsm.transitions.len()
                    ));
                }

                ui.separator();
                ui.label("Layout:");
                let mut dir_changed = false;
                egui::ComboBox::from_id_salt("layout_direction")
                    .selected_text(match self.layout_config.direction {
                        LayoutDirection::TB => "TB",
                        LayoutDirection::LR => "LR",
                    })
                    .show_ui(ui, |ui| {
                        dir_changed |= ui
                            .selectable_value(&mut self.layout_config.direction, LayoutDirection::TB, "TB")
                            .changed();
                        dir_changed |= ui
                            .selectable_value(&mut self.layout_config.direction, LayoutDirection::LR, "LR")
                            .changed();
                    });
                if dir_changed {
                    self.mark_layout_dirty();
                }
            });

            if self.sim.enabled {
                let fsm_for_sim = self.fsms.get(self.selected_fsm).cloned();
                if let Some(fsm) = fsm_for_sim {
                    ui.separator();
                    ui.horizontal(|ui| {
                        let current = self
                            .sim
                            .current_state
                            .as_deref()
                            .unwrap_or("<not started>");
                        ui.label(format!("Current: {current}"));
                        if ui.button("Reset").clicked() {
                            self.sim_reset_to_initial(&fsm);
                        }
                        if ui.button(if self.sim.running { "Pause" } else { "Run" }).clicked() {
                            self.sim.running = !self.sim.running;
                            self.sim.last_frame = Some(Instant::now());
                        }
                        if ui.button("Step").clicked() {
                            self.sim_step(&fsm);
                        }
                        ui.add(egui::Slider::new(&mut self.sim.speed, 0.1..=5.0).text("speed"));
                    });

                    ui.horizontal(|ui| {
                        ui.label("Manual");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.sim.event_input)
                                .desired_width(340.0)
                                .hint_text("one event name, then press Post"),
                        );
                        if ui.button("Post").clicked() {
                            let ev = self.sim.event_input.trim().to_string();
                            self.sim_post_event(ev);
                            self.sim.event_input.clear();
                        }
                        if ui.button("Clear log").clicked() {
                            self.sim.log.clear();
                        }
                    });

                    // The auto-driver gets its own row, with the text field at
                    // the same width as the manual one so the two read as a
                    // pair rather than as one wrapped line.
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.sim.auto_tick, "Auto");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.sim.auto_event)
                                .desired_width(340.0)
                                .hint_text("event1, event2, … cycled while running"),
                        );
                        ui.label("every");
                        ui.add(
                            egui::DragValue::new(&mut self.sim.auto_period_s)
                                .speed(0.1)
                                .range(0.1..=10.0)
                                .suffix("s"),
                        );
                    });

                    // Per-frame sim update (auto event + stepping).
                    let now = Instant::now();
                    let dt_s = if let Some(last) = self.sim.last_frame {
                        (now - last).as_secs_f32()
                    } else {
                        0.0
                    };
                    self.sim.last_frame = Some(now);

                    if self.sim.running {
                        if self.sim.auto_tick {
                            self.sim.auto_accum_s += dt_s;
                            while self.sim.auto_accum_s >= self.sim.auto_period_s {
                                self.sim.auto_accum_s -= self.sim.auto_period_s;
                                let sequence: Vec<String> = self
                                    .sim
                                    .auto_event
                                    .split(',')
                                    .map(|e| e.trim().to_string())
                                    .filter(|e| !e.is_empty())
                                    .collect();
                                if !sequence.is_empty() {
                                    let ev = sequence[self.sim.auto_cursor % sequence.len()].clone();
                                    self.sim.auto_cursor =
                                        (self.sim.auto_cursor + 1) % sequence.len();
                                    self.sim_post_event(ev);
                                }
                            }
                        }
                        // Consume at most one event per frame to keep animation readable.
                        if !self.sim.queued_events.is_empty() {
                            self.sim_step(&fsm);
                        }
                    }

                    egui::ScrollArea::vertical()
                        .max_height(80.0)
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            let start = self.sim.log.len().saturating_sub(30);
                            for line in self.sim.log[start..].iter() {
                                ui.label(line);
                            }
                        });
                }
            }

            ui.horizontal(|ui| {
                let mut changed = false;
                changed |= ui.add(egui::DragValue::new(&mut self.layout_config.nodesep).speed(1.0).prefix("nodesep ")).changed();
                changed |= ui.add(egui::DragValue::new(&mut self.layout_config.ranksep).speed(1.0).prefix("ranksep ")).changed();
                changed |= ui.add(egui::DragValue::new(&mut self.layout_config.edgesep).speed(1.0).prefix("edgesep ")).changed();
                if changed {
                    self.mark_layout_dirty();
                }
            });
            
            ui.separator();

            // Drawing area
            let (response, painter) = ui.allocate_painter(
                ui.available_size(),
                egui::Sense::drag(),
            );

            // Handle panning
            if response.dragged() {
                self.pan_offset += response.drag_delta();
            }

            // Handle zoom with scroll
            let scroll_delta = ctx.input(|i| i.raw_scroll_delta);
            if response.hovered() && scroll_delta.y != 0.0 {
                self.zoom = (self.zoom + scroll_delta.y * 0.001).clamp(0.3, 3.0);
            }

            let rect = response.rect;
            
            // Draw background
            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(25, 28, 32));

            // Draw grid
            draw_grid(&painter, rect, self.zoom, self.pan_offset);

            if let Some(fsm) = self.fsms.get(self.selected_fsm) {
                // Transform helper
                let transform = |pos: egui::Pos2| -> egui::Pos2 {
                    let centered = pos.to_vec2() * self.zoom;
                    rect.center() + centered + self.pan_offset
                };

                if let Some(layout) = &self.layout {
                    // Draw edges from engine-provided points.
                    for edge in &layout.edges {
                        if edge.points.len() >= 2 {
                            let mut route: Vec<egui::Pos2> = edge.points.iter().copied().map(transform).collect();

                            // Ensure there is at least one segment
                            route.dedup_by(|a, b| (a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() < 0.01);
                            if route.len() >= 2 {
                                let color = match edge.transition_type {
                                    TransitionType::Forward => egui::Color32::from_rgb(150, 160, 180),
                                    TransitionType::Return => egui::Color32::from_rgb(120, 180, 140),
                                    TransitionType::Conditional => egui::Color32::from_rgb(180, 150, 120),
                                    TransitionType::Timer => egui::Color32::from_rgb(180, 180, 120),
                                };
                                draw_orthogonal_arrow_colored(&painter, &route, self.zoom, color);
                            }
                        }
                    }

                    // Draw labels as nodes produced by the engine (no edge-label proxy required).
                    for label in &layout.labels {
                        let label_pos = transform(label.pos);
                        let font_size = self.layout_config.edge_label_font_size * self.zoom;
                        let text_size = Self::measure_text(ctx, &label.text, font_size);
                        let rect = egui::Rect::from_center_size(
                            label_pos,
                            text_size + egui::vec2(14.0 * self.zoom, 8.0 * self.zoom),
                        );
                        draw_label(
                            &painter,
                            &LabelInfo {
                                pos: label_pos,
                                rect,
                                text: label.text.clone(),
                                font_size,
                            },
                        );
                    }

                    // Draw the initial pseudo-state if present.
                    if let Some(&pos) = self.state_positions.get("[*]") {
                        let p = transform(pos);
                        painter.circle_filled(p, 8.0 * self.zoom, egui::Color32::WHITE);
                        painter.circle_filled(p, 4.0 * self.zoom, egui::Color32::BLACK);
                    }

                    // Draw states (on top)
                    for state in &fsm.states {
                        if let Some(&pos) = self.state_positions.get(&state.name) {
                            let transformed_pos = transform(pos);
                            let is_active = self
                                .sim
                                .enabled
                                .then(|| self.sim.current_state.as_deref() == Some(state.name.as_str()))
                                .unwrap_or(false);
                            draw_state(
                                &painter,
                                transformed_pos,
                                state,
                                fsm.initial_state.as_deref() == Some(&state.name),
                                is_active,
                                self.zoom,
                            );
                        }
                    }

                    // Animate last fired transition as a moving dot along the engine route.
                    if self.sim.enabled {
                        if let Some(fired) = &self.sim.last_fired {
                            let elapsed = fired.started_at.elapsed().as_secs_f32();
                            if elapsed <= fired.duration_s {
                                if let Some(t_idx) = fired.transition_index {
                                    if let Some(route) = Self::sim_route_for_transition(layout, t_idx, &fired.from, &fired.to) {
                                        let route_screen: Vec<egui::Pos2> = route.into_iter().map(transform).collect();
                                        let t = (elapsed / fired.duration_s).clamp(0.0, 1.0);
                                        if let Some(p) = Self::polyline_point_at(&route_screen, t) {
                                            painter.circle_filled(
                                                p,
                                                6.0 * self.zoom,
                                                egui::Color32::from_rgb(255, 220, 120),
                                            );
                                            painter.circle_stroke(
                                                p,
                                                6.0 * self.zoom,
                                                egui::Stroke::new(2.0 * self.zoom, egui::Color32::from_rgb(40, 30, 20)),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Layout engine has not produced a diagram yet.\nCheck the error panel for layout errors.",
                        egui::FontId::proportional(16.0),
                        egui::Color32::GRAY,
                    );
                }
            } else {
                // No FSM loaded message
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "No FSM loaded.\nWrite FSM code and click 'Parse & Visualize'",
                    egui::FontId::proportional(18.0),
                    egui::Color32::GRAY,
                );
            }
        });

        // Bottom panel: Info
        egui::TopBottomPanel::bottom("info_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Oxidate v0.1.0");
                ui.separator();
                ui.label("Scroll to zoom | Drag to pan");
                
                if let Some(fsm) = self.fsms.get(self.selected_fsm) {
                    ui.separator();
                    if let Some(ref initial) = fsm.initial_state {
                        ui.label(format!("Initial: {}", initial));
                    }
                }
            });
        });

        // eframe/egui only repaints on input by default. The simulator needs continuous
        // repainting for Auto stepping + transition animation, even when the mouse is idle.
        if self.sim.enabled {
            let animating = self
                .sim
                .last_fired
                .as_ref()
                .is_some_and(|f| f.started_at.elapsed().as_secs_f32() < f.duration_s);
            if self.sim.running || animating {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
        }
    }
}

/// Information about a label for overlap detection
#[derive(Clone)]
struct LabelInfo {
    pos: egui::Pos2,
    rect: egui::Rect,
    text: String,
    font_size: f32,
}






/// Determine the type of transition for rendering decisions (layout is engine-driven).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransitionType {
    Forward,      // Main flow - straight arrows
    Return,       // Return transitions - curved arrows
    Conditional,  // Has guards - curved arrows
    Timer,        // Timer events - label above
}

impl Default for TransitionType {
    fn default() -> Self {
        TransitionType::Forward
    }
}

fn classify_transition(transition: &fsm::Transition, is_reverse: bool) -> TransitionType {
    // Get event name if present
    let event_name = transition.event.as_ref()
        .map(|e| e.name.to_lowercase())
        .unwrap_or_default();
    
    // Timer events
    if event_name.contains("timeout") || event_name.contains("timer") || event_name.contains("expired") {
        return TransitionType::Timer;
    }
    
    // Conditional (has guard)
    if transition.guard.is_some() {
        return TransitionType::Conditional;
    }
    
    // Return vs Forward
    if is_reverse {
        TransitionType::Return
    } else {
        TransitionType::Forward
    }
}


/// Format label text - break into multiple SHORT lines for better readability
fn format_label_text(label: &str) -> String {
    let mut result = String::new();
    
    // Split event and guard
    if let Some(bracket_start) = label.find('[') {
        let event = label[..bracket_start].trim();
        let guard_part = &label[bracket_start..];
        
        // Add event (may need to break if long)
        if event.len() > 15 {
            // Break long event names at underscores
            let parts: Vec<&str> = event.split('_').collect();
            let mut line = String::new();
            for (i, part) in parts.iter().enumerate() {
                if line.len() + part.len() > 12 && !line.is_empty() {
                    result.push_str(&line);
                    result.push('\n');
                    line = part.to_string();
                } else {
                    if !line.is_empty() {
                        line.push('_');
                    }
                    line.push_str(part);
                }
                if i == parts.len() - 1 {
                    result.push_str(&line);
                }
            }
        } else {
            result.push_str(event);
        }
        
        // Add guard on new line
        result.push('\n');
        result.push_str(guard_part);
    } else if label.len() > 15 {
        // Long label without guard - break at underscores
        let parts: Vec<&str> = label.split('_').collect();
        let mut line = String::new();
        for (i, part) in parts.iter().enumerate() {
            if line.len() + part.len() > 12 && !line.is_empty() {
                result.push_str(&line);
                result.push('\n');
                line = part.to_string();
            } else {
                if !line.is_empty() {
                    line.push('_');
                }
                line.push_str(part);
            }
            if i == parts.len() - 1 {
                result.push_str(&line);
            }
        }
    } else {
        result = label.to_string();
    }
    
    result
}






/// Draw orthogonal arrow with custom color
fn draw_orthogonal_arrow_colored(painter: &egui::Painter, route: &[egui::Pos2], zoom: f32, color: egui::Color32) {
    if route.len() < 2 {
        return;
    }
    
    let stroke = egui::Stroke::new(1.5 * zoom, color);
    
    // Draw line segments
    for i in 0..route.len() - 1 {
        painter.line_segment([route[i], route[i + 1]], stroke);
    }
    
    // Arrowhead direction: the last segment with usable length. Taking the
    // final pair blindly points the head in a meaningless direction whenever
    // the route ends with a very short jog.
    let last = route[route.len() - 1];
    let dir = route
        .iter()
        .rev()
        .skip(1)
        .map(|p| last - *p)
        .find(|v| v.length() > 1.0)
        .map(|v| v.normalized())
        .unwrap_or(egui::vec2(0.0, 1.0));
    
    let arrow_size = 10.0 * zoom;
    let arrow_angle = 0.4;
    
    let perp = egui::vec2(-dir.y, dir.x);
    let arrow_p1 = last - dir * arrow_size + perp * arrow_size * arrow_angle;
    let arrow_p2 = last - dir * arrow_size - perp * arrow_size * arrow_angle;
    
    painter.add(egui::Shape::convex_polygon(
        vec![last, arrow_p1, arrow_p2],
        color,
        egui::Stroke::NONE,
    ));
}

/// Draw a transition label
fn draw_label(painter: &egui::Painter, info: &LabelInfo) {
    // Background
    painter.rect_filled(info.rect, 3.0, egui::Color32::from_rgb(30, 35, 45));
    painter.rect_stroke(info.rect, 3.0, egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(70, 80, 95)));
    
    // Text
    painter.text(
        info.pos,
        egui::Align2::CENTER_CENTER,
        &info.text,
        egui::FontId::proportional(info.font_size),
        egui::Color32::from_rgb(255, 230, 120),
    );
}

fn draw_grid(painter: &egui::Painter, rect: egui::Rect, zoom: f32, offset: egui::Vec2) {
    let grid_size = 50.0 * zoom;
    let grid_color = egui::Color32::from_rgba_unmultiplied(100, 100, 100, 30);
    
    let start_x = ((rect.left() - offset.x) / grid_size).floor() * grid_size + offset.x;
    let start_y = ((rect.top() - offset.y) / grid_size).floor() * grid_size + offset.y;
    
    let mut x = start_x;
    while x < rect.right() {
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0_f32, grid_color),
        );
        x += grid_size;
    }
    
    let mut y = start_y;
    while y < rect.bottom() {
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0_f32, grid_color),
        );
        y += grid_size;
    }
}

/// Estimate the visual size of a state box
/// Which side of a box a route point sits on.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum BoxSide {
    Top,
    Bottom,
    Left,
    Right,
}

fn side_of(point: egui::Pos2, rect: egui::Rect) -> BoxSide {
    // Endpoints are already clipped onto the border, so the smallest distance
    // identifies the side unambiguously.
    let candidates = [
        (BoxSide::Top, (point.y - rect.top()).abs()),
        (BoxSide::Bottom, (point.y - rect.bottom()).abs()),
        (BoxSide::Left, (point.x - rect.left()).abs()),
        (BoxSide::Right, (point.x - rect.right()).abs()),
    ];
    candidates
        .iter()
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(side, _)| *side)
        .unwrap()
}



/// Chooses where each edge label sits.
///
/// Placing a label at a fixed point on its route and shoving whatever it hits
/// does not converge: every shove creates the next collision. Instead each label
/// proposes a set of positions along its own route, and the one that scores best
/// is taken — so a bad spot is declined rather than resolved afterwards.
///
/// The score charges for overlapping a state box, another route, or a label
/// already placed, and pays a small bonus for lining up with one. Labels are
/// placed in order, so earlier ones constrain later ones; that is what makes a
/// row of them share an edge instead of scattering.
fn place_labels(
    labels: &mut [LayoutedLabel],
    routes: &[Vec<egui::Pos2>],
    owners: &[usize],
    fixed: &[bool],
    boxes: &std::collections::HashMap<String, egui::Rect>,
    font_size: f32,
) {
    const BOX_PENALTY: f32 = 1000.0;
    const ROUTE_PENALTY: f32 = 120.0;
    const LABEL_PENALTY: f32 = 400.0;
    const DRIFT_WEIGHT: f32 = 0.6;
    const ALIGN_BONUS: f32 = 45.0;
    const ALIGN_TOLERANCE: f32 = 3.0;

    let state_rects: Vec<egui::Rect> = boxes.values().copied().collect();

    let size_of = |text: &str| {
        let lines = text.lines().count().max(1) as f32;
        egui::vec2(
            label_box_width(text, font_size),
            lines * font_size * 1.4 + 8.0,
        )
    };

    let mut placed: Vec<egui::Rect> = Vec::new();

    for index in 0..labels.len() {
        let size = size_of(&labels[index].text);

        // A self-loop encloses its label by construction; moving it would take
        // the label outside the loop drawn around it.
        if fixed.get(index).copied().unwrap_or(false) {
            placed.push(egui::Rect::from_center_size(labels[index].pos, size));
            continue;
        }

        let route = owners
            .get(index)
            .and_then(|owner| routes.get(*owner))
            .cloned()
            .unwrap_or_default();

        let candidates = label_candidates(&route, size, labels[index].pos);
        let natural = candidates.first().copied().unwrap_or(labels[index].pos);

        let mut best = natural;
        let mut best_score = f32::MAX;

        for candidate in candidates {
            let rect = egui::Rect::from_center_size(candidate, size);
            let mut score = (candidate - natural).length() * DRIFT_WEIGHT;

            for state in &state_rects {
                if rect.intersects(*state) {
                    score += BOX_PENALTY;
                }
            }

            for (route_index, points) in routes.iter().enumerate() {
                // Sitting on its own line is the point; the label's background
                // covers it. Other routes passing underneath are the problem.
                if owners.get(index) == Some(&route_index) {
                    continue;
                }
                for pair in points.windows(2) {
                    if segment_meets_rect(pair[0], pair[1], rect) {
                        score += ROUTE_PENALTY;
                    }
                }
            }

            for other in &placed {
                if rect.intersects(*other) {
                    score += LABEL_PENALTY;
                } else if (other.center().x - candidate.x).abs() < ALIGN_TOLERANCE
                    || (other.center().y - candidate.y).abs() < ALIGN_TOLERANCE
                {
                    // Sharing an edge with a neighbour reads as deliberate.
                    score -= ALIGN_BONUS;
                }
            }

            if score < best_score {
                best_score = score;
                best = candidate;
            }
        }

        // Scoring picks the least bad candidate, which on a route that hugs a
        // state can still be one that overlaps it — and since boxes are drawn
        // after labels, the label simply disappears behind one. Clearing a state
        // box is therefore enforced rather than merely preferred.
        let mut chosen = best;
        for _ in 0..4 {
            let rect = egui::Rect::from_center_size(chosen, size);
            let Some(hit) = state_rects.iter().find(|r| r.intersects(rect)) else {
                break;
            };
            chosen += shortest_push(rect, *hit, 8.0);
        }

        labels[index].pos = chosen;
        placed.push(egui::Rect::from_center_size(chosen, size));
    }
}

/// The smallest move that clears `rect` of `obstacle`, plus `gap`.
fn shortest_push(rect: egui::Rect, obstacle: egui::Rect, gap: f32) -> egui::Vec2 {
    let left = obstacle.left() - rect.right() - gap;
    let right = obstacle.right() - rect.left() + gap;
    let up = obstacle.top() - rect.bottom() - gap;
    let down = obstacle.bottom() - rect.top() + gap;

    let dx = if left.abs() <= right.abs() { left } else { right };
    let dy = if up.abs() <= down.abs() { up } else { down };

    if dx.abs() <= dy.abs() {
        egui::vec2(dx, 0.0)
    } else {
        egui::vec2(0.0, dy)
    }
}

/// Positions a label may take along its route, best guess first.
///
/// Along each segment at a few fractions, plus a perpendicular offset either
/// way, so a label can step off a crowded line without leaving it.
fn label_candidates(route: &[egui::Pos2], size: egui::Vec2, fallback: egui::Pos2) -> Vec<egui::Pos2> {
    const FRACTIONS: [f32; 3] = [0.5, 0.35, 0.65];
    const EPS: f32 = 0.5;

    if route.len() < 2 {
        return vec![fallback];
    }

    // Longest segment first: the label reads best on a long straight run.
    let mut segments: Vec<(egui::Pos2, egui::Pos2)> =
        route.windows(2).map(|p| (p[0], p[1])).collect();
    segments.sort_by(|a, b| {
        (b.1 - b.0)
            .length()
            .partial_cmp(&(a.1 - a.0).length())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut candidates = Vec::new();
    for (a, b) in segments {
        let vertical = (a.x - b.x).abs() < EPS;
        // Two distances either side: a short step off the line, and a longer
        // one for when the route runs alongside a state box.
        let near = if vertical { size.x * 0.5 + 8.0 } else { size.y * 0.5 + 8.0 };
        let far = near * 2.2;
        let offsets = [0.0, near, -near, far, -far];

        for fraction in FRACTIONS {
            let along = a + (b - a) * fraction;
            for offset in offsets {
                candidates.push(if vertical {
                    egui::pos2(along.x + offset, along.y)
                } else {
                    egui::pos2(along.x, along.y + offset)
                });
            }
        }
    }

    candidates
}

/// Whether an axis-aligned segment touches a rectangle.
fn segment_meets_rect(a: egui::Pos2, b: egui::Pos2, rect: egui::Rect) -> bool {
    const EPS: f32 = 0.5;

    if (a.x - b.x).abs() < EPS {
        let (lo, hi) = (a.y.min(b.y), a.y.max(b.y));
        a.x >= rect.left() && a.x <= rect.right() && lo <= rect.bottom() && hi >= rect.top()
    } else if (a.y - b.y).abs() < EPS {
        let (lo, hi) = (a.x.min(b.x), a.x.max(b.x));
        a.y >= rect.top() && a.y <= rect.bottom() && lo <= rect.right() && hi >= rect.left()
    } else {
        // Diagonals should not exist after `orthogonalise`; treat the bounding
        // box as a conservative approximation rather than assume.
        egui::Rect::from_two_pos(a, b).intersects(rect)
    }
}

/// Nudges apart route segments that lie on top of one another.
///
/// Endpoint distribution spaces edges along each box border, but says nothing
/// about the runs between them: two edges could still travel the same `x` over
/// an overlapping stretch of `y`, drawn as a single line. Crossings at right
/// angles are unavoidable in a dense graph and are left alone — it is the
/// collinear overlaps that mislead.
///
/// Only interior points move. The first and last are clipped to a box border and
/// moving them would reintroduce the floating arrowheads.
fn separate_overlapping_runs(edges: &mut [LayoutedEdge]) {
    const EPS: f32 = 0.5;
    const STEP: f32 = 14.0;
    const ATTEMPTS: usize = 6;

    // Every segment except the two that touch a box border. Those two carry the
    // clipped endpoint, and moving them would float the arrowhead off the box.
    //
    // A route of only two or three points has no such segment, so it cannot be
    // separated this way — its overlap has to be resolved by moving whatever it
    // overlaps with.
    let movable: Vec<(usize, usize)> = edges
        .iter()
        .enumerate()
        .flat_map(|(e, edge)| {
            (1..edge.points.len().saturating_sub(2)).map(move |i| (e, i))
        })
        .collect();

    for (edge_index, seg) in movable {
        for attempt in 0..ATTEMPTS {
            let (a, b) = {
                let pts = &edges[edge_index].points;
                (pts[seg], pts[seg + 1])
            };
            let vertical = (a.x - b.x).abs() < EPS;
            let horizontal = (a.y - b.y).abs() < EPS;
            if !vertical && !horizontal {
                break;
            }

            let clash = edges.iter().enumerate().any(|(other_index, other)| {
                if other_index == edge_index {
                    return false;
                }
                other.points.windows(2).any(|pair| {
                    let (c, d) = (pair[0], pair[1]);
                    if vertical && (c.x - d.x).abs() < EPS && (a.x - c.x).abs() < EPS {
                        let (a0, a1) = (a.y.min(b.y), a.y.max(b.y));
                        let (c0, c1) = (c.y.min(d.y), c.y.max(d.y));
                        return a0 < c1 - EPS && c0 < a1 - EPS;
                    }
                    if horizontal && (c.y - d.y).abs() < EPS && (a.y - c.y).abs() < EPS {
                        let (a0, a1) = (a.x.min(b.x), a.x.max(b.x));
                        let (c0, c1) = (c.x.min(d.x), c.x.max(d.x));
                        return a0 < c1 - EPS && c0 < a1 - EPS;
                    }
                    false
                })
            });

            if !clash {
                break;
            }

            // Alternate sides so a run does not drift steadily in one direction.
            let offset = if attempt % 2 == 0 {
                STEP * (attempt as f32 / 2.0 + 1.0)
            } else {
                -STEP * ((attempt as f32 + 1.0) / 2.0)
            };
            // Moving a run shifts both of its ends. The neighbouring segments
            // absorb that by getting longer or shorter, which keeps every angle
            // square without touching the clipped endpoints.
            let pts = &mut edges[edge_index].points;
            if vertical {
                let base = pts[seg].x;
                pts[seg].x = base + offset;
                pts[seg + 1].x = base + offset;
            } else {
                let base = pts[seg].y;
                pts[seg].y = base + offset;
                pts[seg + 1].y = base + offset;
            }
        }
    }
}

/// Spreads the endpoints meeting one side of a box evenly along it.
///
/// dagre picks where each edge meets a node, and after the route has been
/// squared off and clipped those positions look arbitrary — two arrows landing a
/// few pixels apart, a third far away. Distributing them turns a cluster into a
/// comb.
///
/// Moving an endpoint also moves the elbow feeding it, which is what keeps the
/// final segment axis-aligned; as a side effect parallel elbows line up.
fn distribute_endpoints(
    edges: &mut [LayoutedEdge],
    boxes: &std::collections::HashMap<String, egui::Rect>,
) {
    // (box name, side, is_arrival) -> indices of edges meeting there
    // (box, side) -> the edges meeting it, each flagged as arriving or leaving
    let mut groups: std::collections::HashMap<(String, BoxSide), Vec<(usize, bool)>> =
        std::collections::HashMap::new();

    for (i, edge) in edges.iter().enumerate() {
        if edge.v == edge.w || edge.points.len() < 2 {
            continue; // self-loops carry their own geometry
        }
        // Departures and arrivals share the border, so they share a group. Keying
        // them apart let an outgoing edge and an incoming one be assigned the
        // very same point, and the two ran along each other from there.
        if let Some(rect) = boxes.get(&edge.v) {
            let side = side_of(edge.points[0], *rect);
            groups.entry((edge.v.clone(), side)).or_default().push((i, false));
        }
        if let Some(rect) = boxes.get(&edge.w) {
            let side = side_of(edge.points[edge.points.len() - 1], *rect);
            groups.entry((edge.w.clone(), side)).or_default().push((i, true));
        }
    }

    for ((name, side), mut members) in groups {
        if members.len() < 2 {
            continue;
        }
        let Some(&rect) = boxes.get(&name) else {
            continue;
        };

        let horizontal_side = matches!(side, BoxSide::Top | BoxSide::Bottom);
        let coord = |edge: &LayoutedEdge, is_arrival: bool| {
            let p = if is_arrival {
                edge.points[edge.points.len() - 1]
            } else {
                edge.points[0]
            };
            if horizontal_side {
                p.x
            } else {
                p.y
            }
        };

        // Keep the existing left-to-right (or top-to-bottom) order so edges
        // don't cross each other just to be evenly spaced.
        members.sort_by(|a, b| {
            coord(&edges[a.0], a.1)
                .partial_cmp(&coord(&edges[b.0], b.1))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let (lo, hi) = if horizontal_side {
            let inset = rect.width() * 0.15;
            (rect.left() + inset, rect.right() - inset)
        } else {
            let inset = rect.height() * 0.15;
            (rect.top() + inset, rect.bottom() - inset)
        };
        let count = members.len();

        for (slot, (edge_index, is_arrival)) in members.into_iter().enumerate() {
            let target = lo + (hi - lo) * (slot as f32 + 1.0) / (count as f32 + 1.0);
            let edge = &mut edges[edge_index];
            let n = edge.points.len();
            let (end, neighbour) = if is_arrival { (n - 1, n - 2) } else { (0, 1) };

            if horizontal_side {
                // Final segment is vertical: shift both x to keep it so.
                if (edge.points[end].x - edge.points[neighbour].x).abs() < 0.5 {
                    edge.points[neighbour].x = target;
                }
                edge.points[end].x = target;
            } else {
                if (edge.points[end].y - edge.points[neighbour].y).abs() < 0.5 {
                    edge.points[neighbour].y = target;
                }
                edge.points[end].y = target;
            }
        }
    }
}

/// Midpoint of a route's longest straight segment.
///
/// The label belongs on the line it describes. dagre's own label coordinate
/// refers to the diagonal route it produced, which no longer exists after the
/// route is squared off, clipped and redistributed — so it is computed from the
/// final geometry instead.
fn label_anchor(points: &[egui::Pos2]) -> egui::Pos2 {
    let mut best = points[0];
    let mut best_len = -1.0_f32;
    for pair in points.windows(2) {
        let len = (pair[1] - pair[0]).length();
        if len > best_len {
            best_len = len;
            best = pair[0] + (pair[1] - pair[0]) * 0.5;
        }
    }
    best
}

/// Forces the first and last points of an axis-aligned route onto the borders of
/// the boxes it connects.
///
/// dagre computes endpoints against the node rect it was given, but any
/// rewriting we do afterwards — and any rounding along the way — can leave the
/// arrowhead floating short of the box or buried inside it. Clipping against the
/// rect we actually draw makes the endpoint correct by construction.
fn clip_route_to_boxes(
    points: &[egui::Pos2],
    source: egui::Rect,
    target: egui::Rect,
) -> Vec<egui::Pos2> {
    const EPS: f32 = 0.5;

    let mut pts: Vec<egui::Pos2> = points.to_vec();
    if pts.len() < 2 {
        return pts;
    }

    // Points buried inside a box are invisible; dropping them means the segment
    // that survives is the one actually meeting the border.
    while pts.len() > 2 && source.contains(pts[1]) {
        pts.remove(0);
    }
    while pts.len() > 2 && target.contains(pts[pts.len() - 2]) {
        pts.pop();
    }

    // Leaving the source: snap onto whichever border the route heads away from.
    let (a, b) = (pts[0], pts[1]);
    if (a.x - b.x).abs() < EPS {
        pts[0].y = if b.y > source.center().y {
            source.bottom()
        } else {
            source.top()
        };
        pts[0].x = pts[0].x.clamp(source.left(), source.right());
    } else {
        pts[0].x = if b.x > source.center().x {
            source.right()
        } else {
            source.left()
        };
        pts[0].y = pts[0].y.clamp(source.top(), source.bottom());
    }

    // Arriving at the target: same, from the other side.
    let n = pts.len();
    let (p, q) = (pts[n - 2], pts[n - 1]);
    if (p.x - q.x).abs() < EPS {
        pts[n - 1].y = if p.y < target.center().y {
            target.top()
        } else {
            target.bottom()
        };
        pts[n - 1].x = pts[n - 1].x.clamp(target.left(), target.right());
    } else {
        pts[n - 1].x = if p.x < target.center().x {
            target.left()
        } else {
            target.right()
        };
        pts[n - 1].y = pts[n - 1].y.clamp(target.top(), target.bottom());
    }

    pts
}

/// Width the renderer will give a label box, in unzoomed units.
///
/// Mirrors what `measure_text` plus the drawing padding produce, so callers can
/// reserve space for a label before it is drawn.
fn label_box_width(text: &str, font_size: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let widest = text.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    widest as f32 * font_size * 0.55 + 14.0
}

/// A clean rectangular loop out of the right-hand side of a box and back.
///
/// dagre routes self-edges with a geometry of its own that does not survive
/// being rewritten into right angles — the result was two stray parallel lines
/// and an arrowhead across the border. Synthesising the loop keeps it readable
/// and predictable. `reach` is how far right the loop extends: the caller sizes
/// it to enclose the label, otherwise a multi-line label spills back over the
/// state box.
fn self_loop_route(rect: egui::Rect, reach: f32) -> Vec<egui::Pos2> {
    let upper = rect.center().y - rect.height() * 0.28;
    let lower = rect.center().y + rect.height() * 0.28;

    vec![
        egui::pos2(rect.right(), upper),
        egui::pos2(reach, upper),
        egui::pos2(reach, lower),
        egui::pos2(rect.right(), lower),
    ]
}

/// Rewrites a polyline so every segment is axis-aligned.
///
/// dagre returns diagonals between ranks. Each is replaced by a three-segment
/// "Z": travel along the rank direction to the crossover point, cross, then
/// continue. A Z rather than an L keeps the flow direction dominant.
///
/// `lane` staggers the crossover so edges spanning the same pair of ranks don't
/// all run along the same line. Without it every crossover sat at the exact
/// midpoint and parallel edges overlapped.
fn orthogonalise(points: &[egui::Pos2], direction: LayoutDirection, lane: usize) -> Vec<egui::Pos2> {
    const EPS: f32 = 0.5;

    if points.len() < 2 {
        return points.to_vec();
    }

    // Fractions either side of the midpoint, cycled per edge.
    const CROSSOVERS: [f32; 5] = [0.50, 0.62, 0.38, 0.72, 0.28];
    let t = CROSSOVERS[lane % CROSSOVERS.len()];

    let mut out: Vec<egui::Pos2> = Vec::with_capacity(points.len() * 2);
    out.push(points[0]);

    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let dx = (b.x - a.x).abs();
        let dy = (b.y - a.y).abs();

        // Already axis-aligned.
        if dx <= EPS || dy <= EPS {
            out.push(b);
            continue;
        }

        // A shallow diagonal used to survive: the two points inserted below
        // landed within EPS of the originals and were deduplicated away, leaving
        // the diagonal in place and the arrowhead pointing at an angle. Turning
        // it into a single right-angle corner avoids that entirely.
        let shallow = 2.0;
        if dy < shallow {
            out.push(egui::pos2(b.x, a.y));
            out.push(b);
            continue;
        }
        if dx < shallow {
            out.push(egui::pos2(a.x, b.y));
            out.push(b);
            continue;
        }

        match direction {
            LayoutDirection::TB => {
                let cross_y = a.y + (b.y - a.y) * t;
                out.push(egui::pos2(a.x, cross_y));
                out.push(egui::pos2(b.x, cross_y));
            }
            LayoutDirection::LR => {
                let cross_x = a.x + (b.x - a.x) * t;
                out.push(egui::pos2(cross_x, a.y));
                out.push(egui::pos2(cross_x, b.y));
            }
        }
        out.push(b);
    }

    out.dedup_by(|a, b| (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS);

    // Drop points sitting in the middle of a straight run.
    let mut simplified: Vec<egui::Pos2> = Vec::with_capacity(out.len());
    for (i, p) in out.iter().copied().enumerate() {
        if i == 0 || i + 1 == out.len() {
            simplified.push(p);
            continue;
        }
        let prev = *simplified.last().unwrap();
        let next = out[i + 1];
        let straight_x = (prev.x - p.x).abs() < EPS && (p.x - next.x).abs() < EPS;
        let straight_y = (prev.y - p.y).abs() < EPS && (p.y - next.y).abs() < EPS;
        if !(straight_x || straight_y) {
            simplified.push(p);
        }
    }

    simplified
}

fn estimate_state_size(state: &fsm::State) -> egui::Vec2 {
    let font_size = 10.0_f32;
    let char_width = font_size * 0.55;
    let line_height = font_size * 1.3;
    let padding = 15.0_f32;

    let mut action_lines: Vec<String> = Vec::new();
    for entry in &state.entry_actions {
        action_lines.push(format!("entry/ {}", entry.name));
    }
    for exit in &state.exit_actions {
        action_lines.push(format!("exit/ {}", exit.name));
    }

    let name_width = state.name.chars().count() as f32 * 9.0;
    let action_width = action_lines
        .iter()
        .map(|line| line.chars().count() as f32 * char_width)
        .fold(0.0_f32, f32::max);
    let width = name_width.max(action_width).max(80.0) + padding * 2.0;

    let header_height = 22.0_f32;
    let actions_height = if action_lines.is_empty() {
        20.0
    } else {
        (action_lines.len() as f32 * line_height) + padding
    };

    egui::vec2(width, header_height + actions_height)
}



fn draw_state(
    painter: &egui::Painter,
    pos: egui::Pos2,
    state: &fsm::State,
    is_initial: bool,
    is_active: bool,
    zoom: f32,
) {
    // Calculate content for dynamic sizing
    let mut action_lines = Vec::new();
    for entry in &state.entry_actions {
        action_lines.push(format!("entry/ {}", entry.name));
    }
    for exit in &state.exit_actions {
        action_lines.push(format!("exit/ {}", exit.name));
    }
    
    let font_size = 10.0 * zoom;
    let header_height = 22.0 * zoom;

    // Same measurement the layout gave dagre, scaled. Keeping these in sync is
    // what makes edges meet the border instead of stopping short of it.
    let rect = egui::Rect::from_center_size(pos, estimate_state_size(state) * zoom);
    
    // Colors
    let fill_color = match state.state_type {
        StateType::Composite => egui::Color32::from_rgb(50, 80, 120),
        StateType::Final => egui::Color32::from_rgb(100, 50, 50),
        _ => egui::Color32::from_rgb(40, 55, 75),
    };
    
    let header_color = match state.state_type {
        StateType::Composite => egui::Color32::from_rgb(60, 95, 140),
        StateType::Final => egui::Color32::from_rgb(120, 60, 60),
        _ => egui::Color32::from_rgb(55, 75, 100),
    };
    
    let stroke_color = if is_active {
        egui::Color32::from_rgb(255, 220, 120)
    } else if is_initial {
        egui::Color32::from_rgb(100, 220, 100)
    } else {
        egui::Color32::from_rgb(100, 120, 145)
    };

    let stroke_width = if is_active { 3.5 } else if is_initial { 3.0 } else { 1.5 };
    let corner_radius = 8.0 * zoom;
    
    // Draw main box (body)
    painter.rect(
        rect,
        corner_radius,
        fill_color,
        egui::Stroke::new(stroke_width * zoom, stroke_color),
    );
    
    // Draw header compartment (name area)
    let header_rect = egui::Rect::from_min_size(
        rect.min,
        egui::vec2(rect.width(), header_height),
    );
    
    // Header with rounded top corners only
    painter.rect_filled(
        header_rect,
        egui::Rounding {
            nw: corner_radius,
            ne: corner_radius,
            sw: 0.0,
            se: 0.0,
        },
        header_color,
    );
    
    // Separator line between header and body
    painter.line_segment(
        [
            egui::pos2(rect.left(), rect.top() + header_height),
            egui::pos2(rect.right(), rect.top() + header_height),
        ],
        egui::Stroke::new(1.0 * zoom, stroke_color),
    );
    
    // State name in header (centered)
    let name_pos = egui::pos2(rect.center().x, rect.top() + header_height / 2.0);
    painter.text(
        name_pos,
        egui::Align2::CENTER_CENTER,
        &state.name,
        egui::FontId::proportional(13.0 * zoom),
        egui::Color32::WHITE,
    );
    
    // Entry/exit actions in body
    if !action_lines.is_empty() {
        let body_center_y = rect.top() + header_height + (rect.height() - header_height) / 2.0;
        let actions = action_lines.join("\n");
        painter.text(
            egui::pos2(rect.center().x, body_center_y),
            egui::Align2::CENTER_CENTER,
            actions,
            egui::FontId::proportional(font_size),
            egui::Color32::from_rgb(180, 200, 220),
        );
    }
}

// Default FSM code shown on startup
const DEFAULT_FSM_CODE: &str = r#"// Oxidate - FSM Definition Example
// Syntax: Mermaid-like state diagram DSL

fsm TrafficLight {
    // Initial state
    [*] --> Red
    
    // State definitions
    state Red : Stop - Wait for green
    state Yellow : Caution - Prepare to stop
    state Green : Go - Proceed with caution
    
    // Transitions
    Red --> Green : timer_expired
    Green --> Yellow : timer_expired
    Yellow --> Red : timer_expired
}
"#;

const TRAFFIC_LIGHT_EXAMPLE: &str = r#"// Traffic Light State Machine
fsm TrafficLight {
    [*] --> Red
    
    state Red : Stop - Wait for green {
        entry / activate_red_light
        exit / deactivate_red_light
    }
    
    state Yellow : Caution {
        entry / activate_yellow_light
        exit / deactivate_yellow_light
    }
    
    state Green : Go! {
        entry / activate_green_light
        exit / deactivate_green_light
    }
    
    Red --> Green : timer_expired [day_mode]
    Red --> Yellow : timer_expired [night_mode]
    Green --> Yellow : timer_expired
    Yellow --> Red : timer_expired
}
"#;

const DOOR_LOCK_EXAMPLE: &str = r#"// Smart Door Lock State Machine
fsm DoorLock {
    [*] --> Locked
    
    state Locked : Door is secured {
        entry / engage_lock
        exit / disengage_lock
    }
    
    state Unlocked : Door can be opened {
        entry / notify_unlocked
    }
    
    state Open : Door is open {
        entry / start_open_timer
        exit / stop_open_timer
    }
    
    state Alarming : Security alert! {
        entry / sound_alarm
        exit / silence_alarm
    }
    
    Locked --> Unlocked : valid_key
    Locked --> Alarming : invalid_key [attempts > 3]
    Unlocked --> Locked : lock_cmd
    Unlocked --> Open : door_opened
    Open --> Unlocked : door_closed
    Open --> Alarming : timeout [held_too_long]
    Alarming --> Locked : reset_alarm
}
"#;

const VENDING_MACHINE_EXAMPLE: &str = r#"// Vending Machine State Machine
fsm VendingMachine {
    [*] --> Idle
    
    state Idle : Insert coins {
        entry / display_welcome
        exit / clear_display
    }
    
    state AcceptingCoins : Accepting payment {
        entry / show_balance
        coin_inserted / add_to_balance
    }
    
    state Dispensing : Delivering product {
        entry / dispense_product
        exit / update_inventory
    }
    
    state ReturningChange : Giving change {
        entry / calculate_change
        exit / dispense_change
    }
    
    Idle --> AcceptingCoins : coin_inserted
    AcceptingCoins --> AcceptingCoins : coin_inserted / add_coin
    AcceptingCoins --> Dispensing : select_product [sufficient_funds]
    AcceptingCoins --> Idle : cancel / return_coins
    Dispensing --> ReturningChange : dispensed [has_change]
    Dispensing --> Idle : dispensed [no_change]
    ReturningChange --> Idle : change_returned
}
"#;

#[cfg(test)]
mod layout_tests {
    //! Properties the edge routes must hold.
    //!
    //! Fixtures are real `dagre` output for `examples/door_lock.fsm`, captured by
    //! running the layout and printing the raw points. Keeping actual output here
    //! means these test the pipeline against the shapes dagre really produces,
    //! not against shapes invented to be convenient.

    use super::*;
    use std::collections::HashMap;

    const EPS: f32 = 0.6;

    /// Nodes are 180x70, all centred at x=156.5.
    fn door_lock_boxes() -> HashMap<String, egui::Rect> {
        let mut boxes = HashMap::new();
        for (name, y) in [
            ("Locked", 35.0),
            ("Unlocked", 215.0),
            ("Open", 395.0),
            ("Alarming", 575.0),
        ] {
            boxes.insert(
                name.to_string(),
                egui::Rect::from_center_size(egui::pos2(156.5, y), egui::vec2(180.0, 70.0)),
            );
        }
        boxes
    }

    fn door_lock_routes() -> Vec<(&'static str, &'static str, Vec<egui::Pos2>)> {
        vec![
            ("Locked", "Unlocked", vec![
                egui::pos2(179.0, 70.0), egui::pos2(214.0, 125.0), egui::pos2(179.0, 180.0)]),
            ("Locked", "Alarming", vec![
                egui::pos2(231.0, 70.0), egui::pos2(348.0, 125.0), egui::pos2(348.0, 215.0),
                egui::pos2(348.0, 305.0), egui::pos2(348.0, 395.0), egui::pos2(348.0, 485.0),
                egui::pos2(231.0, 540.0)]),
            ("Unlocked", "Locked", vec![
                egui::pos2(121.0, 180.0), egui::pos2(65.0, 125.0), egui::pos2(121.0, 70.0)]),
            ("Unlocked", "Open", vec![
                egui::pos2(176.0, 250.0), egui::pos2(207.0, 305.0), egui::pos2(176.0, 360.0)]),
            ("Open", "Unlocked", vec![
                egui::pos2(137.0, 360.0), egui::pos2(106.0, 305.0), egui::pos2(137.0, 250.0)]),
            ("Open", "Alarming", vec![
                egui::pos2(156.0, 430.0), egui::pos2(156.0, 485.0), egui::pos2(156.0, 540.0)]),
            ("Alarming", "Locked", vec![
                egui::pos2(92.0, 540.0), egui::pos2(-10.0, 485.0), egui::pos2(-10.0, 395.0),
                egui::pos2(-10.0, 305.0), egui::pos2(-10.0, 215.0), egui::pos2(-10.0, 125.0),
                egui::pos2(92.0, 70.0)]),
        ]
    }

    /// The same sequence `compute_layout_native` applies to dagre's output.
    fn routed() -> (HashMap<String, egui::Rect>, Vec<LayoutedEdge>) {
        let boxes = door_lock_boxes();
        let mut edges = Vec::new();
        for (i, (v, w, raw)) in door_lock_routes().into_iter().enumerate() {
            let squared = orthogonalise(&raw, LayoutDirection::TB, i);
            let clipped = clip_route_to_boxes(&squared, boxes[v], boxes[w]);
            edges.push(LayoutedEdge {
                v: v.to_string(),
                w: w.to_string(),
                transition_index: Some(i),
                points: clipped,
                transition_type: TransitionType::Forward,
            });
        }
        distribute_endpoints(&mut edges, &boxes);
        separate_overlapping_runs(&mut edges);
        (boxes, edges)
    }

    fn on_border(p: egui::Pos2, r: egui::Rect) -> bool {
        let vertical_edge = (p.x - r.left()).abs() < EPS || (p.x - r.right()).abs() < EPS;
        let horizontal_edge = (p.y - r.top()).abs() < EPS || (p.y - r.bottom()).abs() < EPS;
        let within_y = p.y >= r.top() - EPS && p.y <= r.bottom() + EPS;
        let within_x = p.x >= r.left() - EPS && p.x <= r.right() + EPS;
        (vertical_edge && within_y) || (horizontal_edge && within_x)
    }

    #[test]
    fn every_segment_is_axis_aligned() {
        // No 45-degree runs: each segment moves in x or in y, never both.
        let (_, edges) = routed();
        for edge in &edges {
            for pair in edge.points.windows(2) {
                let diagonal =
                    (pair[1].x - pair[0].x).abs() > EPS && (pair[1].y - pair[0].y).abs() > EPS;
                assert!(
                    !diagonal,
                    "{} -> {} has a diagonal segment {:?} -> {:?}",
                    edge.v, edge.w, pair[0], pair[1]
                );
            }
        }
    }

    #[test]
    fn endpoints_sit_on_the_box_borders() {
        // Arrowheads stopping short of a state, or buried inside it, were the
        // most visible routing defect.
        let (boxes, edges) = routed();
        for edge in &edges {
            let first = edge.points[0];
            let last = edge.points[edge.points.len() - 1];
            assert!(
                on_border(first, boxes[&edge.v]),
                "{} -> {} leaves from {:?}, not on the source border",
                edge.v, edge.w, first
            );
            assert!(
                on_border(last, boxes[&edge.w]),
                "{} -> {} arrives at {:?}, not on the target border",
                edge.v, edge.w, last
            );
        }
    }

    #[test]
    fn no_zero_length_segments() {
        // A degenerate final segment makes the arrowhead point nowhere.
        let (_, edges) = routed();
        for edge in &edges {
            for pair in edge.points.windows(2) {
                assert!(
                    (pair[1] - pair[0]).length() > EPS,
                    "{} -> {} has a zero-length segment at {:?}",
                    edge.v, edge.w, pair[0]
                );
            }
        }
    }

    #[test]
    fn routes_do_not_cut_through_unrelated_boxes() {
        let (boxes, edges) = routed();
        for edge in &edges {
            for (name, rect) in &boxes {
                if *name == edge.v || *name == edge.w {
                    continue;
                }
                // Touching a border is fine; passing through the interior is not.
                let interior = egui::Rect::from_min_max(
                    egui::pos2(rect.left() + 2.0, rect.top() + 2.0),
                    egui::pos2(rect.right() - 2.0, rect.bottom() - 2.0),
                );
                for p in &edge.points {
                    assert!(
                        !interior.contains(*p),
                        "{} -> {} passes through {}",
                        edge.v, edge.w, name
                    );
                }
            }
        }
    }

    #[test]
    fn label_anchors_lie_on_their_route() {
        // The anchor used to come from dagre and referred to the diagonal route
        // that no longer exists after squaring off.
        let (_, edges) = routed();
        for edge in &edges {
            let anchor = label_anchor(&edge.points);
            let on_route = edge.points.windows(2).any(|pair| {
                let (a, b) = (pair[0], pair[1]);
                let inside = anchor.x >= a.x.min(b.x) - EPS
                    && anchor.x <= a.x.max(b.x) + EPS
                    && anchor.y >= a.y.min(b.y) - EPS
                    && anchor.y <= a.y.max(b.y) + EPS;
                let aligned = ((b.x - a.x).abs() < EPS && (anchor.x - a.x).abs() < EPS)
                    || ((b.y - a.y).abs() < EPS && (anchor.y - a.y).abs() < EPS);
                inside && aligned
            });
            assert!(on_route, "{} -> {} label anchor {:?} is off the route", edge.v, edge.w, anchor);
        }
    }

    #[test]
    fn endpoints_on_one_side_stay_apart() {
        let (boxes, edges) = routed();
        let mut per_side: HashMap<(String, BoxSide), Vec<f32>> = HashMap::new();
        for edge in &edges {
            let last = edge.points[edge.points.len() - 1];
            let rect = boxes[&edge.w];
            let side = side_of(last, rect);
            let along = match side {
                BoxSide::Top | BoxSide::Bottom => last.x,
                BoxSide::Left | BoxSide::Right => last.y,
            };
            per_side.entry((edge.w.clone(), side)).or_default().push(along);
        }
        for (key, mut coords) in per_side {
            if coords.len() < 2 {
                continue;
            }
            coords.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let gap = coords.windows(2).map(|c| c[1] - c[0]).fold(f32::MAX, f32::min);
            assert!(gap > 8.0, "{key:?}: endpoints only {gap:.1}px apart");
        }
    }

    #[test]
    fn self_loop_is_closed_orthogonal_and_clear_of_the_state() {
        // A multi-line label used to spill back over the box it belonged to.
        let rect = egui::Rect::from_center_size(egui::pos2(0.0, 0.0), egui::vec2(180.0, 70.0));
        for text in ["short", "coin\ninserted / add\ncoin"] {
            let label_w = label_box_width(text, 12.0);
            let reach = rect.right() + 20.0 + label_w;
            let points = self_loop_route(rect, reach);

            for pair in points.windows(2) {
                let diagonal =
                    (pair[1].x - pair[0].x).abs() > EPS && (pair[1].y - pair[0].y).abs() > EPS;
                assert!(!diagonal, "self-loop segment is diagonal for {text:?}");
            }
            assert!((points[0].x - rect.right()).abs() < EPS);
            assert!((points[points.len() - 1].x - rect.right()).abs() < EPS);

            let label_pos = egui::pos2(rect.right() + 10.0 + label_w * 0.5, rect.center().y);
            let label_rect = egui::Rect::from_center_size(label_pos, egui::vec2(label_w, 40.0));
            assert!(!label_rect.intersects(rect), "self-loop label overlaps the state for {text:?}");
            assert!(reach >= label_rect.right(), "self-loop does not enclose its label for {text:?}");
        }
    }


    fn overlaps(edges: &[LayoutedEdge]) -> Vec<String> {
        let mut segments: Vec<(String, egui::Pos2, egui::Pos2)> = Vec::new();
        for edge in edges {
            for pair in edge.points.windows(2) {
                segments.push((format!("{}->{}", edge.v, edge.w), pair[0], pair[1]));
            }
        }

        let mut found = Vec::new();
        for i in 0..segments.len() {
            for j in i + 1..segments.len() {
                let (name_a, a0, a1) = &segments[i];
                let (name_b, b0, b1) = &segments[j];
                if name_a == name_b {
                    continue;
                }

                let both_horizontal = (a0.y - a1.y).abs() < EPS
                    && (b0.y - b1.y).abs() < EPS
                    && (a0.y - b0.y).abs() < EPS;
                if both_horizontal
                    && a0.x.min(a1.x) < b0.x.max(b1.x) - EPS
                    && b0.x.min(b1.x) < a0.x.max(a1.x) - EPS
                {
                    found.push(format!("horizontal at y={:.0}: {name_a} / {name_b}", a0.y));
                }

                let both_vertical = (a0.x - a1.x).abs() < EPS
                    && (b0.x - b1.x).abs() < EPS
                    && (a0.x - b0.x).abs() < EPS;
                if both_vertical
                    && a0.y.min(a1.y) < b0.y.max(b1.y) - EPS
                    && b0.y.min(b1.y) < a0.y.max(a1.y) - EPS
                {
                    found.push(format!("vertical at x={:.0}: {name_a} / {name_b}", a0.x));
                }
            }
        }
        found
    }

    #[test]
    fn no_two_routes_share_a_line() {
        let (_, edges) = routed();
        let found = overlaps(&edges);
        assert!(found.is_empty(), "routes drawn on top of each other: {found:#?}");
    }

    #[test]
    fn a_departure_and_an_arrival_do_not_share_a_border_point() {
        // Grouping them separately gave an outgoing edge and an incoming one the
        // same point on the same side, and they ran along each other from there.
        let (boxes, edges) = routed();

        let mut used: Vec<(String, f32, f32)> = Vec::new();
        for edge in &edges {
            for (name, point) in [
                (&edge.v, edge.points[0]),
                (&edge.w, edge.points[edge.points.len() - 1]),
            ] {
                if !boxes.contains_key(name) {
                    continue;
                }
                let clash = used.iter().any(|(other, x, y)| {
                    other == name && (x - point.x).abs() < EPS && (y - point.y).abs() < EPS
                });
                assert!(!clash, "two routes meet '{name}' at the same point {point:?}");
                used.push((name.clone(), point.x, point.y));
            }
        }
    }

    #[test]
    fn labels_avoid_boxes_other_routes_and_each_other() {
        // Three demands at once: a label may not sit on a state box, may not sit
        // on a route other than its own, and may not sit on another label.
        // Shoving after the fact never converged — each shove made the next
        // collision — so positions are proposed and scored instead.
        let boxes = door_lock_boxes();
        let font = 12.0_f32;

        let routes = vec![
            vec![
                egui::pos2(-40.0, 40.0),
                egui::pos2(-40.0, 150.0),
                egui::pos2(20.0, 150.0),
                egui::pos2(20.0, 245.0),
            ],
            vec![
                egui::pos2(30.0, 40.0),
                egui::pos2(30.0, 150.0),
                egui::pos2(90.0, 150.0),
                egui::pos2(90.0, 245.0),
            ],
            // A route crossing the area both labels would prefer.
            vec![egui::pos2(-200.0, 120.0), egui::pos2(200.0, 120.0)],
        ];

        let mut labels = vec![
            LayoutedLabel { pos: egui::pos2(-10.0, 150.0), text: "cancel / return\ncoins".into() },
            LayoutedLabel { pos: egui::pos2(60.0, 150.0), text: "coin_inserted".into() },
        ];
        let owners = vec![0usize, 1usize];
        let fixed = vec![false, false];

        place_labels(&mut labels, &routes, &owners, &fixed, &boxes, font);

        let size = |text: &str| {
            let lines = text.lines().count().max(1) as f32;
            egui::vec2(label_box_width(text, font), lines * font * 1.4 + 8.0)
        };

        for (i, label) in labels.iter().enumerate() {
            let rect = egui::Rect::from_center_size(label.pos, size(&label.text));

            for (name, state) in &boxes {
                assert!(!rect.intersects(*state), "label {i} sits on state '{name}'");
            }

            for (route_index, points) in routes.iter().enumerate() {
                if route_index == owners[i] {
                    continue; // its own line is what it labels
                }
                for pair in points.windows(2) {
                    assert!(
                        !segment_meets_rect(pair[0], pair[1], rect),
                        "label {i} sits on route {route_index}"
                    );
                }
            }

            for (j, other) in labels.iter().enumerate() {
                if i >= j {
                    continue;
                }
                assert!(
                    !rect.intersects(egui::Rect::from_center_size(other.pos, size(&other.text))),
                    "labels {i} and {j} overlap"
                );
            }
        }
    }

    #[test]
    fn a_label_never_ends_up_behind_a_state_box() {
        // Boxes are drawn after labels, so an overlap does not look like an
        // overlap — the label simply vanishes. Scoring alone picks the least bad
        // candidate, which on a route hugging a state is still one that overlaps,
        // so clearing a box is enforced rather than preferred.
        let mut boxes = std::collections::HashMap::new();
        boxes.insert(
            "Idle".to_string(),
            egui::Rect::from_center_size(egui::pos2(0.0, 0.0), egui::vec2(240.0, 100.0)),
        );

        // A route that wraps the box: every natural candidate lands on it.
        let routes = vec![vec![
            egui::pos2(-60.0, 55.0),
            egui::pos2(-60.0, 20.0),
            egui::pos2(60.0, 20.0),
            egui::pos2(60.0, 55.0),
        ]];
        let mut labels = vec![LayoutedLabel {
            pos: egui::pos2(0.0, 20.0),
            text: "cancel / return\ncoins".into(),
        }];

        place_labels(&mut labels, &routes, &[0], &[false], &boxes, 12.0);

        let text = &labels[0].text;
        let size = egui::vec2(
            label_box_width(text, 12.0),
            text.lines().count() as f32 * 12.0 * 1.4 + 8.0,
        );
        assert!(
            !egui::Rect::from_center_size(labels[0].pos, size).intersects(boxes["Idle"]),
            "label still behind the state box at {:?}",
            labels[0].pos
        );
    }

    #[test]
    fn neighbouring_labels_line_up() {
        // Two labels on parallel routes reading at different heights looks
        // accidental. The score pays a bonus for sharing an edge with one
        // already placed.
        let boxes = door_lock_boxes();
        let routes = vec![
            vec![egui::pos2(-40.0, 40.0), egui::pos2(-40.0, 245.0)],
            vec![egui::pos2(90.0, 40.0), egui::pos2(90.0, 245.0)],
        ];
        let mut labels = vec![
            LayoutedLabel { pos: egui::pos2(-40.0, 140.0), text: "lock_cmd".into() },
            LayoutedLabel { pos: egui::pos2(90.0, 175.0), text: "valid_key".into() },
        ];

        place_labels(
            &mut labels,
            &routes,
            &[0, 1],
            &[false, false],
            &boxes,
            12.0,
        );

        let dy = (labels[0].pos.y - labels[1].pos.y).abs();
        let dx = (labels[0].pos.x - labels[1].pos.x).abs();
        assert!(dy < 3.0 || dx < 3.0, "labels not aligned: dx={dx:.1} dy={dy:.1}");
    }

    #[test]
    fn a_label_with_nothing_near_it_is_left_alone() {
        // Displacement is a last resort: a label that reads fine should not move.
        let boxes = door_lock_boxes();
        let original = egui::pos2(900.0, 900.0);
        let mut labels = vec![LayoutedLabel { pos: original, text: "clear".into() }];

        let routes: Vec<Vec<egui::Pos2>> = Vec::new();
        let owners = vec![0usize; labels.len()];
        let fixed = vec![false; labels.len()];
        place_labels(&mut labels, &routes, &owners, &fixed, &boxes, 12.0);

        assert_eq!(labels[0].pos, original);
    }
}
