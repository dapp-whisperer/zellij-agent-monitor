---
status: ready
priority: p3
issue_id: "029"
tags: [code-review, docs]
dependencies: []
---

# README status indicators do not match UI

## Problem Statement

The README lists status indicators that don’t match the actual UI characters. This can confuse users when verifying or troubleshooting status states.

## Findings

- UI renders Idle as `[.]` and Completed as `[v]` in `src/lib.rs`.
- README lists Idle as `[·]` and Completed as `[✓]`.
- Working and NeedsInput match in intent but not in actual indicator examples.

## Proposed Solutions

### Option 1: Update README to match UI

**Approach:** Replace indicator examples with the exact characters used in the UI.

**Pros:**
- Simple documentation fix.
- No code changes required.

**Cons:**
- Leaves UI as ASCII-only if that was not intended.

**Effort:** 15-30 minutes

**Risk:** Low

---

### Option 2: Update UI to match README

**Approach:** Swap the UI indicators to `·` and `✓`.

**Pros:**
- Matches more visually descriptive README.

**Cons:**
- Requires ensuring font/terminal support.

**Effort:** 30-60 minutes

**Risk:** Low

## Recommended Action

Option 1: Update README to match UI. Replace indicator examples with exact ASCII characters used in the code.

## Technical Details

**Affected files:**
- `src/lib.rs`
- `README.md`

## Resources

- **Branch:** `feat/status-detection-hooks` (no PR)

## Acceptance Criteria

- [ ] README examples match UI output.
- [ ] Status indicator section remains accurate for all states.

## Work Log

### 2026-02-01 - Initial Discovery

**By:** Codex

**Actions:**
- Compared README indicators to rendered UI strings.
- Logged mismatch for Idle and Completed.

**Learnings:**
- Documentation drifted during UI changes.

### 2026-02-01 - Approved for Work

**By:** Claude Triage System

**Actions:**
- Issue approved during triage session
- Status changed from pending → ready
- Selected Option 1: Update README to match UI

**Learnings:**
- ASCII-only indicators ensure broad terminal compatibility

## Notes

- Keep indicators ASCII if targeting minimal terminal support.
