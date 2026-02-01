---
status: pending
priority: p3
issue_id: "007"
tags: [code-review, simplicity]
dependencies: []
---

# Remove Dead Code and Unused Fields

## Problem Statement

Multiple `#[allow(dead_code)]` annotations indicate leftover code from refactoring that should be removed.

## Findings

1. **Simplicity Reviewer**: Identified ~45-50 lines of removable code
2. Dead code creates maintenance burden and confusion

**Dead code locations:**
- `DEBUG_LOG_PATHS` constant (lines 83-93) - 11 lines
- `raw` field in `StatusRead::Parsed` (line 100-101)
- `raw` field in `StatusRead::Unrecognized` (line 104-105)
- `ReadError` variant (line 108-109)
- `debug_last` field (line 53) - written but never read
- `_color_idx` variable in render (line 386) - computed but unused

## Proposed Solutions

### Option 1: Remove all dead code (Recommended)
- Delete `DEBUG_LOG_PATHS`
- Remove `raw` fields from `StatusRead` variants
- Remove `debug_last` field from `State`
- Remove `_color_idx` computation

- **Effort**: Small
- **Risk**: None

## Acceptance Criteria

- [ ] No `#[allow(dead_code)]` annotations remain
- [ ] All remaining code is actively used
- [ ] Code compiles successfully

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-01-31 | Created | Identified by code-simplicity-reviewer agent |
