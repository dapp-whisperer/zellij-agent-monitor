---
status: pending
priority: p2
issue_id: "004"
tags: [code-review, performance, simplicity]
dependencies: []
---

# Redundant WASI File Reads Before Command Fallback

## Problem Statement

The `poll_status_files()` function attempts to read status files via `std::fs` first, which usually fails due to WASI restrictions, then falls back to `request_status_via_command()` which spawns a shell.

This creates wasted syscalls on every poll cycle for agents without status files.

**Location:** `src/lib.rs:527-562`

## Findings

1. **Performance Oracle Agent**: Identified as MEDIUM severity - double work
2. **Simplicity Reviewer**: Noted as redundant code pattern
3. WASI restrictions mean `std::fs` reads typically fail for this plugin

## Proposed Solutions

### Option 1: Skip WASI reads entirely (Recommended)
```rust
fn poll_status_files(&mut self) {
    for agent in &self.state.agents {
        if matches!(agent.status, AgentStatus::Completed | AgentStatus::Failed) {
            continue;
        }
        // Skip WASI read, go directly to command
        self.request_status_via_command(&agent.agent_id);
    }
}
```
- **Pros**: Eliminates wasted syscalls, simpler code
- **Cons**: Loses potential for WASI reads if they ever work
- **Effort**: Small
- **Risk**: Low

### Option 2: Cache WASI failure state
Try WASI once at startup, remember if it works, skip if not.
- **Pros**: Adapts to environment
- **Cons**: More complex
- **Effort**: Medium
- **Risk**: Low

## Recommended Action

Option 1 - Remove `read_agent_status()` calls and use command-based reading exclusively.

## Acceptance Criteria

- [ ] Status polling uses only command-based reads
- [ ] No wasted WASI syscalls
- [ ] Status detection still works correctly

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-01-31 | Created | Identified by performance-oracle and simplicity-reviewer agents |
