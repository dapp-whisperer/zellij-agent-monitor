---
status: pending
priority: p2
issue_id: "003"
tags: [code-review, security]
dependencies: []
---

# Path Traversal Risk in Status File Operations

## Problem Statement

The `read_agent_status` and `delete_status_file` functions construct file paths using `agent_id` without path traversal validation. An `agent_id` containing `../` sequences could read or delete files outside the intended directory.

**Affected locations:**
- `src/lib.rs:114-131` (read_agent_status)
- `src/lib.rs:172-177` (delete_status_file)

## Findings

1. **Security Sentinel Agent**: Identified as MEDIUM severity
2. UUID generation produces safe characters, but no explicit validation
3. WASI sandbox limits scope, but plugin requests `FullHdAccess`

## Proposed Solutions

### Option 1: Add path traversal validation (Recommended)
```rust
fn is_safe_agent_id(id: &str) -> bool {
    !id.contains('/') && !id.contains('\\') && !id.contains("..")
        && id.len() <= 16
        && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}
```
- **Pros**: Explicit validation, defense-in-depth
- **Cons**: Additional function
- **Effort**: Small
- **Risk**: Low

### Option 2: Use a single validation function for all agent_id uses
Combine with command injection fix (#001) into one validation utility.
- **Pros**: Single source of truth
- **Cons**: None
- **Effort**: Small
- **Risk**: Low

## Recommended Action

Option 2 - Create a single `validate_agent_id()` function used by all code paths (file operations, command execution).

## Acceptance Criteria

- [ ] `agent_id` values containing path separators are rejected
- [ ] Validation applied in `read_agent_status`, `delete_status_file`, `request_status_via_command`
- [ ] Single validation function used everywhere

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-01-31 | Created | Identified by security-sentinel agent during code review |
