---
status: pending
priority: p2
issue_id: "012"
tags: [code-review, data-integrity]
dependencies: []
---

# Orphaned Agents with pane_id: None Never Cleaned Up

## Problem Statement

When `spawn_agent()` is called, an agent is added to the vector with `pane_id: None` BEFORE the `CommandPaneOpened` event fires. If the pane fails to open, the agent persists indefinitely.

**Locations**:
- `src/lib.rs:684-691` - Agent registered with `pane_id: None`
- `src/lib.rs:372` - Cleanup only matches agents with valid `pane_id`

```rust
// Agent registered before pane opens
self.state.agents.push(Agent {
    agent_id,
    pane_id: None,  // Not yet assigned
    ...
});

// Cleanup only removes agents with matching pane_id
self.state.agents.retain(|a| a.pane_id != Some(id));
```

## Findings

1. **Data Integrity Guardian**: Identified as HIGH severity - orphaned agents accumulate
2. If `CommandPaneOpened` never fires, status files are never cleaned up

## Proposed Solutions

### Option 1: Add timeout-based cleanup (Recommended)
Track when agents were created and remove those that never received a `pane_id`:

```rust
pub struct Agent {
    // ... existing fields ...
    pub created_tick: u64,
}

// In handle_timer, add cleanup:
const ORPHAN_TIMEOUT_TICKS: u64 = 100; // 10 seconds at 100ms
self.state.agents.retain(|a| {
    if a.pane_id.is_none() {
        let age = self.state.tick_count.saturating_sub(a.created_tick);
        if age > ORPHAN_TIMEOUT_TICKS {
            delete_status_file(&a.agent_id);
            return false; // Remove orphaned agent
        }
    }
    true
});
```

- **Effort**: Medium (30 minutes)
- **Risk**: Low

### Option 2: Register agent only after pane opens
Move agent registration to `handle_pane_opened()`:

- **Effort**: Medium
- **Risk**: Medium - may lose context between spawn and open events

## Recommended Action

Option 1 - Add timeout-based orphan cleanup.

## Acceptance Criteria

- [ ] Agents without `pane_id` are removed after timeout
- [ ] Status files for orphaned agents are cleaned up
- [ ] Normal spawn/open flow still works

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-02-01 | Created | Identified by data-integrity-guardian agent |

## Resources

- Data integrity review findings
