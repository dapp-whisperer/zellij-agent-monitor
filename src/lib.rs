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
    Running,   // Active (includes starting state)
    Completed, // Exited with code 0
    Failed,    // Exited with non-zero code
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
                        status: AgentStatus::Running,
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

                    self.state.agents.retain(|a| a.pane_id != id);

                    // Adjust selection if needed
                    if was_selected && !self.state.agents.is_empty() {
                        self.state.selected = self.state.selected.min(self.state.agents.len() - 1);
                    }
                }
                true
            }

            Event::Timer(_) => {
                // Keep timer running for future use (e.g., when scrollback API available)
                set_timeout(5.0);
                false
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
                let status = match agent.status {
                    AgentStatus::Running => "[R]",
                    AgentStatus::Completed => "[✓]",
                    AgentStatus::Failed => "[X]",
                };
                // Truncate title if needed
                let max_title_len = cols.saturating_sub(10);
                let title: String = agent.title.chars().take(max_title_len).collect();
                println!("{} {} {}", marker, status, title);
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
