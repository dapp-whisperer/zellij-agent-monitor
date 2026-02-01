---
status: ready
priority: p2
issue_id: "023"
tags: [code-review, performance, architecture]
dependencies: []
---

# Consolidate Timer Tick O(n) Scans

## Problem Statement

The `handle_timer()` function performs 4 separate O(n) iterations over the agents list on every tick (100-500ms). This could be consolidated into a single pass for better performance at scale.

## Findings

**From performance-oracle:**
- `retain()` for orphan cleanup: O(n)
- `poll_status_files()`: O(n) with file I/O
- `any()` for has_working: O(n)
- `any()` for NeedsInput: O(n)

At 100 agents, this results in 400 iterations per tick.

**From architecture-strategist:**
- `handle_timer` has mixed responsibilities violating SRP
- Combines tick counting, spinner, orphan cleanup, polling, timeout calculation, render decision

## Proposed Solutions

### Option A: Single-Pass Consolidation (Recommended)
Collect all needed state in one iteration:

```rust
fn handle_timer(&mut self, _elapsed: f64) -> bool {
    self.state.tick_count += 1;
    self.state.spinner_frame = (self.state.spinner_frame + 1) % SPINNER.len();

    let tick = self.state.tick_count;
    let mut has_working = false;
    let mut needs_input = false;
    let mut orphans: Vec<usize> = Vec::new();

    for (i, agent) in self.state.agents.iter().enumerate() {
        if agent.pane_id.is_none() && tick.saturating_sub(agent.created_tick) > ORPHAN_TIMEOUT_TICKS {
            orphans.push(i);
        }
        match agent.status {
            AgentStatus::Working => has_working = true,
            AgentStatus::NeedsInput => needs_input = true,
            _ => {}
        }
    }

    // Remove orphans in reverse order
    for i in orphans.into_iter().rev() {
        let agent = self.state.agents.remove(i);
        delete_status_file(&agent.agent_id);
    }

    self.poll_status_files();
    set_timeout(if has_working { FAST_POLL_SECS } else { SLOW_POLL_SECS });
    has_working || needs_input
}
```

**Pros:** Reduces 4 passes to 1+1 (the polling loop is still separate)
**Cons:** Slightly more complex single loop
**Effort:** Small (30 min)
**Risk:** Low

### Option B: Extract Helper Methods (SRP Focus)
Split `handle_timer` into focused helpers:

```rust
fn handle_timer(&mut self, _elapsed: f64) -> bool {
    self.advance_tick();
    self.advance_spinner();
    self.cleanup_orphans();
    self.poll_status_files();
    self.should_rerender()
}
```

**Pros:** Clean separation, testable helpers
**Cons:** Doesn't reduce iterations, adds method call overhead
**Effort:** Medium (1 hour)
**Risk:** Low

## Recommended Action

Option A: Single-pass consolidation. Collect orphans, has_working, and needs_input in one iteration. Reduces O(4n) to O(2n) per tick.

## Technical Details

**Affected files:**
- `src/lib.rs:474-514` (handle_timer)

**Current complexity:** O(4n) per tick
**Target complexity:** O(2n) per tick (single state pass + polling pass)

## Acceptance Criteria

- [ ] `handle_timer` performs maximum 2 iterations over agents
- [ ] Orphan cleanup still works correctly
- [ ] Adaptive polling still works
- [ ] Tests pass

## Work Log

| Date | Action | Result |
|------|--------|--------|
| 2026-02-01 | Created from code review | - |

## Resources

- Code review findings from performance-oracle and architecture-strategist
- Source: `src/lib.rs:474-514`

### 2026-02-01 - Approved for Work

**By:** Claude Triage System

**Actions:**
- Issue approved during triage session
- Status changed from pending → ready
- Selected Option A: Single-pass consolidation

**Learnings:**
- Simple loop consolidation can halve iteration count with minimal complexity
