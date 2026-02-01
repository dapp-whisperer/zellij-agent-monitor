---
status: pending
priority: p2
issue_id: "014"
tags: [code-review, security]
dependencies: []
---

# Shell Command Uses String Interpolation Instead of Environment Variables

## Problem Statement

The `request_status_via_command()` function constructs a shell command using string formatting, even though the safer environment variable pattern is used elsewhere (in `write_debug_to_host()`).

**Location**: `src/lib.rs:646-657`

```rust
let command = format!(
    "cat {} 2>/dev/null || cat {} 2>/dev/null",
    fallback_path, primary_path
);
run_command(&["/bin/sh", "-lc", &command], context);
```

Compare to safer pattern in `write_debug_to_host()`:
```rust
let mut env = BTreeMap::new();
env.insert("LOG_PATH".to_string(), log_path.to_string());
run_command_with_env_variables_and_cwd(&["/bin/sh", "-c", "$LOG_PATH"], env, ...);
```

## Findings

1. **Security Sentinel Agent**: Inconsistent with safer pattern already used in codebase
2. While validation prevents exploitation, string interpolation is fragile

## Proposed Solutions

### Option 1: Use environment variables (Recommended)
```rust
fn request_status_via_command(&self, agent_id: &str) {
    if !is_valid_agent_id(agent_id) {
        return;
    }

    let (primary_path, fallback_path) = status_file_paths(agent_id);

    let mut env = BTreeMap::new();
    env.insert("PRIMARY_PATH".to_string(), primary_path);
    env.insert("FALLBACK_PATH".to_string(), fallback_path);

    let mut context = BTreeMap::new();
    context.insert("kind".to_string(), "status_read".to_string());
    context.insert("agent_id".to_string(), agent_id.to_string());

    run_command_with_env_variables_and_cwd(
        &["/bin/sh", "-c", "cat \"$FALLBACK_PATH\" 2>/dev/null || cat \"$PRIMARY_PATH\" 2>/dev/null"],
        env,
        PathBuf::from("."),
        context,
    );
}
```

- **Effort**: Small (15 minutes)
- **Risk**: None

## Recommended Action

Option 1 - Consistent security patterns throughout codebase.

## Acceptance Criteria

- [ ] `request_status_via_command` uses environment variables
- [ ] No direct string interpolation into shell commands
- [ ] Status reading still works correctly

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-02-01 | Created | Identified by security-sentinel agent |

## Resources

- Security review findings
- Existing pattern in `write_debug_to_host()`
