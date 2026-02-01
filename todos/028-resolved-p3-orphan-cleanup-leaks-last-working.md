---
status: ready
priority: p3
issue_id: "028"
tags: [code-review, quality]
dependencies: []
---

# Orphan cleanup does not clear last_working_tick entries

## Problem Statement

When orphaned agents (no pane_id after timeout) are removed, their `last_working_tick` entries are not cleared. Over time this can leak entries and skew hysteresis bookkeeping.

## Findings

- `handle_timer` retains agents and deletes status files for orphans.
- The same path does not remove `last_working_tick` entries for removed agent IDs.
- `handle_pane_closed` does remove map entries, but orphan cleanup path does not.

## Proposed Solutions

### Option 1: Remove map entries during orphan cleanup

**Approach:** When an orphan is dropped, call `last_working_tick.remove(&agent_id)` before returning `false`.

**Pros:**
- Direct fix, minimal code change.

**Cons:**
- Requires minor refactor to access agent_id in retain closure.

**Effort:** 30-60 minutes

**Risk:** Low

---

### Option 2: Sweep map after retain

**Approach:** After retain, rebuild `last_working_tick` by retaining only keys that still exist in `agents`.

**Pros:**
- Keeps map consistent with agent list.

**Cons:**
- Extra iteration on every timer tick.

**Effort:** 1-2 hours

**Risk:** Low

## Recommended Action

Option 1: Remove map entries during orphan cleanup. Call `last_working_tick.remove(&agent_id)` when removing orphans.

## Technical Details

**Affected files:**
- `src/lib.rs`

**Related components:**
- Orphan cleanup in `handle_timer`
- Hysteresis tracking

## Resources

- **Branch:** `feat/status-detection-hooks` (no PR)

## Acceptance Criteria

- [ ] Removing an orphaned agent also removes its `last_working_tick` entry.
- [ ] No regressions in hysteresis behavior.

## Work Log

### 2026-02-01 - Initial Discovery

**By:** Codex

**Actions:**
- Reviewed orphan cleanup path and map lifecycle.
- Identified missing cleanup for `last_working_tick`.

**Learnings:**
- Map cleanup only occurs on pane close, not orphan removal.

### 2026-02-01 - Approved for Work

**By:** Claude Triage System

**Actions:**
- Issue approved during triage session
- Status changed from pending → ready
- Selected Option 1: Remove map entries during orphan cleanup

**Learnings:**
- All cleanup paths should be consistent in what state they clear

## Notes

- Small leak, but persistent for long-running sessions.
