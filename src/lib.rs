use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::Read;
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentStatus {
    Idle,        // Waiting for user input
    Working,     // Tool in progress
    NeedsInput,  // Waiting for approval
    Unread,      // Completed work, not yet viewed
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
    pub last_event: Option<String>,  // Last hook event that triggered status change
    pub created_tick: u64,       // For orphan cleanup timeout
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
    // Fields for debug storage + activity fallback
    pub debug_ring: VecDeque<DebugEntry>,
    pub tick_count: u64,
    pub last_working_tick: HashMap<String, u64>,
    // Focus tracking for Unread -> Idle transitions
    pub floating_panes_visible: bool,
    pub focused_pane_id: Option<u32>,
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

// Orphan cleanup: 10s at 100ms timer rate
const ORPHAN_TIMEOUT_TICKS: u64 = 100;

// Magic number constants
const MAX_AGENT_ID_LEN: usize = 16;
const INITIAL_TIMEOUT_SECS: f64 = 1.0;
const FAST_POLL_SECS: f64 = 0.1;
const SLOW_POLL_SECS: f64 = 0.5;
const DEBUG_ENTRIES_DISPLAY: usize = 5;
const RENDER_PADDING_OFFSET: usize = 10;

// Maximum status file size to read (4KB should be plenty for status content)
// This prevents DoS from oversized files in world-writable temp directories
const MAX_STATUS_FILE_SIZE: u64 = 4096;

/// Check if a pane title contains a Braille spinner character
fn title_has_spinner(title: &str) -> bool {
    title.chars().any(|c| BRAILLE_SPINNERS.contains(&c))
}

/// Returns the status directory path using a secure fallback chain.
/// Prefers user-specific directories to mitigate /tmp symlink attacks.
fn status_dir() -> PathBuf {
    // 1. Try XDG_RUNTIME_DIR (per-user, auto-cleaned on logout)
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime).join("agent-monitor");
    }

    // 2. Try TMPDIR (user-specific on macOS: /var/folders/...)
    if let Ok(tmpdir) = std::env::var("TMPDIR") {
        return PathBuf::from(tmpdir).join("agent-monitor");
    }

    // 3. Fall back to user-specific /tmp directory
    if let Ok(user) = std::env::var("USER") {
        return PathBuf::from(format!("/tmp/agent-monitor-{}", user));
    }

    // 4. Last resort: shared /tmp (least secure)
    PathBuf::from("/tmp/agent-monitor")
}

/// Ensures the status directory exists with secure permissions (0700).
/// Returns true if directory exists or was created successfully.
fn ensure_status_dir() -> bool {
    let dir = status_dir();

    // Check if directory already exists
    if dir.is_dir() {
        return true;
    }

    // Create directory with restrictive permissions
    // Note: WASI may have limitations on permission setting
    match std::fs::create_dir_all(&dir) {
        Ok(()) => {
            // Try to set permissions to 0700 (owner read/write/execute only)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
            }
            true
        }
        Err(_) => false,
    }
}

/// Validate agent_id to prevent command injection and path traversal.
/// Returns true if the agent_id is safe to use in file paths and shell commands.
fn is_valid_agent_id(id: &str) -> bool {
    id.len() <= MAX_AGENT_ID_LEN
        && !id.is_empty()
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains("..")
        && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Returns the status file path for an agent
fn status_file_path(agent_id: &str) -> PathBuf {
    status_dir().join(format!("{}.status", agent_id))
}

#[derive(Debug)]
enum StatusRead {
    Parsed {
        status: AgentStatus,
        event_name: Option<String>,
        cwd: Option<String>,
    },
    Unrecognized,
    NotFound,
}

/// Reads agent status from file with bounded size to prevent DoS.
/// Format: "W:/path/to/cwd" or legacy "W" (without CWD)
/// Returns NotFound if file doesn't exist or is too large.
fn read_agent_status(agent_id: &str) -> StatusRead {
    // Validate agent_id to prevent path traversal attacks
    if !is_valid_agent_id(agent_id) {
        return StatusRead::NotFound;
    }

    let status_path = status_file_path(agent_id);

    // Open file and check size before reading to prevent DoS
    let file = match std::fs::File::open(&status_path) {
        Ok(f) => f,
        Err(_) => return StatusRead::NotFound,
    };

    // Check file size via metadata - skip oversized files
    if let Ok(metadata) = file.metadata() {
        if metadata.len() > MAX_STATUS_FILE_SIZE {
            // File too large - treat as unrecognized to prevent memory exhaustion
            return StatusRead::Unrecognized;
        }
    }

    // Read up to MAX_STATUS_FILE_SIZE bytes
    let mut buffer = Vec::with_capacity(MAX_STATUS_FILE_SIZE as usize);
    let mut handle = file.take(MAX_STATUS_FILE_SIZE);

    match handle.read_to_end(&mut buffer) {
        Ok(_) => {
            let content = String::from_utf8_lossy(&buffer);
            parse_status_content(&content)
        }
        Err(_) => StatusRead::NotFound,
    }
}

// Maximum lengths for parsed fields to prevent DoS from malicious input
const MAX_EVENT_NAME_LEN: usize = 64;
const MAX_CWD_LEN: usize = 4096;

/// Sanitize a status field by stripping control characters and ANSI escapes.
/// This prevents UI spoofing from malicious status files.
fn sanitize_status_field(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == ' ')
        .collect()
}

fn parse_status_content(content: &str) -> StatusRead {
    let content = content.trim();

    // Parse status character from first part (common to all formats)
    let mut parts = content.splitn(3, ':');
    let status = match parts.next() {
        Some("W") => AgentStatus::Working,
        Some("I") => AgentStatus::Idle,
        Some("?") => AgentStatus::NeedsInput,
        Some("U") => AgentStatus::Unread,
        _ => return StatusRead::Unrecognized,
    };

    // Helper to validate, sanitize, and convert event name
    let to_event = |s: &str| {
        let sanitized = sanitize_status_field(s);
        if sanitized.len() <= MAX_EVENT_NAME_LEN {
            Some(sanitized)
        } else {
            None // Reject overly long event names
        }
    };

    // Helper to validate, sanitize, and convert cwd path
    let to_cwd = |s: &str| {
        let sanitized = sanitize_status_field(s);
        if sanitized.len() <= MAX_CWD_LEN {
            Some(sanitized)
        } else {
            None // Reject overly long paths
        }
    };

    match (parts.next(), parts.next()) {
        // Legacy format: "W"
        (None, _) => StatusRead::Parsed {
            status,
            event_name: None,
            cwd: None,
        },
        // Two parts: "W:/path" or "W:EventName"
        (Some(second), None) => {
            if second.starts_with('/') {
                // Current format: path
                StatusRead::Parsed {
                    status,
                    event_name: None,
                    cwd: to_cwd(second),
                }
            } else {
                // Event name without path
                StatusRead::Parsed {
                    status,
                    event_name: to_event(second),
                    cwd: None,
                }
            }
        }
        // Three parts: "W:EventName:/path" or "W:/path:with:colons"
        (Some(second), Some(third)) => {
            if third.starts_with('/') {
                // New format: event name + absolute path
                StatusRead::Parsed {
                    status,
                    event_name: to_event(second),
                    cwd: to_cwd(third),
                }
            } else if second.starts_with('/') {
                // Path with colons: rejoin as path
                let full_path = format!("{}:{}", second, third);
                StatusRead::Parsed {
                    status,
                    event_name: None,
                    cwd: to_cwd(&full_path),
                }
            } else {
                // Event name with non-absolute path
                StatusRead::Parsed {
                    status,
                    event_name: to_event(second),
                    cwd: to_cwd(third),
                }
            }
        }
    }
}

/// Deletes the status file for an agent
fn delete_status_file(agent_id: &str) {
    // Validate agent_id to prevent path traversal attacks
    if !is_valid_agent_id(agent_id) {
        return;
    }

    let status_path = status_file_path(agent_id);
    let _ = std::fs::remove_file(status_path);
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
            EventType::PaneUpdate,  // For spinner detection fallback + focus tracking
            EventType::TabUpdate,   // For tracking floating pane visibility
        ]);

        // Request minimum permissions required for plugin functionality:
        // - ReadApplicationState: Required for PaneUpdate/TabUpdate events (focus tracking, spinner detection)
        // - ChangeApplicationState: Required for show_pane_with_id, hide_self, close_terminal_pane
        // - RunCommands: Required for run_command_with_env_variables_and_cwd (status file writes, fallback reads)
        // - OpenTerminalsOrPlugins: Required for open_command_pane_floating (spawning agent panes)
        // Note: OpenFiles not needed - std::fs works without it in WASI plugins
        // Note: FullHdAccess removed - was not required for any plugin functionality
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::RunCommands,
            PermissionType::OpenTerminalsOrPlugins,
        ]);

        self.push_debug("load: plugin initialized");
        set_timeout(INITIAL_TIMEOUT_SECS);
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
            Event::TabUpdate(tabs) => self.handle_tab_update(tabs),
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
                let status_str = match agent.status {
                    AgentStatus::Idle => "[.]".to_string(),
                    AgentStatus::Working => {
                        let spinner = SPINNER[self.state.spinner_frame];
                        format!("[{}]", spinner)
                    }
                    AgentStatus::NeedsInput => "[?]".to_string(),
                    AgentStatus::Unread => "[*]".to_string(),
                    AgentStatus::Completed => "[v]".to_string(),
                    AgentStatus::Failed => "[X]".to_string(),
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

        // Show last N debug entries (oldest to newest)
        let entries: Vec<_> = self.state.debug_ring.iter().rev().take(DEBUG_ENTRIES_DISPLAY).collect();
        for entry in entries.into_iter().rev() {
            let max_len = cols.saturating_sub(RENDER_PADDING_OFFSET); // [tick] prefix
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
        self.push_debug("permission: result");
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
                // Skip oversized output as a safety check (should be bounded by head -c)
                if stdout.len() > MAX_STATUS_FILE_SIZE as usize {
                    return true;
                }
                let output = String::from_utf8_lossy(&stdout);
                let parsed = parse_status_content(&output);
                if let StatusRead::Parsed {
                    status: new_status,
                    event_name: new_event,
                    cwd: new_cwd,
                } = parsed
                {
                    // Use update_agent_status for hysteresis support
                    self.update_agent_status(agent_id, new_status, new_event, new_cwd);
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

            // Clean up status file and HashMap entry if it was our agent
            if let Some(ref agent_id) = agent_id_to_cleanup {
                self.state.last_working_tick.remove(agent_id);
                delete_status_file(agent_id);
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

        // Single-pass consolidation: collect orphan indices, has_working, needs_input
        // This reduces O(4n) to O(2n) per tick (one scan here + one in poll_status_files)
        let tick = self.state.tick_count;
        let mut has_working = false;
        let mut needs_input = false;
        let mut orphan_indices: Vec<usize> = Vec::new();

        for (i, agent) in self.state.agents.iter().enumerate() {
            // Check orphan condition: no pane_id after timeout
            if agent.pane_id.is_none()
                && tick.saturating_sub(agent.created_tick) > ORPHAN_TIMEOUT_TICKS
            {
                orphan_indices.push(i);
            }

            // Check status flags for adaptive polling and re-render decision
            match agent.status {
                AgentStatus::Working => has_working = true,
                AgentStatus::NeedsInput => needs_input = true,
                _ => {}
            }
        }

        // Remove orphans in reverse order to preserve indices
        for i in orphan_indices.into_iter().rev() {
            let agent = self.state.agents.remove(i);
            self.state.last_working_tick.remove(&agent.agent_id);
            delete_status_file(&agent.agent_id);
        }

        // Poll status files for all active agents (separate pass with file I/O)
        self.poll_status_files();

        // Adaptive polling: fast when working, slow when all idle
        set_timeout(if has_working { FAST_POLL_SECS } else { SLOW_POLL_SECS });

        // Re-render if there's a working agent (spinner) or needs input
        has_working || needs_input
    }

    /// Handle pane update event (spinner detection fallback + focus tracking)
    fn handle_pane_update(&mut self, manifest: PaneManifest) -> bool {
        // Spinner detection fallback: detect activity via pane title spinners
        // Collect agents to update to avoid borrow conflicts
        let mut spinner_detected: Vec<String> = Vec::new();

        // Track focused pane for Unread -> Idle transitions
        for (_tab_idx, panes) in &manifest.panes {
            for pane in panes {
                // Track which pane is focused
                if pane.is_focused {
                    let old_focused = self.state.focused_pane_id;
                    self.state.focused_pane_id = Some(pane.id);

                    // If focus changed, check for Unread transitions
                    if old_focused != Some(pane.id) {
                        self.check_unread_transitions();
                    }
                }

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

    /// Handle tab update event (track floating pane visibility)
    fn handle_tab_update(&mut self, tabs: Vec<TabInfo>) -> bool {
        // Find active tab and track floating pane visibility
        for tab in &tabs {
            if tab.active {
                let was_visible = self.state.floating_panes_visible;
                self.state.floating_panes_visible = tab.are_floating_panes_visible;

                // If floating panes just became visible, check for Unread transitions
                if !was_visible && tab.are_floating_panes_visible {
                    self.check_unread_transitions();
                }
                break;
            }
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
    /// Push a debug message to the in-memory ring buffer (visible in UI footer)
    fn push_debug(&mut self, msg: &str) {
        // Add to ring buffer with FIFO eviction
        if self.state.debug_ring.len() >= DEBUG_RING_SIZE {
            self.state.debug_ring.pop_front();
        }
        self.state.debug_ring.push_back(DebugEntry {
            tick: self.state.tick_count,
            message: msg.to_string(),
        });
    }

    /// Update agent status with hysteresis for Working->Idle transitions
    /// NeedsInput transitions are immediate (no delay)
    fn update_agent_status(&mut self, agent_id: &str, new_status: AgentStatus, new_event: Option<String>, new_cwd: Option<String>) {
        if let Some(agent) = self.state.agents.iter_mut().find(|a| a.agent_id == agent_id) {
            let old_status = agent.status;

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
                        // But still update CWD and event if provided
                        if new_cwd.is_some() {
                            agent.cwd = new_cwd;
                        }
                        if new_event.is_some() {
                            agent.last_event = new_event;
                        }
                        return; // Don't change status yet
                    }
                }
            }

            // NeedsInput and other transitions are immediate
            let status_changed = agent.status != new_status;
            agent.status = new_status;
            if new_cwd.is_some() {
                agent.cwd = new_cwd;
            }
            if new_event.is_some() {
                agent.last_event = new_event.clone();
            }

            if status_changed {
                let msg = format!(
                    "Status: {} {:?} -> {:?} (event: {:?})",
                    agent_id, old_status, new_status, new_event
                );
                // Note: push_debug called after agent borrow scope ends
                self.push_debug(&msg);
            }
        }
    }

    /// Check if any Unread agents should transition to Idle because their pane is now focused
    fn check_unread_transitions(&mut self) {
        let focused_id = self.state.focused_pane_id;

        // Collect agents that need transition (to avoid borrow conflicts)
        let mut agents_to_transition: Vec<(String, Option<String>)> = Vec::new();

        for agent in &self.state.agents {
            if agent.status == AgentStatus::Unread && agent.pane_id == focused_id {
                agents_to_transition.push((agent.agent_id.clone(), agent.cwd.clone()));
            }
        }

        // Apply transitions
        for (agent_id, cwd) in agents_to_transition {
            if let Some(agent) = self.state.agents.iter_mut().find(|a| a.agent_id == agent_id) {
                agent.status = AgentStatus::Idle;
                self.push_debug(&format!("Unread -> Idle: {} (pane focused)", agent_id));
            }
            // Write I: to status file so the state persists
            self.write_status_file(&agent_id, "I", cwd.as_deref());
        }
    }

    /// Write status to the agent's status file
    fn write_status_file(&self, agent_id: &str, status: &str, cwd: Option<&str>) {
        if !is_valid_agent_id(agent_id) {
            return;
        }

        // Ensure status directory exists with secure permissions
        ensure_status_dir();

        let content = match cwd {
            Some(path) => format!("{}:{}", status, path),
            None => status.to_string(),
        };

        let status_path = status_file_path(agent_id);
        let status_path_str = status_path.to_string_lossy().to_string();

        let mut env = BTreeMap::new();
        env.insert("STATUS_PATH".to_string(), status_path_str);
        env.insert("STATUS_CONTENT".to_string(), content);

        run_command_with_env_variables_and_cwd(
            &["/bin/sh", "-c", "printf '%s' \"$STATUS_CONTENT\" > \"$STATUS_PATH\""],
            env,
            PathBuf::from("."),
            BTreeMap::new(),
        );
    }

    /// Polls status files for all non-terminal agents and updates their status
    fn poll_status_files(&mut self) {
        // Collect updates first to avoid borrow conflicts with update_agent_status
        let mut status_updates: Vec<(String, AgentStatus, Option<String>, Option<String>)> = Vec::new();
        let mut pending_command_reads = Vec::new();

        for agent in &self.state.agents {
            // Don't poll terminal states
            if matches!(agent.status, AgentStatus::Completed | AgentStatus::Failed) {
                continue;
            }

            let read_result = read_agent_status(&agent.agent_id);

            match read_result {
                StatusRead::Parsed { status, event_name, cwd } => {
                    status_updates.push((agent.agent_id.clone(), status, event_name, cwd));
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
        for (agent_id, new_status, new_event, new_cwd) in status_updates {
            self.update_agent_status(&agent_id, new_status, new_event, new_cwd);
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

        let status_path = status_file_path(agent_id);
        let status_path_str = status_path.to_string_lossy().to_string();

        let mut env = BTreeMap::new();
        env.insert("STATUS_PATH".to_string(), status_path_str);
        env.insert("MAX_SIZE".to_string(), MAX_STATUS_FILE_SIZE.to_string());

        let mut context = BTreeMap::new();
        context.insert("kind".to_string(), "status_read".to_string());
        context.insert("agent_id".to_string(), agent_id.to_string());

        // Use head -c to bound output size, preventing DoS from oversized files
        run_command_with_env_variables_and_cwd(
            &["/bin/sh", "-c", "head -c \"$MAX_SIZE\" \"$STATUS_PATH\" 2>/dev/null"],
            env,
            PathBuf::from("."),
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
            last_event: None,
            created_tick: self.state.tick_count,
        });
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_agent_id_valid() {
        assert!(is_valid_agent_id("abc123"));
        assert!(is_valid_agent_id("a1b2c3d4"));
        assert!(is_valid_agent_id("ABCDEF"));
        assert!(is_valid_agent_id("a-b-c"));
    }

    #[test]
    fn test_is_valid_agent_id_invalid() {
        assert!(!is_valid_agent_id(""));  // empty
        assert!(!is_valid_agent_id("12345678901234567"));  // > 16 chars
        assert!(!is_valid_agent_id("abc/def"));  // path separator
        assert!(!is_valid_agent_id("abc\\def"));  // backslash
        assert!(!is_valid_agent_id("abc..def"));  // double dot
        assert!(!is_valid_agent_id("abc def"));  // space
        assert!(!is_valid_agent_id("abc!def"));  // special char
    }

    #[test]
    fn test_parse_status_content_with_cwd() {
        match parse_status_content("W:/path/to/cwd") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Working);
                assert_eq!(event_name, None);
                assert_eq!(cwd, Some("/path/to/cwd".to_string()));
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    #[test]
    fn test_parse_status_content_legacy() {
        match parse_status_content("I") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Idle);
                assert_eq!(event_name, None);
                assert_eq!(cwd, None);
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    #[test]
    fn test_parse_status_content_needs_input() {
        match parse_status_content("?:/some/path") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::NeedsInput);
                assert_eq!(event_name, None);
                assert_eq!(cwd, Some("/some/path".to_string()));
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    #[test]
    fn test_parse_status_content_invalid() {
        assert!(matches!(parse_status_content("X"), StatusRead::Unrecognized));
        assert!(matches!(parse_status_content("invalid"), StatusRead::Unrecognized));
    }

    // New format tests: W:EventName:/path
    #[test]
    fn test_parse_status_content_with_event_and_cwd() {
        match parse_status_content("W:PreToolUse:/home/user/project") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Working);
                assert_eq!(event_name, Some("PreToolUse".to_string()));
                assert_eq!(cwd, Some("/home/user/project".to_string()));
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    #[test]
    fn test_parse_status_content_with_event_no_cwd() {
        match parse_status_content("I:PostToolUse") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Idle);
                assert_eq!(event_name, Some("PostToolUse".to_string()));
                assert_eq!(cwd, None);
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    #[test]
    fn test_parse_status_content_notification_event() {
        match parse_status_content("?:Notification:/tmp/work") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::NeedsInput);
                assert_eq!(event_name, Some("Notification".to_string()));
                assert_eq!(cwd, Some("/tmp/work".to_string()));
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    #[test]
    fn test_parse_status_content_stop_event() {
        match parse_status_content("I:Stop:/var/log") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Idle);
                assert_eq!(event_name, Some("Stop".to_string()));
                assert_eq!(cwd, Some("/var/log".to_string()));
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    #[test]
    fn test_parse_status_content_unknown_event() {
        // Forward compatibility: accept unknown event names
        match parse_status_content("W:FutureHook:/path") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Working);
                assert_eq!(event_name, Some("FutureHook".to_string()));
                assert_eq!(cwd, Some("/path".to_string()));
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    #[test]
    fn test_parse_status_content_path_with_colons() {
        // Path with colons (backward compatible format): W:/path:with:colons
        match parse_status_content("W:/path:with:colons") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Working);
                assert_eq!(event_name, None);
                // splitn(3) preserves first colon in path, rest gets truncated
                assert_eq!(cwd, Some("/path:with:colons".to_string()));
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    #[test]
    fn test_parse_status_content_whitespace_trimmed() {
        match parse_status_content("  W:PreToolUse:/path  \n") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Working);
                assert_eq!(event_name, Some("PreToolUse".to_string()));
                assert_eq!(cwd, Some("/path".to_string()));
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    // Unread status tests
    #[test]
    fn test_parse_status_content_unread_with_cwd() {
        match parse_status_content("U:/home/user/project") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Unread);
                assert_eq!(event_name, None);
                assert_eq!(cwd, Some("/home/user/project".to_string()));
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    #[test]
    fn test_parse_status_content_unread_with_event() {
        match parse_status_content("U:Stop:/var/log") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Unread);
                assert_eq!(event_name, Some("Stop".to_string()));
                assert_eq!(cwd, Some("/var/log".to_string()));
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    #[test]
    fn test_parse_status_content_unread_legacy() {
        match parse_status_content("U") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Unread);
                assert_eq!(event_name, None);
                assert_eq!(cwd, None);
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    // Status directory tests
    #[test]
    fn test_status_dir_returns_path() {
        // status_dir should always return a valid PathBuf
        let dir = status_dir();
        assert!(!dir.as_os_str().is_empty());
        // Should end with agent-monitor or agent-monitor-{user}
        let dir_str = dir.to_string_lossy();
        assert!(dir_str.contains("agent-monitor"));
    }

    #[test]
    fn test_status_file_path_format() {
        let path = status_file_path("abc12345");
        let path_str = path.to_string_lossy();
        // Should contain the agent id and .status extension
        assert!(path_str.contains("abc12345"));
        assert!(path_str.ends_with(".status"));
    }

    #[test]
    fn test_status_file_path_uses_status_dir() {
        let dir = status_dir();
        let path = status_file_path("test1234");
        // The status file should be inside the status directory
        assert!(path.starts_with(&dir));
    }

    // Bounded read tests
    #[test]
    fn test_max_status_file_size_constant() {
        // Verify the constant is a reasonable value (4KB)
        assert_eq!(MAX_STATUS_FILE_SIZE, 4096);
        // Must be at least as large as MAX_CWD_LEN to accommodate full paths
        assert!(MAX_STATUS_FILE_SIZE >= MAX_CWD_LEN as u64);
    }

    #[test]
    fn test_parse_status_content_rejects_oversized_event() {
        // Event name longer than MAX_EVENT_NAME_LEN should be rejected
        let long_event = "A".repeat(MAX_EVENT_NAME_LEN + 1);
        let content = format!("W:{}:/path", long_event);
        match parse_status_content(&content) {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Working);
                assert_eq!(event_name, None); // Rejected due to length
                assert_eq!(cwd, Some("/path".to_string()));
            }
            _ => panic!("Expected Parsed variant with rejected event"),
        }
    }

    #[test]
    fn test_parse_status_content_accepts_max_length_event() {
        // Event name exactly at MAX_EVENT_NAME_LEN should be accepted
        let max_event = "A".repeat(MAX_EVENT_NAME_LEN);
        let content = format!("W:{}:/path", max_event);
        match parse_status_content(&content) {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Working);
                assert_eq!(event_name, Some(max_event));
                assert_eq!(cwd, Some("/path".to_string()));
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    // Sanitization tests
    #[test]
    fn test_sanitize_status_field_strips_control_chars() {
        // Test various control characters are stripped
        assert_eq!(sanitize_status_field("hello\x00world"), "helloworld");
        assert_eq!(sanitize_status_field("test\x1b[31mred\x1b[0m"), "test[31mred[0m");
        assert_eq!(sanitize_status_field("line\r\nbreak"), "linebreak");
        assert_eq!(sanitize_status_field("tab\there"), "tabhere");
    }

    #[test]
    fn test_sanitize_status_field_preserves_space() {
        // Space should be preserved (it's technically a control char but useful)
        assert_eq!(sanitize_status_field("hello world"), "hello world");
        assert_eq!(sanitize_status_field("  spaces  "), "  spaces  ");
    }

    #[test]
    fn test_sanitize_status_field_preserves_normal_text() {
        // Normal text should pass through unchanged
        assert_eq!(sanitize_status_field("PreToolUse"), "PreToolUse");
        assert_eq!(sanitize_status_field("/home/user/project"), "/home/user/project");
        assert_eq!(sanitize_status_field("path-with-dashes_and_underscores"), "path-with-dashes_and_underscores");
    }

    #[test]
    fn test_parse_status_content_sanitizes_event_name() {
        // Control characters in event name should be stripped
        match parse_status_content("W:Evil\x1b[31mEvent:/path") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Working);
                // ANSI escape sequence should have control char stripped
                assert_eq!(event_name, Some("Evil[31mEvent".to_string()));
                assert_eq!(cwd, Some("/path".to_string()));
            }
            _ => panic!("Expected Parsed variant"),
        }
    }

    #[test]
    fn test_parse_status_content_sanitizes_cwd() {
        // Control characters in cwd should be stripped
        match parse_status_content("I:/home/user\x00/project") {
            StatusRead::Parsed { status, event_name, cwd } => {
                assert_eq!(status, AgentStatus::Idle);
                assert_eq!(event_name, None);
                // Null byte should be stripped
                assert_eq!(cwd, Some("/home/user/project".to_string()));
            }
            _ => panic!("Expected Parsed variant"),
        }
    }
}
