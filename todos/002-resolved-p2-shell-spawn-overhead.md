---
status: pending
priority: p2
issue_id: "002"
tags: [code-review, performance]
dependencies: []
---

# Shell Spawn Per Debug Message Causes Performance Overhead

## Problem Statement

The `write_debug_to_host()` function at `src/lib.rs:456-477` spawns a new shell process for every debug message. It uses `/bin/sh -lc` (login shell) which loads user profile files, adding 20-100ms overhead per invocation.

At 100ms polling with debug messages each tick, this could spawn 10+ shell processes per second during active monitoring.

## Findings

1. **Performance Oracle Agent**: Identified as HIGH severity performance issue
2. The `-l` flag (login shell) is unnecessary - it loads `.bash_profile`, `.zshrc`, etc.
3. Current implementation: ~5-15ms per spawn + 20-100ms for login shell overhead

## Proposed Solutions

### Option 1: Remove login shell flag (Quick fix)
```rust
run_command_with_env_variables_and_cwd(
    &["/bin/sh", "-c", ...],  // Changed from "-lc" to "-c"
    ...
);
```
- **Pros**: One character change, immediate improvement
- **Cons**: Still spawns shell per message
- **Effort**: Trivial
- **Risk**: None

### Option 2: Remove filesystem logging entirely (Recommended)
Keep only the ring buffer for debugging; remove `write_debug_to_host()` call from `push_debug()`.
```rust
fn push_debug(&mut self, msg: &str) {
    if self.state.debug_ring.len() >= DEBUG_RING_SIZE {
        self.state.debug_ring.pop_front();
    }
    self.state.debug_ring.push_back(DebugEntry {
        tick: self.state.tick_count,
        message: msg.to_string(),
    });
    // Remove: self.write_debug_to_host(msg);
}
```
- **Pros**: Eliminates all shell spawning, ~25 lines removed
- **Cons**: Loses persistent debug log file
- **Effort**: Small
- **Risk**: Low (ring buffer visible in UI)

### Option 3: Batch filesystem writes
Write to filesystem every N messages or every N seconds instead of every message.
- **Pros**: Reduces spawns by 90%+
- **Cons**: More complex implementation
- **Effort**: Medium
- **Risk**: Low

## Recommended Action

Option 2 - Remove filesystem logging. The ring buffer display in the UI is sufficient for debugging, and this eliminates significant overhead.

## Technical Details

**Affected files:**
- `src/lib.rs:452-453` (call to write_debug_to_host)
- `src/lib.rs:456-477` (write_debug_to_host function - can be removed)

## Acceptance Criteria

- [ ] `push_debug()` no longer spawns shell processes
- [ ] Debug ring buffer still visible in UI footer
- [ ] Plugin performance improved during active monitoring

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-01-31 | Created | Identified by performance-oracle agent during code review |

## Resources

- Benchmark: `time for i in {1..100}; do /bin/sh -lc "echo test"; done` vs `-c`
