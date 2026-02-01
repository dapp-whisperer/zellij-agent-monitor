---
status: ready
priority: p3
issue_id: "021"
tags: [code-review, documentation, unread-indicator]
dependencies: []
---

# Document U: (Unread) Status Format

## Problem Statement

The `U:` (Unread) status character is implemented in the plugin but not documented in the README or elsewhere. The README only documents `W`, `I`, and `?` status characters. This creates a documentation-implementation gap.

## Findings

**From code-simplicity-reviewer:**
- The Unread state is parsed at line 167: `Some("U") => AgentStatus::Unread`
- Tests exist at lines 1010-1045 covering `U:` parsing
- README does not mention `U:` in the status format documentation

**From agent-native-reviewer:**
- Status format is only discoverable by reading source code or tests
- No external documentation file exists
- Format lacks versioning for changes

## Proposed Solutions

### Option A: Update README (Recommended)
Add `U:` to the status character documentation in README.md.

**Pros:** Simple, keeps docs centralized
**Cons:** None
**Effort:** Small (10 min)
**Risk:** None

### Option B: Create PROTOCOL.md
Create a separate `/tmp/agent-monitor/PROTOCOL.md` that agents can read.

**Pros:** Discoverable by agents at runtime
**Cons:** More files to maintain
**Effort:** Medium (30 min)
**Risk:** Low

## Recommended Action

Option A - Update README with U: documentation.

## Technical Details

**Affected files:**
- `README.md`

**Format to document:**
```
STATUS_CHAR[:EVENT_NAME][:CWD_PATH]

STATUS_CHAR:
- W = Working (tool in progress)
- I = Idle (waiting for input)
- ? = NeedsInput (waiting for approval)
- U = Unread (completed but not viewed)
```

## Acceptance Criteria

- [ ] README documents all 4 status characters (W, I, ?, U)
- [ ] Examples show U: format usage
- [ ] Behavior of Unread->Idle transition is explained

## Work Log

| Date | Action | Result |
|------|--------|--------|
| 2026-02-01 | Created from code review | - |

## Resources

- Code review findings from agent-native-reviewer and code-simplicity-reviewer
- Source: `src/lib.rs:167` (U: parsing)
- Tests: `src/lib.rs:1010-1045`

### 2026-02-01 - Approved for Work

**By:** Claude Triage System

**Actions:**
- Issue approved during triage session
- Status changed from pending → ready
- Selected Option A: Update README

**Learnings:**
- Documentation should stay in sync with implementation
