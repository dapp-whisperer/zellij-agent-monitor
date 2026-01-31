---
title: "feat: Debug Storage + Activity Fallback"
type: feat
date: 2026-01-31
---

# Debug Storage + Activity Fallback (TMUXCC-Inspired)

## Overview

Add robust debugging infrastructure and activity detection fallbacks to the Zellij agent monitor plugin. This addresses two key limitations:

1. **Plugin file I/O appears blocked** - WASI restrictions prevent reliable file logging
2. **LLM generation time has no hook** - Internal generation (no tool calls) shows as Idle

## Problem Statement

Current state:
- Debug logs never appear in expected locations due to WASI filesystem restrictions
- Agent shows "Idle" during LLM internal generation because hooks only fire around tool use
- No visibility into plugin behavior when things go wrong

Impact:
- Debugging plugin issues requires guesswork
- Users see misleading "Idle" status during active generation
- Rapid tool execution causes status flicker

## Proposed Solution

Implement a **layered detection strategy** with dual-channel debug logging:

1. **Primary**: Hook status files (existing, reliable when tools run)
2. **Secondary**: Pane title spinner detection (covers LLM generation gaps)
3. **Tertiary**: Hysteresis buffer (prevents UI flicker)
4. **Debug**: In-memory ring buffer + host-side logging via `run_command`

## Technical Approach

### Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     State (lib.rs)                      │
├─────────────────────────────────────────────────────────┤
│  agents: Vec<Agent>                                     │
│  debug_ring: VecDeque<DebugEntry>  ← NEW                │
│  last_working_tick: HashMap<String, u64>  ← NEW         │
│  tick_count: u64  ← NEW (timer-based clock)             │
└─────────────────────────────────────────────────────────┘
                           │
         ┌─────────────────┼─────────────────┐
         ▼                 ▼                 ▼
   ┌──────────┐     ┌──────────────┐   ┌──────────────┐
   │ Hook     │     │ Spinner      │   │ Hysteresis   │
   │ Status   │     │ Detection    │   │ Buffer       │
   │ (Primary)│     │ (Secondary)  │   │ (Anti-flick) │
   └──────────┘     └──────────────┘   └──────────────┘
         │                 │                 │
         └─────────────────┴─────────────────┘
                           │
                           ▼
                    ┌─────────────┐
                    │ Final Status│
                    │ (UI Display)│
                    └─────────────┘
```

### Implementation Phases

#### Phase 1: Debug Storage Infrastructure

**Ring Buffer (in-memory)**

Add to `State` struct (`src/lib.rs:41-49`):

```rust
pub struct State {
    // ... existing fields ...
    pub debug_ring: VecDeque<DebugEntry>,
    pub tick_count: u64,
}

struct DebugEntry {
    tick: u64,
    message: String,
}

const DEBUG_RING_SIZE: usize = 100;
```

**Push method:**

```rust
fn push_debug(&mut self, msg: &str) {
    if self.debug_ring.len() >= DEBUG_RING_SIZE {
        self.debug_ring.pop_front();
    }
    self.debug_ring.push_back(DebugEntry {
        tick: self.tick_count,
        message: msg.to_string(),
    });

    // Also write to host filesystem
    self.write_debug_to_host(msg);
}
```

**Host-side logging (bypasses WASI):**

Enhance existing `append_debug_log_via_command()` at `src/lib.rs:408-425`:

```rust
fn write_debug_to_host(&self, line: &str) {
    let log_path = "/private/tmp/agent-monitor/plugin-logs/plugin-debug.log";
    let log_dir = "/private/tmp/agent-monitor/plugin-logs";
    let timestamp = self.tick_count;
    let formatted = format!("[{}] {}", timestamp, line);

    let mut env = BTreeMap::new();
    env.insert("LOG_DIR".to_string(), log_dir.to_string());
    env.insert("LOG_PATH".to_string(), log_path.to_string());
    env.insert("LOG_LINE".to_string(), formatted);

    run_command_with_env_variables_and_cwd(
        &["/bin/sh", "-lc",
          "mkdir -p \"$LOG_DIR\" && printf '%s\\n' \"$LOG_LINE\" >> \"$LOG_PATH\""],
        env,
        PathBuf::from("."),
        BTreeMap::new(),
    );
}
```

**UI footer display:**

In `render()` at `src/lib.rs:311-356`, add after agent list:

```rust
// Show last 5 debug entries in footer
println!("── Debug ──");
for entry in self.state.debug_ring.iter().rev().take(5).rev() {
    println!("  [{}] {}", entry.tick, entry.message);
}
```

#### Phase 2: Spinner Detection Fallback

**Subscribe to PaneUpdate:**

In `load()` at `src/lib.rs:161-168`, add `EventType::PaneUpdate`:

```rust
subscribe(&[
    EventType::Key,
    EventType::Timer,
    EventType::RunCommandResult,
    EventType::CommandPaneOpened,
    EventType::CommandPaneExited,
    EventType::PaneClosed,
    EventType::PaneUpdate,  // NEW - for spinner detection
]);
```

**Spinner character detection:**

```rust
const BRAILLE_SPINNERS: &[char] = &[
    '⠿', '⠇', '⠋', '⠙', '⠸', '⠴', '⠦', '⠧', '⠖', '⠏',
    '⠹', '⠼', '⠷', '⠾', '⠽', '⠻', '⠐', '⠑', '⠒', '⠓',
];

fn title_has_spinner(title: &str) -> bool {
    title.chars().any(|c| BRAILLE_SPINNERS.contains(&c))
}
```

**PaneUpdate handler in `update()`:**

```rust
Event::PaneUpdate(manifest) => {
    for (_tab_idx, panes) in &manifest.panes {
        for pane in panes {
            // Find matching agent by pane_id
            if let Some(agent) = self.state.agents.iter_mut()
                .find(|a| a.pane_id == Some(pane.id))
            {
                let spinner_active = title_has_spinner(&pane.title);

                // Only use spinner as fallback when hook says Idle
                if agent.status == AgentStatus::Idle && spinner_active {
                    agent.status = AgentStatus::Working;
                    self.push_debug(&format!(
                        "Fallback: spinner detected for {}", agent.agent_id
                    ));
                }
            }
        }
    }
    true
}
```

#### Phase 3: Hysteresis Buffer

**State additions:**

```rust
pub struct State {
    // ... existing ...
    pub last_working_tick: HashMap<String, u64>,
}

const HYSTERESIS_TICKS: u64 = 20;  // ~2s at 100ms timer rate
```

**Timer tick increment:**

In Timer handler at `src/lib.rs:276-303`:

```rust
Event::Timer => {
    self.state.tick_count += 1;
    self.state.spinner_frame = (self.state.spinner_frame + 1) % SPINNER.len();
    // ... existing polling logic ...
}
```

**Status transition with hysteresis:**

When updating agent status:

```rust
fn update_agent_status(&mut self, agent_id: &str, new_status: AgentStatus) {
    if let Some(agent) = self.state.agents.iter_mut()
        .find(|a| a.agent_id == agent_id)
    {
        let old_status = agent.status.clone();

        // Track when we were last Working
        if new_status == AgentStatus::Working {
            self.state.last_working_tick.insert(agent_id.to_string(), self.state.tick_count);
        }

        // Apply hysteresis only for Working -> Idle transitions
        if old_status == AgentStatus::Working && new_status == AgentStatus::Idle {
            if let Some(last_tick) = self.state.last_working_tick.get(agent_id) {
                let elapsed = self.state.tick_count - last_tick;
                if elapsed < HYSTERESIS_TICKS {
                    // Keep Working status, don't transition yet
                    self.push_debug(&format!(
                        "Hysteresis: keeping Working for {} ({}/{} ticks)",
                        agent_id, elapsed, HYSTERESIS_TICKS
                    ));
                    return;
                }
            }
        }

        // NeedsInput transitions are immediate (no hysteresis)
        agent.status = new_status;
    }
}
```

### Files to Modify

| File | Changes |
|------|---------|
| `src/lib.rs:41-49` | Add `debug_ring`, `tick_count`, `last_working_tick` to State |
| `src/lib.rs:17-24` | No changes to AgentStatus enum |
| `src/lib.rs:161-168` | Add `EventType::PaneUpdate` subscription |
| `src/lib.rs:184-309` | Add PaneUpdate handler, update Timer handler |
| `src/lib.rs:311-356` | Add debug footer rendering |
| `src/lib.rs:364-425` | Enhance debug logging with ring buffer + host write |

### State Machine

```
                    ┌───────────────┐
                    │  Initializing │
                    └───────┬───────┘
                            │ First signal received
                            ▼
        ┌───────────────────────────────────────┐
        │                                       │
        ▼                                       │
   ┌─────────┐  Hook: W or Spinner  ┌─────────┐ │
   │  Idle   │ ──────────────────► │ Working │ │
   │   (I)   │                      │   (W)   │ │
   └────┬────┘ ◄────────────────── └────┬────┘ │
        │      Hook: I + 2s hysteresis  │      │
        │                               │      │
        │       Hook: ?                 │      │
        │  ┌────────────────────────────┘      │
        ▼  ▼                                   │
   ┌──────────┐                                │
   │NeedsInput│ ───────────────────────────────┘
   │   (?)    │  Any other signal (immediate)
   └──────────┘
```

## Acceptance Criteria

### Functional Requirements

- [x] In-memory ring buffer stores last 100 debug entries
- [x] Debug entries visible in plugin UI footer (last 5 lines)
- [x] Host log updates at `/private/tmp/agent-monitor/plugin-logs/plugin-debug.log`
- [x] Agent shows "Working" during LLM generation (spinner fallback)
- [x] Status does not flicker during rapid tool execution (hysteresis)
- [x] NeedsInput (`?`) displays immediately (no hysteresis delay)
- [x] Hook status takes precedence over spinner detection

### Non-Functional Requirements

- [x] Timer overhead remains minimal (100ms Working / 500ms Idle polling)
- [x] Ring buffer memory bounded to ~100 entries
- [x] Host log write failures don't crash plugin

### Quality Gates

- [ ] Manual testing with real Claude Code sessions
- [ ] Verify all 5 test scenarios pass (see Testing section)

## Testing Plan

### Test 1: Hooks Only
1. Run agent, execute a tool command
2. Observe `W → I` transition in UI
3. Verify debug log shows transition

### Test 2: No Tools (LLM Generation)
1. Ask Claude a long question requiring extended generation
2. Observe spinner detection kicks in
3. UI should show `W` during generation

### Test 3: NeedsInput
1. Trigger a permission request (e.g., file write)
2. Verify `?` appears immediately
3. Approve, verify return to `I`

### Test 4: Hysteresis
1. Run rapid tool execution (multiple tools in sequence)
2. Status should stay at `W`, not flicker
3. After tools complete, 2s delay before `I`

### Test 5: Debug Logging
1. Check UI footer shows recent debug entries
2. Verify `/private/tmp/agent-monitor/plugin-logs/plugin-debug.log` updates
3. Entries should include tick count and message

## Dependencies & Risks

### Dependencies
- Zellij 0.43+ (for `PaneUpdate` event)
- No external crates needed (uses std `VecDeque`, `HashMap`)

### Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| PaneUpdate doesn't provide title | Low | High | Fall back to hook-only mode |
| run_command fails silently | Medium | Low | Ring buffer preserves logs in memory |
| Hysteresis feels laggy | Medium | Medium | Make threshold configurable |
| Spinner charset incomplete | Low | Low | Add chars as discovered |

## Future Considerations

### Phase 2 (Optional): Content Heuristics

If title spinner detection proves unreliable:
- Parse last ~20 lines of pane output
- Detect keywords: "thinking", "processing", "working"
- Requires Zellij API for pane content access (may not be available)

### Potential Enhancements
- Configurable hysteresis duration
- Toggle debug footer visibility with keybinding
- Log rotation for host-side log file
- Multiple agent status aggregation

## References

### Internal References
- State struct: `src/lib.rs:41-49`
- Event subscriptions: `src/lib.rs:161-168`
- Timer handler: `src/lib.rs:276-303`
- Render method: `src/lib.rs:311-356`
- Debug logging: `src/lib.rs:364-425`
- Status parsing: `src/lib.rs:102-138`

### External References
- TMUXCC spinner detection: https://github.com/nyanko3141592/tmuxcc
- Zellij plugin API: https://docs.rs/zellij-tile/0.43
- PaneInfo struct: https://docs.rs/zellij-utils/latest/zellij_utils/data/struct.PaneInfo.html

### Related Work
- Original plan: `PLAN_ACTIVITY_FALLBACK.md`
- TMUXCC learnings: `TMUXCC_LEARNINGS.md`
