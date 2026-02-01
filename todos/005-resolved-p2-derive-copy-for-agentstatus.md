---
status: pending
priority: p2
issue_id: "005"
tags: [code-review, performance, rust-idioms]
dependencies: []
---

# AgentStatus Should Derive Copy

## Problem Statement

`AgentStatus` is cloned in multiple places but all variants are unit variants (zero-sized). It should derive `Copy` to eliminate unnecessary clone overhead.

**Location:** `src/lib.rs:16-29`

Current:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Idle, Working, NeedsInput, Completed, Failed,
}
```

Clones at:
- Line 488: `let old_status = agent.status.clone();`
- Line 512: `agent.status = new_status.clone();`

## Findings

1. **Pattern Recognition Agent**: Identified as Rust idiom violation
2. All enum variants are unit types - no data, should be `Copy`

## Proposed Solutions

### Option 1: Add Copy derive (Recommended)
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentStatus { ... }
```
- **Pros**: One word change, eliminates clone overhead
- **Cons**: None
- **Effort**: Trivial
- **Risk**: None

## Recommended Action

Option 1 - Add `Copy` to derive list.

## Acceptance Criteria

- [ ] `AgentStatus` derives `Copy`
- [ ] Remove explicit `.clone()` calls on `AgentStatus` values
- [ ] Code compiles without changes to logic

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-01-31 | Created | Identified by pattern-recognition-specialist agent |
