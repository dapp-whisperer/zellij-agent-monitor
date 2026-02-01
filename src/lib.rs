use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
use uuid::Uuid;
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
    pub agent_id: String,        // UUID we generate (used for status file)
    pub pane_id: Option<u32>,    // Set when CommandPaneOpened fires
    pub title: String,
    pub status: AgentStatus,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DebugEntry {
    pub tick: u64,
    pub message: String,
}

#[derive(Default)]
pub struct State {
    pub agents: Vec<Agent>,
    pub selected: usize,
    pub agent_counter: u32,
    pub spinner_frame: usize,
    pub debug_log_path: Option<String>,
    // Fields for debug storage + activity fallback
    pub debug_ring: VecDeque<DebugEntry>,
    pub tick_count: u64,
    pub last_working_tick: HashMap<String, u64>,
}

// Spinner frames for working indicator
const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

// Braille spinner characters for fallback detection in pane titles
const BRAILLE_SPINNERS: &[char] = &[
    '⠿', '⠇', '⠋', '⠙', '⠸', '⠴', '⠦', '⠧', '⠖', '⠏',
    '⠹', '⠼', '⠷', '⠾', '⠽', '⠻', '⠐', '⠑', '⠒', '⠓',
];

// Debug ring buffer size
const DEBUG_RING_SIZE: usize = 100;

// Hysteresis: ~2s at 100ms timer rate (20 ticks)
const HYSTERESIS_TICKS: u64 = 20;

/// Check if a pane title contains a Braille spinner character
fn title_has_spinner(title: &str) -> bool {
    title.chars().any(|c| BRAILLE_SPINNERS.contains(&c))
}

// Status file directory
const STATUS_DIR: &str = "/tmp/agent-monitor";
const STATUS_DIR_FALLBACK: &str = "/private/tmp/agent-monitor";

/// Validate agent_id to prevent command injection and path traversal.
/// Returns true if the agent_id is safe to use in file paths and shell commands.
fn is_valid_agent_id(id: &str) -> bool {
    id.len() <= 16
        && !id.is_empty()
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains("..")
        && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Returns (primary_path, fallback_path) for status file
fn status_file_paths(agent_id: &str) -> (String, String) {
    (
        format!("{}/{}.status", STATUS_DIR, agent_id),
        format!("{}/{}.status", STATUS_DIR_FALLBACK, agent_id),
    )
}

#[derive(Debug)]
enum StatusRead {
    Parsed {
        status: AgentStatus,
        cwd: Option<String>,
    },
    Unrecognized,
    NotFound,
}

/// Reads agent status from file. Returns (status, cwd) if file exists.
/// Format: "W:/path/to/cwd" or legacy "W" (without CWD)
fn read_agent_status(agent_id: &str) -> StatusRead {
    // Validate agent_id to prevent path traversal attacks
    if !is_valid_agent_id(agent_id) {
        return StatusRead::NotFound;
    }

    let (primary_path, fallback_path) = status_file_paths(agent_id);

    match std::fs::read_to_string(&primary_path)
        .or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                std::fs::read_to_string(&fallback_path)
            } else {
                Err(e)
            }
        })
    {
        Ok(content) => parse_status_content(&content),
        Err(_) => StatusRead::NotFound,
    }
}

fn parse_status_content(content: &str) -> StatusRead {
    let content = content.trim();
    // Try new format: "W:/path/to/cwd"
    if let Some((status_char, cwd)) = content.split_once(':') {
        let status = match status_char {
            "W" => AgentStatus::Working,
            "I" => AgentStatus::Idle,
            "?" => AgentStatus::NeedsInput,
            _ => return StatusRead::Unrecognized,
        };
        return StatusRead::Parsed {
            status,
            cwd: Some(cwd.to_string()),
        };
    }
    // Fallback: legacy format without CWD
    let status = match content {
        "W" => AgentStatus::Working,
        "I" => AgentStatus::Idle,
        "?" => AgentStatus::NeedsInput,
        _ => return StatusRead::Unrecognized,
    };
    StatusRead::Parsed { status, cwd: None }
}

/// Deletes the status file for an agent
fn delete_status_file(agent_id: &str) {
    // Validate agent_id to prevent path traversal attacks
    if !is_valid_agent_id(agent_id) {
        return;
    }

    let (primary_path, fallback_path) = status_file_paths(agent_id);
    let _ = std::fs::remove_file(primary_path);
    let _ = std::fs::remove_file(fallback_path);
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
            EventType::RunCommandResult,
            EventType::CommandPaneOpened,
            EventType::CommandPaneExited,
            EventType::PaneClosed,
            EventType::PaneUpdate, // For spinner detection fallback
        ]);

        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::OpenFiles,
            PermissionType::RunCommands,
            PermissionType::OpenTerminalsOrPlugins,
            PermissionType::FullHdAccess,
        ]);

        self.append_debug_log("load: plugin initialized");
        set_timeout(1.0);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::PermissionRequestResult(result) => self.handle_permission_result(result),
            Event::CommandPaneOpened(pane_id, context) => self.handle_pane_opened(pane_id, context),
            Event::CommandPaneExited(pane_id, exit_code, context) => {
                self.handle_pane_exited(pane_id, exit_code, context)
            }
            Event::RunCommandResult(exit_code, stdout, stderr, context) => {
                self.handle_command_result(exit_code, stdout, stderr, context)
            }
            Event::PaneClosed(pane_id) => self.handle_pane_closed(pane_id),
            Event::Timer(elapsed) => self.handle_timer(elapsed),
            Event::PaneUpdate(manifest) => self.handle_pane_update(manifest),
            Event::Key(key) => self.handle_key(key),
            _ => false,
        }
    }

    fn render(&mut self, _rows: usize, cols: usize) {
        let header = format!("- Agents ({}) ", self.state.agents.len());
        let padding = "-".repeat(cols.saturating_sub(header.len()));
        println!("{}{}", header, padding);

        if self.state.agents.is_empty() {
            println!("  (no agents)");
            println!();
            println!("  Press 'n' to spawn a new agent");
        } else {
            for (i, agent) in self.state.agents.iter().enumerate() {
                let marker = if i == self.state.selected { ">" } else { " " };
                let (status_str, _color_idx) = match agent.status {
                    AgentStatus::Idle => ("[.]".to_string(), 0),         // cyan
                    AgentStatus::Working => {
                        let spinner = SPINNER[self.state.spinner_frame];
                        (format!("[{}]", spinner), 2)                    // yellow
                    }
                    AgentStatus::NeedsInput => ("[?]".to_string(), 3),   // orange
                    AgentStatus::Completed => ("[v]".to_string(), 1),    // green
                    AgentStatus::Failed => ("[X]".to_string(), 3),       // red
                };
                // Format CWD as last 2 path components
                let cwd_display = agent.cwd.as_ref()
                    .map(|p| {
                        let parts: Vec<&str> = p.rsplit('/').filter(|s| !s.is_empty()).take(2).collect();
                        format!(" ({})", parts.into_iter().rev().collect::<Vec<_>>().join("/"))
                    })
                    .unwrap_or_default();
                // Truncate title if needed, accounting for CWD display
                let max_title_len = cols.saturating_sub(10 + cwd_display.len());
                let title: String = agent.title.chars().take(max_title_len).collect();
                println!("{} {} {}{}", marker, status_str, title, cwd_display);
            }
        }

        // Debug footer: show last 5 entries from ring buffer
        println!();
        let debug_header = format!("- Debug (tick {}) ", self.state.tick_count);
        let debug_padding = "-".repeat(cols.saturating_sub(debug_header.len()));
        println!("{}{}", debug_header, debug_padding);

        // Show last 5 debug entries (oldest to newest)
        let entries: Vec<_> = self.state.debug_ring.iter().rev().take(5).collect();
        for entry in entries.into_iter().rev() {
            let max_len = cols.saturating_sub(10); // [tick] prefix
            let truncated: String = entry.message.chars().take(max_len).collect();
            println!("  [{}] {}", entry.tick, truncated);
        }
        if self.state.debug_ring.is_empty() {
            println!("  (no debug messages)");
        }

        let footer = "- n:new  enter:focus  x:kill  q:hide ";
        let footer_padding = "-".repeat(cols.saturating_sub(footer.len()));
        println!("{}{}", footer, footer_padding);
    }
}

// =============================================================================
// Event Handlers
// =============================================================================

impl AgentMonitorPlugin {
    /// Handle permission request result event
    fn handle_permission_result(&mut self, _result: PermissionStatus) -> bool {
        self.append_debug_log("permission: result");
        true
    }

    /// Handle command pane opened event
    fn handle_pane_opened(&mut self, pane_id: u32, context: BTreeMap<String, String>) -> bool {
        // Only track panes we spawned (have our marker in context)
        if context.get("agent_monitor").is_some() {
            // Find the pre-registered agent by agent_id and set its pane_id
            if let Some(agent_id) = context.get("agent_id") {
                if let Some(agent) = self.state.agents.iter_mut().find(|a| &a.agent_id == agent_id) {
                    agent.pane_id = Some(pane_id);
                }
            }
        }
        true
    }

    /// Handle command pane exited event
    fn handle_pane_exited(
        &mut self,
        pane_id: u32,
        exit_code: Option<i32>,
        _context: BTreeMap<String, String>,
    ) -> bool {
        if let Some(agent) = self
            .state
            .agents
            .iter_mut()
            .find(|a| a.pane_id == Some(pane_id))
        {
            agent.status = match exit_code {
                Some(0) => AgentStatus::Completed,
                _ => AgentStatus::Failed,
            };
        }
        true
    }

    /// Handle run command result event
    fn handle_command_result(
        &mut self,
        _exit_code: Option<i32>,
        stdout: Vec<u8>,
        _stderr: Vec<u8>,
        context: BTreeMap<String, String>,
    ) -> bool {
        if context.get("kind").map(|s| s.as_str()) == Some("status_read") {
            if let Some(agent_id) = context.get("agent_id") {
                let output = String::from_utf8_lossy(&stdout);
                let parsed = parse_status_content(&output);
                if let StatusRead::Parsed {
                    status: new_status,
                    cwd: new_cwd,
                } = parsed
                {
                    // Use update_agent_status for hysteresis support
                    self.update_agent_status(agent_id, new_status, new_cwd);
                }
            }
        }
        true
    }

    /// Handle pane closed event
    fn handle_pane_closed(&mut self, pane_id: PaneId) -> bool {
        if let PaneId::Terminal(id) = pane_id {
            let was_selected = self
                .state
                .agents
                .get(self.state.selected)
                .map(|a| a.pane_id == Some(id))
                .unwrap_or(false);

            // Find agent_id for cleanup before removing
            let agent_id_to_cleanup = self.state.agents.iter()
                .find(|a| a.pane_id == Some(id))
                .map(|a| a.agent_id.clone());

            self.state.agents.retain(|a| a.pane_id != Some(id));

            // Clean up status file if it was our agent
            if let Some(agent_id) = agent_id_to_cleanup {
                delete_status_file(&agent_id);
            }

            // Adjust selection if needed
            if was_selected && !self.state.agents.is_empty() {
                self.state.selected = self.state.selected.min(self.state.agents.len() - 1);
            }
        }
        true
    }

    /// Handle timer event
    fn handle_timer(&mut self, _elapsed: f64) -> bool {
        // Increment tick counter for hysteresis timing
        self.state.tick_count += 1;

        // Advance spinner
        self.state.spinner_frame = (self.state.spinner_frame + 1) % SPINNER.len();

        if self.state.spinner_frame == 0 {
            self.push_debug("timer: tick");
        }

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

    /// Handle pane update event (spinner detection fallback)
    fn handle_pane_update(&mut self, manifest: PaneManifest) -> bool {
        // Spinner detection fallback: detect activity via pane title spinners
        // Collect agents to update to avoid borrow conflicts
        let mut spinner_detected: Vec<String> = Vec::new();

        for (_tab_idx, panes) in &manifest.panes {
            for pane in panes {
                // Find matching agent by pane_id
                if let Some(agent) = self.state.agents.iter()
                    .find(|a| a.pane_id == Some(pane.id))
                {
                    let spinner_active = title_has_spinner(&pane.title);

                    // Only use spinner as fallback when hook says Idle
                    if agent.status == AgentStatus::Idle && spinner_active {
                        spinner_detected.push(agent.agent_id.clone());
                    }
                }
            }
        }

        // Apply spinner-based Working status
        for agent_id in spinner_detected {
            if let Some(agent) = self.state.agents.iter_mut()
                .find(|a| a.agent_id == agent_id)
            {
                agent.status = AgentStatus::Working;
                self.state.last_working_tick.insert(
                    agent_id.clone(),
                    self.state.tick_count
                );
            }
            self.push_debug(&format!("Fallback: spinner detected for {}", agent_id));
        }
        true
    }

    /// Handle key press event
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
                    if let Some(pane_id) = agent.pane_id {
                        show_pane_with_id(PaneId::Terminal(pane_id), true);
                    }
                }
                true
            }

            BareKey::Char('n') => {
                self.spawn_agent();
                true
            }

            BareKey::Char('x') => {
                if let Some(agent) = self.state.agents.get(self.state.selected) {
                    if let Some(pane_id) = agent.pane_id {
                        close_terminal_pane(pane_id);
                    }
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
}

// =============================================================================
// Debug Logging + Ring Buffer
// =============================================================================

impl AgentMonitorPlugin {
    /// Push a debug message to both the ring buffer and host filesystem
    fn push_debug(&mut self, msg: &str) {
        // Add to ring buffer with FIFO eviction
        if self.state.debug_ring.len() >= DEBUG_RING_SIZE {
            self.state.debug_ring.pop_front();
        }
        self.state.debug_ring.push_back(DebugEntry {
            tick: self.state.tick_count,
            message: msg.to_string(),
        });

        // Also write to host filesystem (bypasses WASI)
        self.write_debug_to_host(msg);
    }

    /// Write debug line to host filesystem via run_command (bypasses WASI restrictions)
    fn write_debug_to_host(&self, line: &str) {
        let log_path = "/private/tmp/agent-monitor/plugin-logs/plugin-debug.log";
        let log_dir = "/private/tmp/agent-monitor/plugin-logs";
        let formatted = format!("[{}] {}", self.state.tick_count, line);

        let mut env = BTreeMap::new();
        env.insert("LOG_DIR".to_string(), log_dir.to_string());
        env.insert("LOG_PATH".to_string(), log_path.to_string());
        env.insert("LOG_LINE".to_string(), formatted);

        run_command_with_env_variables_and_cwd(
            &[
                "/bin/sh",
                "-lc",
                "mkdir -p \"$LOG_DIR\" && printf '%s\\n' \"$LOG_LINE\" >> \"$LOG_PATH\"",
            ],
            env,
            PathBuf::from("."),
            BTreeMap::new(),
        );
    }

    /// Legacy method - now delegates to push_debug
    fn append_debug_log(&mut self, line: &str) {
        self.push_debug(line);
    }

    /// Update agent status with hysteresis for Working->Idle transitions
    /// NeedsInput transitions are immediate (no delay)
    fn update_agent_status(&mut self, agent_id: &str, new_status: AgentStatus, new_cwd: Option<String>) {
        if let Some(agent) = self.state.agents.iter_mut().find(|a| a.agent_id == agent_id) {
            let old_status = agent.status.clone();

            // Track when we were last Working
            if new_status == AgentStatus::Working {
                self.state.last_working_tick.insert(agent_id.to_string(), self.state.tick_count);
            }

            // Apply hysteresis only for Working -> Idle transitions
            if old_status == AgentStatus::Working && new_status == AgentStatus::Idle {
                if let Some(last_tick) = self.state.last_working_tick.get(agent_id) {
                    let elapsed = self.state.tick_count.saturating_sub(*last_tick);
                    if elapsed < HYSTERESIS_TICKS {
                        // Keep Working status, don't transition yet
                        // But still update CWD if provided
                        if new_cwd.is_some() {
                            agent.cwd = new_cwd;
                        }
                        return; // Don't change status yet
                    }
                }
            }

            // NeedsInput and other transitions are immediate
            let status_changed = agent.status != new_status;
            agent.status = new_status.clone();
            if new_cwd.is_some() {
                agent.cwd = new_cwd;
            }

            if status_changed {
                self.push_debug(&format!(
                    "Status: {} {:?} -> {:?}",
                    agent_id, old_status, new_status
                ));
            }
        }
    }

    /// Polls status files for all non-terminal agents and updates their status
    fn poll_status_files(&mut self) {
        // Collect updates first to avoid borrow conflicts with update_agent_status
        let mut status_updates: Vec<(String, AgentStatus, Option<String>)> = Vec::new();
        let mut pending_command_reads = Vec::new();

        for agent in &self.state.agents {
            // Don't poll terminal states
            if matches!(agent.status, AgentStatus::Completed | AgentStatus::Failed) {
                continue;
            }

            let read_result = read_agent_status(&agent.agent_id);

            match read_result {
                StatusRead::Parsed { status, cwd } => {
                    status_updates.push((agent.agent_id.clone(), status, cwd));
                }
                StatusRead::NotFound => {
                    pending_command_reads.push(agent.agent_id.clone());
                }
                StatusRead::Unrecognized => {
                    // Ignore unrecognized content
                }
            }
        }

        // Apply status updates with hysteresis
        for (agent_id, new_status, new_cwd) in status_updates {
            self.update_agent_status(&agent_id, new_status, new_cwd);
        }

        // Request status via command for agents without files
        for agent_id in pending_command_reads {
            self.request_status_via_command(&agent_id);
        }
    }

    fn request_status_via_command(&self, agent_id: &str) {
        // Validate agent_id to prevent command injection
        if !is_valid_agent_id(agent_id) {
            return;
        }

        let (primary_path, fallback_path) = status_file_paths(agent_id);
        let mut context = BTreeMap::new();
        context.insert("kind".to_string(), "status_read".to_string());
        context.insert("agent_id".to_string(), agent_id.to_string());
        let command = format!(
            "cat {} 2>/dev/null || cat {} 2>/dev/null",
            fallback_path, primary_path
        );
        run_command(
            &[
                "/bin/sh",
                "-lc",
                &command,
            ],
            context,
        );
    }

    fn spawn_agent(&mut self) {
        self.state.agent_counter += 1;
        let agent_id = Uuid::new_v4().to_string()[..8].to_string(); // Short UUID
        let title = format!("Agent {}", self.state.agent_counter);

        let mut context = BTreeMap::new();
        context.insert("agent_monitor".to_string(), "true".to_string());
        context.insert("agent_id".to_string(), agent_id.clone());
        context.insert("title".to_string(), title.clone());

        // Pass agent_id as env var via env command
        open_command_pane_floating(
            CommandToRun {
                path: PathBuf::from("env"),
                args: vec![
                    format!("AGENT_MONITOR_ID={}", agent_id),
                    "claude".to_string(),
                ],
                cwd: None,
            },
            None,
            context,
        );

        // Pre-register the agent (pane_id filled in on CommandPaneOpened)
        self.state.agents.push(Agent {
            agent_id,
            pane_id: None,
            title,
            status: AgentStatus::Idle,
            cwd: None,
        });
    }
}
