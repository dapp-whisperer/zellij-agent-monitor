---
title: "fix: Clear spinner when agent is interrupted"
type: fix
date: 2026-02-01
---

# fix: Clear spinner when agent is interrupted

## Overview

When a user interrupts a Claude Code agent (Ctrl+C or Escape), the agent stays in the pane but the plugin continues showing the Working spinner `[⠋]` instead of returning to Idle `[.]`. This creates a confusing UI where the agent appears to still be working when it's actually waiting for input.

## Problem Statement

**Current behavior:**
1. Agent is Working → plugin shows spinner `[⠋]`
2. User presses Ctrl+C to interrupt
3. Agent stops working, shows prompt waiting for input
4. Plugin still shows spinner `[⠋]` (stuck)

**Expected behavior:**
1. Agent is Working → plugin shows spinner `[⠋]`
2. User presses Ctrl+C to interrupt
3. Agent stops working, shows prompt waiting for input
4. Plugin shows idle `[.]`

## Root Cause Analysis

The plugin has **asymmetric spinner detection**:
- ✅ When `Idle` + spinner detected → set to `Working` (line 630-631)
- ❌ When `Working` + spinner disappears → no action (missing logic)

Additionally, the Claude Code `Stop` hook [explicitly does not fire on user interrupt](https://code.claude.com/docs/en/hooks), so hook-based status updates won't help here.

## Proposed Solution

Add reverse spinner detection: when an agent is `Working` and the pane title no longer contains a spinner, transition to `Idle`.

### Technical Approach

In `handle_pane_update()`, after detecting spinner presence:

```rust
// Current: Idle + spinner → Working
if agent.status == AgentStatus::Idle && spinner_active {
    spinner_detected.push(agent.agent_id.clone());
}

// NEW: Working + no spinner → Idle (with hysteresis check)
if agent.status == AgentStatus::Working && !spinner_active {
    // Check if we've been in Working state long enough
    // to avoid flickering on brief spinner absences
    if let Some(last_tick) = self.state.last_working_tick.get(&agent.agent_id) {
        let ticks_since_working = self.state.tick_count.saturating_sub(*last_tick);
        if ticks_since_working >= SPINNER_ABSENCE_THRESHOLD {
            spinner_cleared.push(agent.agent_id.clone());
        }
    }
}
```

### Constants

```rust
// Number of ticks without spinner before transitioning Working → Idle
// ~1 second at 100ms tick rate, balances responsiveness vs. flicker
const SPINNER_ABSENCE_THRESHOLD: u64 = 10;
```

### Edge Cases

1. **Brief spinner absence during tool execution**: The hysteresis threshold prevents premature Idle transition
2. **Agent crashed vs interrupted**: Both result in no spinner; Idle is appropriate for either
3. **Conflicting with hook status**: Hook-based status should take precedence over spinner fallback

### Priority Logic

The detection should only apply as a **fallback**:
1. If status file says `I:` → trust it (hook-based)
2. If status file says `W:` but no spinner for N ticks → override to Idle
3. If no status file exists → use spinner detection only

## Acceptance Criteria

- [x] When user interrupts agent (Ctrl+C), status changes from `[⠋]` to `[.]` within ~1 second
- [x] Brief spinner absences during normal operation don't cause flickering
- [x] Hook-based status updates still take precedence
- [x] Debug log shows spinner-based state transitions

## Files to Modify

- `src/lib.rs`
  - Add `SPINNER_ABSENCE_THRESHOLD` constant (~line 90)
  - Modify `handle_pane_update()` to detect spinner disappearance (~line 603-651)
  - Track "last spinner seen tick" per agent (may need new state field or reuse existing)

## Testing

Manual test scenarios:
1. Start agent, let it work on a task, interrupt with Ctrl+C → should show `[.]`
2. Start agent, let it work normally without interrupt → should show `[⠋]` while working, `[.]` when done
3. Agent with rapid tool calls → should not flicker between states

## References

- [Claude Code Hooks Documentation](https://code.claude.com/docs/en/hooks) - confirms Stop hook doesn't fire on interrupt
- Current spinner detection: `src/lib.rs:603-651`
- Hysteresis logic for Working→Idle: `src/lib.rs:738-786`
