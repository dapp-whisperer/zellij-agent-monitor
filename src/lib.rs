use std::collections::BTreeMap;
use std::path::PathBuf;
use zellij_tile::prelude::*;

// WASI entry point required by Zellij 0.43.1 when using wasm32-wasip1 target
#[no_mangle]
pub extern "C" fn _start() {
    // Empty - Zellij manages plugin lifecycle via load/update/render exports
}

// =============================================================================
// Data Structures
// =============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Idle,        // Waiting for user input
    Working,     // Tool in progress
    NeedsInput,  // Waiting for approval
    Completed,   // Exited with code 0
    Failed,      // Exited with non-zero code
}

impl Default for AgentStatus {
    fn default() -> Self {
        AgentStatus::Idle
    }
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub pane_id: u32,
    pub title: String,
    pub status: AgentStatus,
}

#[derive(Default)]
pub struct State {
    pub agents: Vec<Agent>,
    pub selected: usize,
    pub agent_counter: u32,
    pub spinner_frame: usize,
}

// Spinner frames for working indicator
const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

// Status file directory
const STATUS_DIR: &str = "/tmp/agent-monitor";

/// Reads agent status from file. Returns None if file doesn't exist or is invalid.
fn read_agent_status(pane_id: u32) -> Option<AgentStatus> {
    let path = format!("{}/{}.status", STATUS_DIR, pane_id);

    match std::fs::read_to_string(&path) {
        Ok(content) => match content.trim() {
            "W" => Some(AgentStatus::Working),
            "I" => Some(AgentStatus::Idle),
            "?" => Some(AgentStatus::NeedsInput),
            _ => None, // Unknown, keep current state
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            None // File doesn't exist yet, keep current state
        }
        Err(_) => None, // Read error, keep current state
    }
}

/// Deletes the status file for an agent
fn delete_status_file(pane_id: u32) {
    let path = format!("{}/{}.status", STATUS_DIR, pane_id);
    let _ = std::fs::remove_file(path);
}

// =============================================================================
// Plugin
// =============================================================================

#[derive(Default)]
struct AgentMonitorPlugin {
    state: State,
}

register_plugin!(AgentMonitorPlugin);

impl ZellijPlugin for AgentMonitorPlugin {
    fn load(&mut self, _config: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Key,
            EventType::Timer,
            EventType::CommandPaneOpened,
            EventType::CommandPaneExited,
            EventType::PaneClosed,
        ]);

        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::RunCommands,
            PermissionType::OpenTerminalsOrPlugins,
        ]);

        set_timeout(1.0);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(_) => true,

            Event::CommandPaneOpened(pane_id, context) => {
                // Only track panes we spawned (have our marker in context)
                if context.get("agent_monitor").is_some() {
                    let title = context
                        .get("title")
                        .cloned()
                        .unwrap_or_else(|| format!("Agent {}", pane_id));
                    self.state.agents.push(Agent {
                        pane_id,
                        title,
                        status: AgentStatus::Idle, // Start as Idle - waiting for user input
                    });
                }
                true
            }

            Event::CommandPaneExited(pane_id, exit_code, _context) => {
                if let Some(agent) = self
                    .state
                    .agents
                    .iter_mut()
                    .find(|a| a.pane_id == pane_id)
                {
                    agent.status = match exit_code {
                        Some(0) => AgentStatus::Completed,
                        _ => AgentStatus::Failed,
                    };
                }
                true
            }

            Event::PaneClosed(pane_id) => {
                if let PaneId::Terminal(id) = pane_id {
                    let was_selected = self
                        .state
                        .agents
                        .get(self.state.selected)
                        .map(|a| a.pane_id == id)
                        .unwrap_or(false);

                    // Check if this was one of our agents before removing
                    let was_our_agent = self.state.agents.iter().any(|a| a.pane_id == id);

                    self.state.agents.retain(|a| a.pane_id != id);

                    // Clean up status file if it was our agent
                    if was_our_agent {
                        delete_status_file(id);
                    }

                    // Adjust selection if needed
                    if was_selected && !self.state.agents.is_empty() {
                        self.state.selected = self.state.selected.min(self.state.agents.len() - 1);
                    }
                }
                true
            }

            Event::Timer(_) => {
                // Advance spinner
                self.state.spinner_frame = (self.state.spinner_frame + 1) % SPINNER.len();

                // Poll status files for all active agents
                self.poll_status_files();

                // Adaptive polling: fast when working, slow when all idle
                let has_working = self
                    .state
                    .agents
                    .iter()
                    .any(|a| a.status == AgentStatus::Working);

                set_timeout(if has_working { 0.1 } else { 0.5 });

                // Re-render if there's a working agent (spinner) or needs input
                has_working
                    || self
                        .state
                        .agents
                        .iter()
                        .any(|a| a.status == AgentStatus::NeedsInput)
            }

            Event::Key(key) => self.handle_key(key),

            _ => false,
        }
    }

    fn render(&mut self, _rows: usize, cols: usize) {
        let header = format!("─ Agents ({}) ", self.state.agents.len());
        let padding = "─".repeat(cols.saturating_sub(header.len()));
        println!("{}{}", header, padding);

        if self.state.agents.is_empty() {
            println!("  (no agents)");
            println!();
            println!("  Press 'n' to spawn a new agent");
        } else {
            for (i, agent) in self.state.agents.iter().enumerate() {
                let marker = if i == self.state.selected { "▶" } else { " " };
                let (status_str, _color_idx) = match agent.status {
                    AgentStatus::Idle => ("[·]".to_string(), 0),         // cyan
                    AgentStatus::Working => {
                        let spinner = SPINNER[self.state.spinner_frame];
                        (format!("[{}]", spinner), 2)                    // yellow
                    }
                    AgentStatus::NeedsInput => ("[?]".to_string(), 3),   // orange
                    AgentStatus::Completed => ("[✓]".to_string(), 1),    // green
                    AgentStatus::Failed => ("[X]".to_string(), 3),       // red
                };
                // Truncate title if needed
                let max_title_len = cols.saturating_sub(10);
                let title: String = agent.title.chars().take(max_title_len).collect();
                println!("{} {} {}", marker, status_str, title);
            }
        }

        println!();
        let footer = "─ n:new  ↵:focus  x:kill  q:hide ";
        let footer_padding = "─".repeat(cols.saturating_sub(footer.len()));
        println!("{}{}", footer, footer_padding);
    }
}

// =============================================================================
// Key Handling
// =============================================================================

impl AgentMonitorPlugin {
    /// Polls status files for all non-terminal agents and updates their status
    fn poll_status_files(&mut self) {
        for agent in &mut self.state.agents {
            // Don't poll terminal states
            if matches!(agent.status, AgentStatus::Completed | AgentStatus::Failed) {
                continue;
            }

            if let Some(new_status) = read_agent_status(agent.pane_id) {
                agent.status = new_status;
            }
        }
    }

    fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        match key.bare_key {
            BareKey::Char('j') | BareKey::Down => {
                if !self.state.agents.is_empty() {
                    self.state.selected = (self.state.selected + 1) % self.state.agents.len();
                }
                true
            }

            BareKey::Char('k') | BareKey::Up => {
                if !self.state.agents.is_empty() {
                    self.state.selected = self.state.selected.saturating_sub(1);
                }
                true
            }

            BareKey::Enter => {
                if let Some(agent) = self.state.agents.get(self.state.selected) {
                    show_pane_with_id(PaneId::Terminal(agent.pane_id), true);
                }
                true
            }

            BareKey::Char('n') => {
                self.spawn_agent();
                true
            }

            BareKey::Char('x') => {
                if let Some(agent) = self.state.agents.get(self.state.selected) {
                    close_terminal_pane(agent.pane_id);
                }
                true
            }

            BareKey::Char('q') | BareKey::Esc => {
                hide_self();
                true
            }

            _ => false,
        }
    }

    fn spawn_agent(&mut self) {
        self.state.agent_counter += 1;
        let title = format!("Agent {}", self.state.agent_counter);

        let mut context = BTreeMap::new();
        context.insert("agent_monitor".to_string(), "true".to_string());
        context.insert("title".to_string(), title.clone());

        open_command_pane_floating(
            CommandToRun {
                path: PathBuf::from("claude"),
                args: vec![],
                cwd: None, // Inherits from current pane
            },
            None, // Default floating position
            context,
        );
    }
}
