---
status: resolved
priority: p2
issue_id: "026"
tags: [code-review, quality, docs]
dependencies: []
---

# Status hook ID mismatch prevents updates

## Problem Statement

Status detection relies on status files named by the agent UUID, but the README hook setup still writes files keyed by `ZELLIJ_PANE_ID`. This mismatch means the plugin won’t find the status files and agents stay `Idle` (unless the spinner fallback kicks in), undermining the core feature.

## Findings

- `src/lib.rs` spawns agents with `AGENT_MONITOR_ID` and reads `/tmp/agent-monitor/<agent_id>.status`.
- README hook examples write `/tmp/agent-monitor/${ZELLIJ_PANE_ID:-$$}.status`.
- With the documented hook, the plugin never reads the correct file, so status updates are skipped.

## Proposed Solutions

### Option 1: Update README and hook examples to use `AGENT_MONITOR_ID`

**Approach:** Document hooks to write `AGENT_MONITOR_ID` and fall back to `ZELLIJ_PANE_ID` only when missing.

**Pros:**
- Aligns with current implementation.
- Minimal code changes.

**Cons:**
- Existing users need to update configs.

**Effort:** 30-60 minutes

**Risk:** Low

---

### Option 2: Support both `AGENT_MONITOR_ID` and pane ID in the plugin

**Approach:** If the agent has a known `pane_id`, check `/tmp/agent-monitor/<pane_id>.status` in addition to the UUID-based path.

**Pros:**
- Backward compatible with existing hooks.
- Improves upgrade experience.

**Cons:**
- More complexity and branching.
- Risk of ambiguous state if both exist.

**Effort:** 1-2 hours

**Risk:** Medium

---

### Option 3: Switch back to pane-id-based status keys

**Approach:** Use pane_id as the status filename once `CommandPaneOpened` fires, and migrate existing UUID-based handling.

**Pros:**
- Aligns with current documented hook behavior.
- Less custom configuration for users.

**Cons:**
- Harder to reconcile pre-open states.
- Larger refactor.

**Effort:** 3-4 hours

**Risk:** Medium

## Recommended Action

Option 1: Update README and hook examples to use `AGENT_MONITOR_ID`. Document hooks to write `AGENT_MONITOR_ID` and fall back to `ZELLIJ_PANE_ID` only when missing.

## Technical Details

**Affected files:**
- `src/lib.rs`
- `README.md`

**Related components:**
- Status hook integration
- Status file reader

## Resources

- **Branch:** `feat/status-detection-hooks` (no PR)

## Acceptance Criteria

- [x] Status updates work with the documented hook configuration.
- [x] README and implementation agree on file naming.
- [ ] Manual verification: status changes to Working/Idle/NeedsInput with hooks enabled.

## Work Log

### 2026-02-01 - Initial Discovery

**By:** Codex

**Actions:**
- Reviewed status file naming and hook examples.
- Identified mismatch between `AGENT_MONITOR_ID` and `ZELLIJ_PANE_ID`.

**Learnings:**
- Current implementation and docs are out of sync, causing status reads to fail.

### 2026-02-01 - Approved for Work

**By:** Claude Triage System

**Actions:**
- Issue approved during triage session
- Status changed from pending → ready
- Selected Option 1: Update README to use AGENT_MONITOR_ID

**Learnings:**
- Documentation/implementation sync is critical for user experience

### 2026-02-01 - Resolved

**By:** Claude Opus 4.5

**Actions:**
- Updated all hook examples in README.md to use `${AGENT_MONITOR_ID:-${ZELLIJ_PANE_ID:-$$}}`
- Added explanatory note about `AGENT_MONITOR_ID` being set by the plugin
- Added variable priority documentation
- Updated troubleshooting section to reference `AGENT_MONITOR_ID`
- Updated "How It Works" section to clarify UUID-based naming

**Learnings:**
- Fallback chain ensures hooks work in all contexts: plugin-spawned, standalone Zellij, or outside Zellij

## Notes

- This impacts core UX; consider prioritizing for next release.
