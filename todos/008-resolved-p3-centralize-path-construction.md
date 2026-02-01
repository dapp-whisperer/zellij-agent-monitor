---
status: pending
priority: p3
issue_id: "008"
tags: [code-review, patterns]
dependencies: []
---

# Centralize Status File Path Construction

## Problem Statement

Status file paths are constructed in 3 different places, risking inconsistency:
- `read_agent_status()` (lines 115-116)
- `delete_status_file()` (lines 173-174)
- `request_status_via_command()` (lines 568-569)

## Proposed Solutions

### Option 1: Create helper function (Recommended)
```rust
fn status_file_paths(agent_id: &str) -> (String, String) {
    (
        format!("{}/{}.status", STATUS_DIR, agent_id),
        format!("{}/{}.status", STATUS_DIR_FALLBACK, agent_id),
    )
}
```

## Acceptance Criteria

- [ ] Single function constructs status file paths
- [ ] All callers use the helper function
- [ ] No duplicate path format strings

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-01-31 | Created | Identified by pattern-recognition-specialist agent |
