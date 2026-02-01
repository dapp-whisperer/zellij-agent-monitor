---
status: pending
priority: p1
issue_id: "011"
tags: [code-review, data-integrity, memory-leak]
dependencies: []
---

# last_working_tick HashMap Memory Leak

## Problem Statement

The `last_working_tick` HashMap at `src/lib.rs:56` grows unbounded. Entries are inserted when agents become Working but are NEVER removed when agents are closed.

**Location**: `src/lib.rs:56, 448-451, 564`

```rust
pub last_working_tick: HashMap<String, u64>,

// Inserts happen:
self.state.last_working_tick.insert(agent_id.clone(), self.state.tick_count);  // Line 449
self.state.last_working_tick.insert(agent_id.to_string(), self.state.tick_count);  // Line 564
```

Over long sessions, this causes unbounded memory growth.

## Findings

1. **Data Integrity Guardian**: Identified as HIGH severity memory leak
2. No cleanup occurs in `handle_pane_closed()` (lines 358-385)

## Proposed Solutions

### Option 1: Clean up in handle_pane_closed (Recommended)
Add cleanup before the agent is removed from the vector:

```rust
fn handle_pane_closed(&mut self, pane_id: PaneId) -> bool {
    if let PaneId::Terminal(id) = pane_id {
        // ... existing code ...

        // Clean up HashMap entry
        if let Some(agent_id) = agent_id_to_cleanup.as_ref() {
            self.state.last_working_tick.remove(agent_id);
        }

        // ... rest of function ...
    }
}
```

- **Effort**: Trivial (5 minutes)
- **Risk**: None

### Option 2: Move last_working_tick into Agent struct
Store the tick directly in the `Agent` struct to avoid HashMap overhead entirely:

```rust
pub struct Agent {
    // ... existing fields ...
    pub last_working_tick: u64,
}
```

- **Effort**: Medium (30 minutes - requires updating all access sites)
- **Risk**: Low

## Recommended Action

Option 1 for immediate fix, consider Option 2 as a follow-up optimization.

## Acceptance Criteria

- [ ] `last_working_tick` entries removed when agents close
- [ ] Long-running sessions don't leak memory
- [ ] Code compiles with `cargo check`

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-02-01 | Created | Identified by data-integrity-guardian agent |

## Resources

- Data integrity review findings
