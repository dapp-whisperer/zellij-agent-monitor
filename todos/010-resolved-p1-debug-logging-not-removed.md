---
status: pending
priority: p1
issue_id: "010"
tags: [code-review, performance, regression]
dependencies: []
---

# Debug Filesystem Logging Not Actually Removed

## Problem Statement

TODO 002 was marked as resolved but `write_debug_to_host()` was NOT removed. The function still exists at `src/lib.rs:529-549` and is still called from `push_debug()` at line 525.

This spawns a shell process (`/bin/sh -lc`) for EVERY debug message, causing significant performance overhead.

## Findings

1. **Performance Oracle Agent**: Identified as HIGH severity - shell spawning still active
2. **Simplicity Reviewer**: Noted as development artifact that should be removed
3. **Git History Analyzer**: Confirmed the function is still present

**Evidence**:
```
grep -n "write_debug_to_host" src/lib.rs
525:        self.write_debug_to_host(msg);
529:    fn write_debug_to_host(&self, line: &str) {
```

## Proposed Solutions

### Option 1: Remove filesystem logging entirely (Recommended)
1. Remove the call at line 525: `self.write_debug_to_host(msg);`
2. Remove the entire `write_debug_to_host()` function (lines 529-549)
3. Keep only the ring buffer for debugging (visible in UI)

- **Effort**: Small (10 minutes)
- **Risk**: None - ring buffer provides sufficient debugging capability

### Option 2: Gate behind debug flag
```rust
#[cfg(debug_assertions)]
fn write_debug_to_host(&self, line: &str) { ... }

#[cfg(not(debug_assertions))]
fn write_debug_to_host(&self, _line: &str) {}
```

- **Effort**: Small
- **Risk**: None

## Recommended Action

Option 1 - Complete removal. The in-memory ring buffer shown in the UI footer is sufficient for debugging.

## Acceptance Criteria

- [ ] `write_debug_to_host()` function removed
- [ ] Call to `write_debug_to_host()` in `push_debug()` removed
- [ ] Debug ring buffer still works and displays in UI
- [ ] Code compiles with `cargo check`

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-02-01 | Created | Identified during code review - TODO 002 was not properly implemented |

## Resources

- Original TODO: `todos/002-resolved-p2-shell-spawn-overhead.md`
- Performance review findings
