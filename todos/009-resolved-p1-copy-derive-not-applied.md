---
status: pending
priority: p1
issue_id: "009"
tags: [code-review, rust-idioms, regression]
dependencies: []
---

# AgentStatus Copy Derive Not Actually Applied

## Problem Statement

TODO 005 was marked as resolved but the `Copy` derive was NOT actually added to `AgentStatus`. The enum at `src/lib.rs:16` still shows:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
```

Missing `Copy` means unnecessary `.clone()` calls are still occurring throughout the codebase.

## Findings

1. **Performance Oracle Agent**: Confirmed `Copy` is NOT present on line 16-17
2. **Pattern Recognition Agent**: Identified as Rust idiom violation
3. **Git History Analyzer**: Noted the discrepancy between TODO resolution and actual implementation

**Evidence**: `grep -n "derive.*Copy" src/lib.rs` returns no matches

## Proposed Solutions

### Option 1: Add Copy derive (Required)
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentStatus {
    Idle,
    Working,
    NeedsInput,
    Completed,
    Failed,
}
```

Then remove unnecessary `.clone()` calls:
- Line 560: `let old_status = agent.status.clone();` → `let old_status = agent.status;`
- Line 584: `agent.status = new_status.clone();` → `agent.status = new_status;`

- **Effort**: Trivial (5 minutes)
- **Risk**: None

## Recommended Action

Option 1 - This is a regression fix for an incomplete TODO resolution.

## Acceptance Criteria

- [ ] `AgentStatus` derives `Copy`
- [ ] All `.clone()` calls on AgentStatus values removed
- [ ] Code compiles with `cargo check`

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-02-01 | Created | Identified during code review - TODO 005 was not properly implemented |

## Resources

- Original TODO: `todos/005-resolved-p2-derive-copy-for-agentstatus.md`
- Performance review findings
