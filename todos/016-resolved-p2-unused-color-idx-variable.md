---
status: pending
priority: p2
issue_id: "016"
tags: [code-review, dead-code]
dependencies: []
---

# Unused _color_idx Variable in Render

## Problem Statement

The render function computes `_color_idx` but never uses it. The underscore prefix indicates it was intentionally ignored but the computation should be removed.

**Location**: `src/lib.rs:239-248`

```rust
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
```

## Findings

1. **Pattern Recognition Agent**: Identified as dead code
2. **Simplicity Reviewer**: Noted as YAGNI violation - color support was planned but not implemented
3. Comments suggest colors but Zellij plugin API may not support them

## Proposed Solutions

### Option 1: Remove color_idx entirely (Recommended)
```rust
let status_str = match agent.status {
    AgentStatus::Idle => "[.]".to_string(),
    AgentStatus::Working => format!("[{}]", SPINNER[self.state.spinner_frame]),
    AgentStatus::NeedsInput => "[?]".to_string(),
    AgentStatus::Completed => "[v]".to_string(),
    AgentStatus::Failed => "[X]".to_string(),
};
```

- **Effort**: Trivial (5 minutes)
- **Risk**: None

### Option 2: Implement color support
Research Zellij plugin color API and implement the intended feature.

- **Effort**: Medium-Large
- **Risk**: Low

## Recommended Action

Option 1 - Remove unused computation. Implement colors later if needed.

## Acceptance Criteria

- [ ] `_color_idx` variable removed
- [ ] Match expression simplified
- [ ] Code compiles with `cargo check`

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-02-01 | Created | Identified by pattern-recognition and simplicity-reviewer agents |

## Resources

- Pattern recognition and simplicity review findings
