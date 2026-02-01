---
status: pending
priority: p2
issue_id: "017"
tags: [code-review, simplicity]
dependencies: []
---

# Legacy append_debug_log Wrapper Should Be Removed

## Problem Statement

The `append_debug_log()` method is explicitly marked as a legacy wrapper that just delegates to `push_debug()`. It has only one call site.

**Location**: `src/lib.rs:551-554`

```rust
/// Legacy method - now delegates to push_debug
fn append_debug_log(&mut self, line: &str) {
    self.push_debug(line);
}
```

Single call site at line 205:
```rust
self.append_debug_log("load: plugin initialized");
```

## Findings

1. **Simplicity Reviewer**: Identified as unnecessary indirection
2. **Pattern Recognition Agent**: Noted as dead code pattern

## Proposed Solutions

### Option 1: Remove wrapper, update call site (Recommended)
1. Change line 205 from `self.append_debug_log(...)` to `self.push_debug(...)`
2. Delete lines 551-554

- **Effort**: Trivial (2 minutes)
- **Risk**: None

## Recommended Action

Option 1 - Remove the legacy wrapper.

## Acceptance Criteria

- [ ] `append_debug_log` method removed
- [ ] Call site updated to use `push_debug` directly
- [ ] Code compiles with `cargo check`

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-02-01 | Created | Identified by simplicity-reviewer agent |

## Resources

- Simplicity review findings
