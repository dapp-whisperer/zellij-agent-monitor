---
status: pending
priority: p2
issue_id: "015"
tags: [code-review, dead-code]
dependencies: []
---

# Dead Field: debug_log_path Never Used

## Problem Statement

The `debug_log_path` field in `State` struct is declared but never read or written.

**Location**: `src/lib.rs:52`

```rust
pub struct State {
    pub agents: Vec<Agent>,
    pub selected: usize,
    pub agent_counter: u32,
    pub spinner_frame: usize,
    pub debug_log_path: Option<String>,  // <- DEAD FIELD
    pub debug_ring: VecDeque<DebugEntry>,
    // ...
}
```

Debug logging uses hardcoded paths instead (lines 530-531):
```rust
let log_path = "/private/tmp/agent-monitor/plugin-logs/plugin-debug.log";
let log_dir = "/private/tmp/agent-monitor/plugin-logs";
```

## Findings

1. **Pattern Recognition Agent**: Identified as dead code
2. Field was likely planned for configurable logging but never implemented

## Proposed Solutions

### Option 1: Remove the field (Recommended)
Simply delete line 52.

- **Effort**: Trivial (2 minutes)
- **Risk**: None

### Option 2: Use the field for configuration
If configurable logging is desired, update `write_debug_to_host()` to use this field.

- **Effort**: Medium
- **Risk**: Low

## Recommended Action

Option 1 - Remove the dead field. If configurable logging is needed later, it can be re-added.

## Acceptance Criteria

- [ ] `debug_log_path` field removed from `State`
- [ ] Code compiles with `cargo check`

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-02-01 | Created | Identified by pattern-recognition-specialist agent |

## Resources

- Pattern recognition review findings
