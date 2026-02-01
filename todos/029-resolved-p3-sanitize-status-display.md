---
status: ready
priority: p3
issue_id: "029"
tags: [code-review, security, quality]
dependencies: []
---

# Sanitize status fields before rendering

## Problem Statement

Status fields (event name and cwd) come from untrusted status files and are rendered directly. Control characters could cause UI spoofing or confusing output.

## Findings

- `src/lib.rs:157-239` parses `event_name` and `cwd` without sanitizing control characters.
- `src/lib.rs:320-343` renders `cwd` directly into UI output.
- Debug log entries also include derived strings without filtering.

## Proposed Solutions

### Option 1: Strip control characters (Recommended)

**Approach:** Add a small sanitizer that removes ASCII control chars (including ANSI escape) before storing or rendering `cwd` and `event_name`.

**Pros:**
- Prevents UI spoofing
- Minimal change

**Cons:**
- Slightly alters display for unusual paths

**Effort:** 1-2 hours

**Risk:** Low

---

### Option 2: Escape control characters

**Approach:** Replace control chars with visible placeholders (e.g., `?`).

**Pros:**
- Maintains visibility into malformed values

**Cons:**
- Extra formatting logic

**Effort:** 2-3 hours

**Risk:** Low

## Recommended Action

Option 1: Strip control characters. Add sanitizer to remove ASCII control chars and ANSI escapes before storing `cwd` and `event_name` in state.

## Technical Details

**Affected files:**
- `src/lib.rs:157`
- `src/lib.rs:320`

**Related components:**
- Status parsing and UI rendering

**Database changes (if any):**
- No

## Resources

- Review notes from security-sentinel

## Acceptance Criteria

- [ ] Control characters are removed or escaped before display
- [ ] UI rendering remains stable with malformed status files
- [ ] Tests added or updated to cover sanitized inputs

## Work Log

### 2026-02-01 - Initial Discovery

**By:** Claude Code

**Actions:**
- Identified untrusted status fields in UI output
- Drafted sanitization approaches

**Learnings:**
- Status files are untrusted input surfaces

### 2026-02-01 - Approved for Work

**By:** Claude Triage System

**Actions:**
- Issue approved during triage session
- Status changed from pending → ready
- Selected Option 1: Strip control characters

**Learnings:**
- Sanitize at input boundary (parsing), not output boundary (rendering)

## Notes

- Consider sanitizing before storing in state, not just before rendering
