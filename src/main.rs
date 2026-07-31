//! Oxidate GUI - FSM Visualizer
//! Interactive GUI for creating and visualizing Finite State Machines

use eframe::egui;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod fsm;
mod parser;
mod codegen;

use fsm::{FsmDefinition, StateType};
use parser::parse_fsm;
use codegen::{generate_rust_code_with_target, CodegenTarget};

use serde::{Deserialize, Serialize};

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

fn dagre_demo_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("OXIDATE_DAGRE_DIR") {
        let p = PathBuf::from(dir);
        if p.join("src/layout_json.mjs").exists() {
            return p;
        }
    }

    // When bundled on macOS, resources live at:
    //   Oxidate.app/Contents/Resources/
    // and our demo is copied to:
    //   .../Resources/tools/dagre-svg-demo/
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // macOS bundle: Contents/MacOS/<bin>
            if let Some(contents_dir) = exe_dir.parent() {
                let resources = contents_dir.join("Resources").join("tools/dagre-svg-demo");
                if resources.join("src/layout_json.mjs").exists() {
                    return resources;
                }
            }

            // Generic "resources" layout (zip/AppDir): <exe_dir>/resources/tools/dagre-svg-demo
            let resources = exe_dir.join("resources").join("tools/dagre-svg-demo");
            if resources.join("src/layout_json.mjs").exists() {
                return resources;
            }

            // Next to executable: <exe_dir>/tools/dagre-svg-demo
            let sibling = exe_dir.join("tools/dagre-svg-demo");
            if sibling.join("src/layout_json.mjs").exists() {
                return sibling;
            }

            // AppImage-style: <AppDir>/usr/bin/<bin> → <AppDir>/usr/share/oxidate/tools/dagre-svg-demo
            if let Some(usr_dir) = exe_dir.parent() {
                let appimage = usr_dir.join("share/oxidate/tools/dagre-svg-demo");
                if appimage.join("src/layout_json.mjs").exists() {
                    return appimage;
                }
            }
        }
    }

    // Dev fallback
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools/dagre-svg-demo")
}

fn node_binary() -> PathBuf {
    if let Ok(p) = std::env::var("OXIDATE_NODE") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return pb;
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // macOS bundle: Contents/MacOS/<bin> → Contents/Resources/node/bin/node
            if let Some(contents_dir) = exe_dir.parent() {
                let mac_node = contents_dir.join("Resources/node/bin/node");
                if mac_node.exists() {
                    return mac_node;
                }
            }

            // Windows zip: <exe_dir>/node/node.exe
            #[cfg(windows)]
            {
                let win_node = exe_dir.join("node/node.exe");
                if win_node.exists() {
                    return win_node;
                }
                let win_node = exe_dir.join("resources/node/node.exe");
                if win_node.exists() {
                    return win_node;
                }
            }

            // Linux AppImage / generic: <AppDir>/usr/bin/<bin> → <AppDir>/usr/lib/oxidate/node/bin/node
            #[cfg(not(windows))]
            {
                let unix_node = exe_dir.join("node/bin/node");
                if unix_node.exists() {
                    return unix_node;
                }
                if let Some(usr_dir) = exe_dir.parent() {
                    let appimage_node = usr_dir.join("lib/oxidate/node/bin/node");
                    if appimage_node.exists() {
                        return appimage_node;
                    }
                }
            }
        }
    }

    // Fallback to PATH lookup.
    PathBuf::from("node")
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
    auto_event: String,
    auto_period_s: f32,
    auto_accum_s: f32,

    last_frame: Option<Instant>,
    last_fired: Option<SimFired>,
    log: Vec<String>,
}

#[derive(Clone, Debug)]
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
            auto_event: "timer_expired".to_string(),
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
            codegen_target: CodegenTarget::Embassy, // Default to Embassy for embedded
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
            self.generated_code = generate_rust_code_with_target(fsm, self.codegen_target);
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

    fn compute_layout_with_dagre(&mut self, ctx: &egui::Context, fsm: &FsmDefinition) -> Result<(), String> {
        #[derive(Serialize)]
        struct JsGraphCfg {
            rankdir: String,
            nodesep: f32,
            ranksep: f32,
            edgesep: f32,
            marginx: f32,
            marginy: f32,
        }

        #[derive(Serialize)]
        struct JsNodeIn {
            id: String,
            width: f32,
            height: f32,
        }

        #[derive(Serialize)]
        struct JsEdgeIn {
            v: String,
            w: String,
            name: Option<String>,
            #[serde(rename = "labelWidth")]
            label_width: Option<f32>,
            #[serde(rename = "labelHeight")]
            label_height: Option<f32>,
        }

        #[derive(Serialize)]
        struct JsLayoutInput {
            graph: JsGraphCfg,
            nodes: Vec<JsNodeIn>,
            edges: Vec<JsEdgeIn>,
        }

        #[derive(Deserialize)]
        struct JsPoint {
            x: f32,
            y: f32,
        }

        #[derive(Deserialize)]
        struct JsNodeOut {
            x: f32,
            y: f32,
            width: f32,
            height: f32,
        }

        #[derive(Deserialize)]
        struct JsGraphOut {
            width: f32,
            height: f32,
        }

        #[derive(Deserialize)]
        struct JsEdgeOut {
            v: String,
            w: String,
            name: Option<String>,
            points: Vec<JsPoint>,
            x: Option<f32>,
            y: Option<f32>,
        }

        #[derive(Deserialize)]
        struct JsLayoutOutput {
            graph: JsGraphOut,
            nodes: std::collections::HashMap<String, JsNodeOut>,
            edges: Vec<JsEdgeOut>,
        }

        // Graph config to send to JS Dagre.
        let graph_cfg = JsGraphCfg {
            rankdir: match self.layout_config.direction {
                LayoutDirection::TB => "tb".to_string(),
                LayoutDirection::LR => "lr".to_string(),
            },
            nodesep: self.layout_config.nodesep,
            ranksep: self.layout_config.ranksep,
            edgesep: self.layout_config.edgesep,
            marginx: self.layout_config.marginx,
            marginy: self.layout_config.marginy,
        };

        // Nodes.
        let mut nodes_in: Vec<JsNodeIn> = Vec::new();
        for state in &fsm.states {
            let size = estimate_state_size(state);
            nodes_in.push(JsNodeIn {
                id: state.name.clone(),
                width: size.x,
                height: size.y,
            });
        }

        // Pseudo start node.
        let start_id = "[*]".to_string();
        let has_start = fsm.transitions.iter().any(|t| t.source == "[*]");
        if has_start {
            nodes_in.push(JsNodeIn {
                id: start_id.clone(),
                width: 16.0,
                height: 16.0,
            });
        }

        // Represent every transition as an intermediate node (optionally sized to the label).
        let mut transition_node_type: std::collections::HashMap<String, TransitionType> = std::collections::HashMap::new();
        let mut label_node_text: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut edges_in: Vec<JsEdgeIn> = Vec::new();

        for (t_idx, transition) in fsm.transitions.iter().enumerate() {
            if transition.source == "[*]" {
                edges_in.push(JsEdgeIn {
                    v: start_id.clone(),
                    w: transition.target.clone(),
                    name: Some(format!("start_{t_idx}")),
                    label_width: Some(0.0),
                    label_height: Some(0.0),
                });
                continue;
            }

            let transition_node_id = format!("__tr_{t_idx}");

            let raw_label = transition.label();
            let label = format_label_text(&raw_label);

            // Styling only (does NOT affect layout/routing)
            let transition_type = {
                let event_name = transition.event.as_ref().map(|e| e.name.to_lowercase()).unwrap_or_default();
                if event_name.contains("timeout") || event_name.contains("timer") || event_name.contains("expired") {
                    TransitionType::Timer
                } else if transition.guard.is_some() {
                    TransitionType::Conditional
                } else {
                    TransitionType::Forward
                }
            };

            transition_node_type.insert(transition_node_id.clone(), transition_type);

            if label.is_empty() {
                nodes_in.push(JsNodeIn {
                    id: transition_node_id.clone(),
                    width: 1.0,
                    height: 1.0,
                });
            } else {
                let label_size = Self::measure_text(ctx, &label, self.layout_config.edge_label_font_size);
                nodes_in.push(JsNodeIn {
                    id: transition_node_id.clone(),
                    width: label_size.x + 14.0,
                    height: label_size.y + 8.0,
                });
                label_node_text.insert(transition_node_id.clone(), label);
            }

            edges_in.push(JsEdgeIn {
                v: transition.source.clone(),
                w: transition_node_id.clone(),
                name: Some(format!("tr_{t_idx}_a")),
                label_width: Some(0.0),
                label_height: Some(0.0),
            });
            edges_in.push(JsEdgeIn {
                v: transition_node_id.clone(),
                w: transition.target.clone(),
                name: Some(format!("tr_{t_idx}_b")),
                label_width: Some(0.0),
                label_height: Some(0.0),
            });
        }

        let input = JsLayoutInput {
            graph: graph_cfg,
            nodes: nodes_in,
            edges: edges_in,
        };

        // Run JS Dagre (requires `npm install` in tools/dagre-svg-demo).
        let demo_dir = dagre_demo_dir();
        let script = demo_dir.join("src/layout_json.mjs");
        if !script.exists() {
            return Err(format!(
                "Dagre layout script not found at: {}\n\nThis usually means the bundled resources are missing.\n\nDev: ensure tools/dagre-svg-demo exists.\nPackaged: ensure tools/dagre-svg-demo is shipped alongside the app (or set OXIDATE_DAGRE_DIR).",
                script.display()
            ));
        }
        let input_json = serde_json::to_vec(&input).map_err(|e| format!("Failed to serialize layout input: {e}"))?;

        let node = node_binary();
        let mut child = Command::new(&node)
            .current_dir(&demo_dir)
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                format!(
                    "Failed to spawn Node.js ({}): {e}\n\nIf Node is not installed, install it OR bundle it and set OXIDATE_NODE.\nAlso run: `cd tools/dagre-svg-demo && npm install` (or ship node_modules in releases).",
                    node.display()
                )
            })?;

        {
            let stdin = child.stdin.as_mut().ok_or_else(|| "Failed to open stdin for Node.js".to_string())?;
            stdin
                .write_all(&input_json)
                .map_err(|e| format!("Failed to write to Node.js stdin: {e}"))?;
        }

        let output = child
            .wait_with_output()
            .map_err(|e| format!("Failed to wait for Node.js: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "Dagre (Node.js) layout failed.\n\nIf you haven't yet, run: `cd tools/dagre-svg-demo && npm install`\n\nError:\n{}",
                stderr.trim()
            ));
        }

        let js_layout: JsLayoutOutput = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse Dagre output JSON: {e}"))?;

        // Compute center using returned nodes/edge points.
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        for n in js_layout.nodes.values() {
            min_x = min_x.min(n.x - n.width * 0.5);
            max_x = max_x.max(n.x + n.width * 0.5);
            min_y = min_y.min(n.y - n.height * 0.5);
            max_y = max_y.max(n.y + n.height * 0.5);
        }
        for e in &js_layout.edges {
            for p in &e.points {
                min_x = min_x.min(p.x);
                max_x = max_x.max(p.x);
                min_y = min_y.min(p.y);
                max_y = max_y.max(p.y);
            }
        }
        if !min_x.is_finite() {
            min_x = 0.0;
            min_y = 0.0;
            max_x = 0.0;
            max_y = 0.0;
        }
        let center = egui::vec2((min_x + max_x) * 0.5, (min_y + max_y) * 0.5);

        self.state_positions.clear();
        for (id, n) in js_layout.nodes.iter() {
            self.state_positions.insert(id.clone(), egui::pos2(n.x - center.x, n.y - center.y));
        }

        let mut layout_edges: Vec<LayoutedEdge> = Vec::new();
        for e in &js_layout.edges {
            let transition_type = if e.v.starts_with("__tr_") {
                transition_node_type.get(&e.v).copied().unwrap_or(TransitionType::Forward)
            } else if e.w.starts_with("__tr_") {
                transition_node_type.get(&e.w).copied().unwrap_or(TransitionType::Forward)
            } else {
                TransitionType::Forward
            };

            let transition_index = e
                .name
                .as_deref()
                .and_then(|name| name.strip_prefix("tr_"))
                .and_then(|rest| rest.split('_').next())
                .and_then(|n| n.parse::<usize>().ok());

            layout_edges.push(LayoutedEdge {
                v: e.v.clone(),
                w: e.w.clone(),
                transition_index,
                points: e.points.iter().map(|p| egui::pos2(p.x - center.x, p.y - center.y)).collect(),
                transition_type,
            });
        }

        let mut layout_labels: Vec<LayoutedLabel> = Vec::new();
        for (label_node_id, text) in label_node_text.iter() {
            if let Some(n) = js_layout.nodes.get(label_node_id) {
                layout_labels.push(LayoutedLabel {
                    pos: egui::pos2(n.x - center.x, n.y - center.y),
                    text: text.clone(),
                });
            }
        }

        self.layout = Some(LayoutedDiagram { edges: layout_edges, labels: layout_labels });
        Ok(())
    }

    fn calculate_state_positions(&mut self) {
        self.state_positions.clear();
        
        if let Some(fsm) = self.fsms.get(self.selected_fsm) {
            let num_states = fsm.states.len();
            if num_states == 0 {
                return;
            }

            // Calculate state sizes first for proper spacing
            let state_sizes: Vec<(String, egui::Vec2)> = fsm.states.iter()
                .map(|s| (s.name.clone(), estimate_state_size(s)))
                .collect();
            
            // Find max dimensions
            let max_width = state_sizes.iter().map(|(_, sz)| sz.x).fold(0.0f32, |a, b| a.max(b));
            let max_height = state_sizes.iter().map(|(_, sz)| sz.y).fold(0.0f32, |a, b| a.max(b));
            
            // Use much larger spacing to avoid collisions - significantly increased
            let base_spacing_x = max_width + 280.0;  // Horizontal spacing
            let base_spacing_y = max_height + 220.0; // Vertical spacing
            
            // Try to arrange in a grid that accommodates the FSM structure
            // Analyze transitions to find levels
            let levels = calculate_state_levels(fsm);
            
            if levels.is_empty() {
                // Fallback: simple circle layout with large radius
                let radius = (num_states as f32 * 50.0).max(200.0);
                let center = egui::Pos2::new(0.0, 0.0);
                
                for (i, state) in fsm.states.iter().enumerate() {
                    let angle = (i as f32 / num_states as f32) * 2.0 * std::f32::consts::PI - std::f32::consts::FRAC_PI_2;
                    let x = center.x + radius * angle.cos();
                    let y = center.y + radius * angle.sin();
                    self.state_positions.insert(state.name.clone(), egui::Pos2::new(x, y));
                }
            } else {
                // Use hierarchical layout based on levels
                let mut level_counts: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
                
                for (_, level) in &levels {
                    *level_counts.entry(*level).or_insert(0) += 1;
                }
                
                let max_level = levels.values().max().copied().unwrap_or(0);
                let mut level_current: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
                
                for (state_name, level) in &levels {
                    let count_in_level = level_counts.get(level).copied().unwrap_or(1);
                    let idx_in_level = *level_current.entry(*level).or_insert(0);
                    *level_current.get_mut(level).unwrap() += 1;
                    
                    // Center the states in each level
                    let level_width = (count_in_level - 1) as f32 * base_spacing_x;
                    let start_x = -level_width / 2.0;
                    
                    let x = start_x + idx_in_level as f32 * base_spacing_x;
                    let y = *level as f32 * base_spacing_y;
                    
                    self.state_positions.insert(state_name.clone(), egui::Pos2::new(x, y));
                }
                
                // Apply force-directed adjustment to reduce overlaps
                self.apply_force_layout(&levels, base_spacing_x * 0.8, base_spacing_y * 0.6);
            }
        }
    }
    
    /// Apply force-directed layout adjustment
    fn apply_force_layout(&mut self, levels: &std::collections::HashMap<String, i32>, min_x: f32, min_y: f32) {
        let iterations = 100;
        let repulsion = 15000.0;
        let attraction = 0.01;
        
        for _ in 0..iterations {
            let positions_copy: Vec<(String, egui::Pos2)> = self.state_positions.iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect();
            
            for (name, pos) in positions_copy.iter() {
                let mut force = egui::Vec2::ZERO;
                let my_level = levels.get(name).copied().unwrap_or(0);
                
                // Repulsion from other nodes
                for (other_name, other_pos) in positions_copy.iter() {
                    if name == other_name {
                        continue;
                    }
                    
                    let diff = *pos - *other_pos;
                    let dist = diff.length().max(50.0);
                    
                    // Stronger repulsion for same level
                    let other_level = levels.get(other_name).copied().unwrap_or(0);
                    let level_factor = if my_level == other_level { 2.0 } else { 1.0 };
                    
                    force += diff.normalized() * (repulsion * level_factor / (dist * dist));
                }
                
                // Attraction to center of level (horizontal only)
                force.x += -pos.x * attraction;
                
                // Apply force with damping
                let new_pos = *pos + force * 0.5;
                
                // Enforce minimum distances
                let mut final_pos = new_pos;
                for (other_name, other_pos) in positions_copy.iter() {
                    if name == other_name {
                        continue;
                    }
                    
                    let diff = final_pos - *other_pos;
                    let dx = diff.x.abs();
                    let dy = diff.y.abs();
                    
                    let other_level = levels.get(other_name).copied().unwrap_or(0);
                    
                    // Enforce minimum distances
                    if my_level == other_level && dx < min_x {
                        let push = (min_x - dx) / 2.0 + 10.0;
                        if diff.x >= 0.0 {
                            final_pos.x += push;
                        } else {
                            final_pos.x -= push;
                        }
                    }
                    
                    if my_level != other_level && dy < min_y {
                        let push = (min_y - dy) / 2.0 + 10.0;
                        if diff.y >= 0.0 {
                            final_pos.y += push;
                        } else {
                            final_pos.y -= push;
                        }
                    }
                }
                
                self.state_positions.insert(name.clone(), final_pos);
            }
        }
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
            
            // Generate code for each target
            let code = generate_rust_code_with_target(fsm, self.codegen_target);
            
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
        self.sim.last_fired = None;
        self.sim.last_frame = None;

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
                match self.compute_layout_with_dagre(ctx, &fsm) {
                    Ok(()) => {
                        // Keep parse errors (if any) intact; only clear layout-related errors.
                        if let Some(msg) = &self.error_message {
                            if msg.starts_with("Layout error:") {
                                self.error_message = None;
                            }
                        }
                    }
                    Err(e) => {
                        self.error_message = Some(format!("Layout error: {e}"));
                        self.layout = None;
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
                                ui.selectable_value(&mut self.codegen_target, CodegenTarget::Standard, "🖥 Standard (std)");
                                ui.selectable_value(&mut self.codegen_target, CodegenTarget::Embassy, "🔌 Embassy (async embedded)");
                                ui.selectable_value(&mut self.codegen_target, CodegenTarget::Rtic, "⚡ RTIC (interrupt-driven)");
                            });
                        if self.codegen_target != prev_target {
                            self.regenerate_code();
                        }
                    });
                    
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
                            self.generated_code = generate_rust_code_with_target(fsm, self.codegen_target);
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
                                self.generated_code = generate_rust_code_with_target(fsm, self.codegen_target);
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
                egui::ComboBox::from_id_source("layout_direction")
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
                        ui.label("Event:");
                        ui.text_edit_singleline(&mut self.sim.event_input);
                        if ui.button("Post").clicked() {
                            let ev = self.sim.event_input.trim().to_string();
                            self.sim_post_event(ev);
                            self.sim.event_input.clear();
                        }
                        ui.separator();
                        ui.checkbox(&mut self.sim.auto_tick, "Auto");
                        ui.add(egui::DragValue::new(&mut self.sim.auto_period_s).speed(0.1).clamp_range(0.1..=10.0).prefix("period ").suffix("s"));
                        ui.label("event");
                        ui.text_edit_singleline(&mut self.sim.auto_event);
                        if ui.button("Clear log").clicked() {
                            self.sim.log.clear();
                        }
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
                                let ev = self.sim.auto_event.trim().to_string();
                                if !ev.is_empty() {
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

/// Information about a state box for collision detection
#[derive(Clone)]
struct StateBox {
    rect: egui::Rect,
}

/// Lane allocation for exclusive routing - each transition gets its own lane
struct LaneAllocator {
    /// Used lanes for horizontal segments at different Y positions
    horizontal_lanes: Vec<f32>,
    /// Used lanes for vertical segments at different X positions  
    vertical_lanes: Vec<f32>,
    /// Minimum spacing between lanes
    lane_spacing: f32,
}

impl LaneAllocator {
    fn new(zoom: f32) -> Self {
        Self {
            horizontal_lanes: Vec::new(),
            vertical_lanes: Vec::new(),
            lane_spacing: 35.0 * zoom, // Fixed spacing between lanes
        }
    }
    
    /// Allocate an exclusive horizontal lane, returns Y position
    fn allocate_horizontal_lane(&mut self, preferred_y: f32) -> f32 {
        // Find a lane that doesn't conflict with existing ones
        let mut y = preferred_y;
        let mut iteration = 0;
        
        let max_iterations = 6; // keep routes compact (avoid global detours)
        loop {
            let conflicts = self.horizontal_lanes.iter()
                .any(|&existing| (existing - y).abs() < self.lane_spacing);
            
            if !conflicts {
                self.horizontal_lanes.push(y);
                return y;
            }
            
            // Try alternating above/below
            iteration += 1;
            let offset = (iteration as f32 / 2.0).ceil() * self.lane_spacing;
            y = if iteration % 2 == 0 {
                preferred_y + offset
            } else {
                preferred_y - offset
            };
            
            if iteration >= max_iterations {
                // Fall back to preferred (compact) even if it reuses a lane.
                self.horizontal_lanes.push(preferred_y);
                return preferred_y;
            }
        }
    }
    
    /// Allocate an exclusive vertical lane, returns X position
    fn allocate_vertical_lane(&mut self, preferred_x: f32) -> f32 {
        let mut x = preferred_x;
        let mut iteration = 0;

        let max_iterations = 6; // keep routes compact (avoid global detours)
        loop {
            let conflicts = self.vertical_lanes.iter()
                .any(|&existing| (existing - x).abs() < self.lane_spacing);
            
            if !conflicts {
                self.vertical_lanes.push(x);
                return x;
            }
            
            iteration += 1;
            let offset = (iteration as f32 / 2.0).ceil() * self.lane_spacing;
            x = if iteration % 2 == 0 {
                preferred_x + offset
            } else {
                preferred_x - offset
            };
            
            if iteration >= max_iterations {
                // Fall back to preferred (compact) even if it reuses a lane.
                self.vertical_lanes.push(preferred_x);
                return preferred_x;
            }
        }
    }
}

/// Determine relative position of two states for clockwise routing
fn get_relative_position(from: egui::Pos2, to: egui::Pos2) -> &'static str {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    
    if dx.abs() > dy.abs() {
        if dx > 0.0 { "right" } else { "left" }
    } else {
        if dy > 0.0 { "below" } else { "above" }
    }
}

/// Calculate orthogonal route with EXCLUSIVE lane allocation
fn calculate_orthogonal_route_with_lanes(
    from_rect: egui::Rect,
    to_rect: egui::Rect,
    lane_index: i32,
    zoom: f32,
    transition_type: TransitionType,
    lane_allocator: &mut LaneAllocator,
) -> Vec<egui::Pos2> {
    let mut points = Vec::new();
    
    let from = from_rect.center();
    let to = to_rect.center();
    
    // Gap from state edge
    let gap = 12.0 * zoom;
    
    // Base lane offset - each transition gets progressively further lanes
    let lane_offset = lane_index.abs() as f32 * lane_allocator.lane_spacing;
    
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    
    let is_return = lane_index < 0;
    let position = get_relative_position(from, to);
    
    match transition_type {
        TransitionType::Timer => {
            // Timer transitions: keep routing LOCAL and compact (Mermaid-like).
            // Use ONE outside vertical lane X and ONE top lane Y (bounded), so we never create
            // screen-wrapping rectangles.
            let exit_point = egui::pos2(from_rect.center().x, from_rect.top() - gap);
            let entry_point = egui::pos2(to_rect.center().x, to_rect.top() - gap);

            // Local window around the two states
            let margin = 220.0 * zoom;
            let bbox = from_rect.union(to_rect).expand(margin);

            // Top lane above the local bbox
            let top_y = bbox.top() - (40.0 * zoom + lane_offset);
            let lane_y = lane_allocator.allocate_horizontal_lane(top_y);

            // Outside lane X (left/right) separated by lane_index
            let side = if lane_index % 2 == 0 { -1.0 } else { 1.0 };
            let desired_x = ((from_rect.center().x + to_rect.center().x) * 0.5)
                + side * (70.0 * zoom + lane_offset);
            let clamp_left = bbox.left() - 60.0 * zoom;
            let clamp_right = bbox.right() + 60.0 * zoom;
            let lane_x = lane_allocator.allocate_vertical_lane(desired_x.clamp(clamp_left, clamp_right));

            points.push(exit_point);
            points.push(egui::pos2(lane_x, exit_point.y));
            points.push(egui::pos2(lane_x, lane_y));
            points.push(egui::pos2(entry_point.x, lane_y));
            points.push(entry_point);
        }
        TransitionType::Return | TransitionType::Conditional => {
            // Return/Conditional: route OUTSIDE the main shape
            match position {
                "right" | "left" => {
                    // Route below for horizontal returns
                    let exit_point = egui::pos2(from_rect.center().x, from_rect.bottom() + gap);
                    let entry_point = egui::pos2(to_rect.center().x, to_rect.bottom() + gap);
                    
                    let bottom_y = from_rect.bottom().max(to_rect.bottom()) + 50.0 * zoom + lane_offset;
                    let lane_y = lane_allocator.allocate_horizontal_lane(bottom_y);
                    
                    points.push(exit_point);
                    points.push(egui::pos2(exit_point.x, lane_y));
                    points.push(egui::pos2(entry_point.x, lane_y));
                    points.push(entry_point);
                }
                "above" | "below" => {
                    // Route to the side for vertical returns
                    let side = if is_return { -1.0 } else { 1.0 };
                    let exit_point = if side > 0.0 {
                        egui::pos2(from_rect.right() + gap, from_rect.center().y)
                    } else {
                        egui::pos2(from_rect.left() - gap, from_rect.center().y)
                    };
                    let entry_point = if side > 0.0 {
                        egui::pos2(to_rect.right() + gap, to_rect.center().y)
                    } else {
                        egui::pos2(to_rect.left() - gap, to_rect.center().y)
                    };
                    
                    let side_x = if side > 0.0 {
                        from_rect.right().max(to_rect.right()) + 50.0 * zoom + lane_offset
                    } else {
                        from_rect.left().min(to_rect.left()) - 50.0 * zoom - lane_offset
                    };
                    let lane_x = lane_allocator.allocate_vertical_lane(side_x);
                    
                    points.push(exit_point);
                    points.push(egui::pos2(lane_x, exit_point.y));
                    points.push(egui::pos2(lane_x, entry_point.y));
                    points.push(entry_point);
                }
                _ => {}
            }
        }
        TransitionType::Forward => {
            // Forward transitions: direct routes with exclusive lanes
            if dx.abs() > dy.abs() * 0.5 {
                // Horizontal dominant
                let going_right = dx > 0.0;
                
                // Exit from appropriate side
                let exit_y = from_rect.center().y;
                let entry_y = to_rect.center().y;
                
                let exit_point = if going_right {
                    egui::pos2(from_rect.right() + gap, exit_y)
                } else {
                    egui::pos2(from_rect.left() - gap, exit_y)
                };
                
                let entry_point = if going_right {
                    egui::pos2(to_rect.left() - gap, entry_y)
                } else {
                    egui::pos2(to_rect.right() + gap, entry_y)
                };
                
                // Allocate exclusive vertical lane for the middle segment
                let mid_x = (exit_point.x + entry_point.x) / 2.0 + lane_offset * if going_right { 1.0 } else { -1.0 };
                let lane_x = lane_allocator.allocate_vertical_lane(mid_x);
                
                points.push(exit_point);
                points.push(egui::pos2(lane_x, exit_point.y));
                points.push(egui::pos2(lane_x, entry_point.y));
                points.push(entry_point);
            } else {
                // Vertical dominant
                let going_down = dy > 0.0;
                
                let exit_x = from_rect.center().x;
                let entry_x = to_rect.center().x;
                
                let exit_point = if going_down {
                    egui::pos2(exit_x, from_rect.bottom() + gap)
                } else {
                    egui::pos2(exit_x, from_rect.top() - gap)
                };
                
                let entry_point = if going_down {
                    egui::pos2(entry_x, to_rect.top() - gap)
                } else {
                    egui::pos2(entry_x, to_rect.bottom() + gap)
                };
                
                // Allocate exclusive horizontal lane for middle segment
                let mid_y = (exit_point.y + entry_point.y) / 2.0 + lane_offset * if going_down { 1.0 } else { -1.0 };
                let lane_y = lane_allocator.allocate_horizontal_lane(mid_y);
                
                points.push(exit_point);
                points.push(egui::pos2(exit_point.x, lane_y));
                points.push(egui::pos2(entry_point.x, lane_y));
                points.push(entry_point);
            }
        }
    }
    
    points
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

/// Calculate label position - NEVER on the arrow, always offset to the side
/// Rules:
/// - Entry transitions: label on LEFT
/// - Exit transitions: label on RIGHT  
/// - Timer events: label ABOVE
/// - All labels have large offset from arrows
fn calculate_label_position(
    route: &[egui::Pos2], 
    offset_index: i32, 
    zoom: f32,
    transition_type: TransitionType,
    from_rect: egui::Rect,
    to_rect: egui::Rect,
) -> egui::Pos2 {
    if route.len() < 2 {
        return egui::Pos2::ZERO;
    }
    
    let p1 = route[0];
    let p2 = if route.len() > 1 { route[1] } else { route[0] };
    
    // Calculate direction of first segment
    let dir = (p2 - p1).normalized();
    let perp = egui::vec2(-dir.y, dir.x);
    
    // Base offset - LARGE to ensure no collision with arrow
    let base_perpendicular_offset = 50.0 * zoom;
    let index_offset = offset_index.abs() as f32 * 25.0 * zoom;
    
    match transition_type {
        TransitionType::Timer => {
            // Timer events: position ABOVE and offset sideways (never on the arrow).
            // Anchor near the routed lane (route[1] tends to be the timer's lane X).
            let anchor = route.get(1).copied().unwrap_or_else(|| from_rect.center());
            let side = if offset_index % 2 == 0 { -1.0 } else { 1.0 };
            let label_x = anchor.x + side * (55.0 * zoom + index_offset * 0.2);
            let label_y = anchor.y - (28.0 * zoom + index_offset * 0.4);
            egui::pos2(label_x, label_y)
        }
        TransitionType::Return | TransitionType::Conditional => {
            // Curved transitions: position along the outer curve
            // Find the midpoint of the curved path
            if route.len() >= 3 {
                let mid_idx = route.len() / 2;
                let curve_point = route[mid_idx];
                
                // Offset further from the curve
                let to_center = (from_rect.center() + to_rect.center().to_vec2()) * 0.5;
                let away_dir = (curve_point - to_center).normalized();
                
                curve_point + away_dir * (30.0 * zoom + index_offset)
            } else {
                // Fallback
                let along_pos = p1 + (p2 - p1) * 0.3;
                let side = if offset_index >= 0 { 1.0 } else { -1.0 };
                along_pos + perp * (base_perpendicular_offset + index_offset) * side
            }
        }
        TransitionType::Forward => {
            // Straight transitions: position at 30% along, offset to the side
            let along_pos = p1 + (p2 - p1) * 0.3;
            
            // Determine side based on direction (entry = left, exit = right)
            // If going right/down, label on top/left; if going left/up, label on bottom/right
            let side = if dir.x > 0.0 || dir.y > 0.0 { 1.0 } else { -1.0 };
            let side = side * (if offset_index >= 0 { 1.0 } else { -1.0 });
            
            along_pos + perp * (base_perpendicular_offset + index_offset) * side
        }
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

/// Calculate label info for orthogonal transition
fn calculate_label_info_orthogonal(
    route: &[egui::Pos2],
    transition: &fsm::Transition,
    zoom: f32,
    offset_index: i32,
    transition_type: TransitionType,
    from_rect: egui::Rect,
    to_rect: egui::Rect,
) -> Option<LabelInfo> {
    let raw_label = transition.label();
    if raw_label.is_empty() {
        return None;
    }
    
    // Format label - break into multiple SHORT lines
    let label = format_label_text(&raw_label);
    let lines: Vec<&str> = label.lines().collect();
    let num_lines = lines.len();
    
    let label_pos = calculate_label_position(route, offset_index, zoom, transition_type, from_rect, to_rect);
    
    let font_size = 11.0 * zoom;
    let char_width = font_size * 0.55;
    
    // Find longest line for width calculation
    let max_line_len = lines.iter().map(|l| l.len()).max().unwrap_or(0);
    let text_width = max_line_len as f32 * char_width;
    let line_height = font_size * 1.3;
    let text_height = line_height * num_lines as f32;
    let padding = 5.0 * zoom;
    
    let rect = egui::Rect::from_center_size(
        label_pos,
        egui::vec2(text_width + padding * 2.0, text_height + padding),
    );
    
    Some(LabelInfo {
        pos: label_pos,
        rect,
        text: label,
        font_size,
    })
}

/// Check if two rectangles overlap with margin
fn rects_overlap_with_margin(a: &egui::Rect, b: &egui::Rect, margin: f32) -> bool {
    let a_expanded = a.expand(margin);
    a_expanded.intersects(*b)
}

/// Calculate overlap depth between two rectangles
fn overlap_depth(a: &egui::Rect, b: &egui::Rect) -> f32 {
    if !a.intersects(*b) {
        return 0.0;
    }
    
    let x_overlap = (a.right().min(b.right()) - a.left().max(b.left())).max(0.0);
    let y_overlap = (a.bottom().min(b.bottom()) - a.top().max(b.top())).max(0.0);
    
    x_overlap.min(y_overlap)
}

/// Resolve overlapping labels - considers both other labels AND state boxes
fn resolve_label_overlaps(labels: &mut [LabelInfo], state_boxes: &[StateBox]) {
    if labels.is_empty() {
        return;
    }
    
    let max_iterations = 150;
    let margin = 8.0;
    
    for iteration in 0..max_iterations {
        let mut any_collision = false;
        
        for i in 0..labels.len() {
            let mut total_push = egui::Vec2::ZERO;
            let mut push_count = 0;
            
            // Check collision with other labels
            for j in 0..labels.len() {
                if i == j {
                    continue;
                }
                
                if rects_overlap_with_margin(&labels[i].rect, &labels[j].rect, margin) {
                    any_collision = true;
                    let depth = overlap_depth(&labels[i].rect, &labels[j].rect);
                    
                    let center_i = labels[i].rect.center();
                    let center_j = labels[j].rect.center();
                    let diff = center_i - center_j;
                    
                    let push_dir = if diff.length() > 0.1 {
                        diff.normalized()
                    } else {
                        egui::vec2(0.0, if i < j { -1.0 } else { 1.0 })
                    };
                    
                    let push_amount = (depth + margin + 10.0) * 0.5;
                    total_push += push_dir * push_amount;
                    push_count += 1;
                }
            }
            
            // Check collision with state boxes
            for state_box in state_boxes {
                if rects_overlap_with_margin(&labels[i].rect, &state_box.rect, margin) {
                    any_collision = true;
                    let depth = overlap_depth(&labels[i].rect, &state_box.rect);
                    
                    let center_label = labels[i].rect.center();
                    let center_state = state_box.rect.center();
                    let diff = center_label - center_state;
                    
                    let push_dir = if diff.length() > 0.1 {
                        diff.normalized()
                    } else {
                        egui::vec2(1.0, 0.0)
                    };
                    
                    // Push harder away from states
                    let push_amount = (depth + margin + 20.0) * 0.8;
                    total_push += push_dir * push_amount;
                    push_count += 1;
                }
            }
            
            if push_count > 0 {
                let move_vec = total_push / push_count as f32;
                labels[i].pos += move_vec;
                labels[i].rect = labels[i].rect.translate(move_vec);
            }
        }
        
        if !any_collision {
            break;
        }
        
        // Add jitter to escape local minima
        if iteration > 80 && iteration % 10 == 0 {
            for (idx, label) in labels.iter_mut().enumerate() {
                let jitter = egui::vec2(
                    ((iteration + idx * 7) % 13) as f32 - 6.0,
                    ((iteration + idx * 11) % 13) as f32 - 6.0,
                );
                label.pos += jitter;
                label.rect = label.rect.translate(jitter);
            }
        }
    }
}

/// Draw orthogonal arrow with arrowhead
fn draw_orthogonal_arrow(painter: &egui::Painter, route: &[egui::Pos2], zoom: f32) {
    draw_orthogonal_arrow_colored(painter, route, zoom, egui::Color32::from_rgb(160, 175, 195));
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
    
    // Draw arrowhead at the end
    let last = route[route.len() - 1];
    let prev = route[route.len() - 2];
    let dir = (last - prev).normalized();
    
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
    painter.rect_stroke(info.rect, 3.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(70, 80, 95)));
    
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
            egui::Stroke::new(1.0, grid_color),
        );
        x += grid_size;
    }
    
    let mut y = start_y;
    while y < rect.bottom() {
        painter.line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            egui::Stroke::new(1.0, grid_color),
        );
        y += grid_size;
    }
}

/// Estimate the visual size of a state box
fn estimate_state_size(state: &fsm::State) -> egui::Vec2 {
    let mut action_lines = 0;
    let mut max_action_len = 0;
    
    for entry in &state.entry_actions {
        action_lines += 1;
        max_action_len = max_action_len.max(entry.name.len() + 7); // "entry/ "
    }
    for exit in &state.exit_actions {
        action_lines += 1;
        max_action_len = max_action_len.max(exit.name.len() + 6); // "exit/ "
    }
    
    // Add internal transitions
    action_lines += state.internal_transitions.len();
    for internal in &state.internal_transitions {
        let line_len = internal.label().len();
        max_action_len = max_action_len.max(line_len);
    }
    
    let name_len = state.name.len();
    let max_chars = name_len.max(max_action_len);
    
    // Estimate width: chars * approximate char width + padding
    let width = (max_chars as f32 * 8.0).max(100.0) + 30.0;
    
    // Estimate height: header + separator + action lines + padding
    let height = 30.0 + (action_lines.max(1) as f32 * 16.0) + 20.0;
    
    egui::vec2(width, height)
}

/// Calculate hierarchical levels for states based on transitions
fn calculate_state_levels(fsm: &fsm::FsmDefinition) -> std::collections::HashMap<String, i32> {
    let mut levels: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
    
    // Find initial state
    let initial = fsm.initial_state.as_ref();
    
    // BFS to assign levels
    let mut queue: std::collections::VecDeque<(String, i32)> = std::collections::VecDeque::new();
    
    if let Some(init) = initial {
        levels.insert(init.clone(), 0);
        queue.push_back((init.clone(), 0));
    } else if let Some(first_state) = fsm.states.first() {
        levels.insert(first_state.name.clone(), 0);
        queue.push_back((first_state.name.clone(), 0));
    }
    
    // Build adjacency from transitions
    let mut outgoing: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for transition in &fsm.transitions {
        if transition.source != "[*]" && !transition.target.starts_with("<<") && transition.target != "[*]" {
            outgoing.entry(transition.source.clone())
                .or_insert_with(Vec::new)
                .push(transition.target.clone());
        }
    }
    
    // BFS
    while let Some((state, level)) = queue.pop_front() {
        if let Some(targets) = outgoing.get(&state) {
            for target in targets {
                if !levels.contains_key(target) {
                    // Self-loops stay at same level
                    let new_level = if target == &state { level } else { level + 1 };
                    levels.insert(target.clone(), new_level);
                    queue.push_back((target.clone(), new_level));
                }
            }
        }
    }
    
    // Assign remaining states that weren't reached
    let max_level = levels.values().max().copied().unwrap_or(0);
    for state in &fsm.states {
        if !levels.contains_key(&state.name) {
            levels.insert(state.name.clone(), max_level + 1);
        }
    }
    
    levels
}

/// Calculate the bounding rectangle for a state (used for routing and collision)
fn calculate_state_rect(state: &fsm::State, pos: egui::Pos2, zoom: f32) -> egui::Rect {
    let mut action_lines = Vec::new();
    for entry in &state.entry_actions {
        action_lines.push(format!("entry/ {}", entry.name));
    }
    for exit in &state.exit_actions {
        action_lines.push(format!("exit/ {}", exit.name));
    }
    
    let font_size = 10.0 * zoom;
    let char_width = font_size * 0.55;
    let line_height = font_size * 1.3;
    
    // Width based on name or actions, whichever is larger
    let name_width = state.name.len() as f32 * 9.0 * zoom;
    let action_width = action_lines.iter()
        .map(|line| line.len() as f32 * char_width)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    
    let padding = 15.0 * zoom;
    let width = name_width.max(action_width).max(80.0 * zoom) + padding * 2.0;
    
    // Height: header (name) + separator + actions area
    let header_height = 22.0 * zoom;
    let actions_height = if action_lines.is_empty() {
        20.0 * zoom
    } else {
        (action_lines.len() as f32 * line_height) + padding
    };
    let height = header_height + actions_height;
    
    egui::Rect::from_center_size(pos, egui::vec2(width, height))
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
    
    // Dynamic sizing based on content
    let font_size = 10.0 * zoom;
    let char_width = font_size * 0.55;
    let line_height = font_size * 1.3;
    
    // Width based on name or actions
    let name_width = state.name.len() as f32 * 9.0 * zoom;
    let action_width = action_lines.iter()
        .map(|line| line.len() as f32 * char_width)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    
    let padding = 15.0 * zoom;
    let width = name_width.max(action_width).max(80.0 * zoom) + padding * 2.0;
    
    // Height: header + actions
    let header_height = 22.0 * zoom;
    let actions_height = if action_lines.is_empty() {
        20.0 * zoom
    } else {
        (action_lines.len() as f32 * line_height) + padding
    };
    let total_height = header_height + actions_height;
    
    let rect = egui::Rect::from_center_size(pos, egui::vec2(width, total_height));
    
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
        egui::vec2(width, header_height),
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
        let body_center_y = rect.top() + header_height + actions_height / 2.0;
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
