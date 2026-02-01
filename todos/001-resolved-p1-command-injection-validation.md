---
status: pending
priority: p1
issue_id: "001"
tags: [code-review, security]
dependencies: []
---

# Command Injection Risk in request_status_via_command

## Problem Statement

The `request_status_via_command` function at `src/lib.rs:564-579` constructs a shell command by directly interpolating `agent_id` into a command string without explicit validation.

```rust
let command = format!(
    "cat /private/tmp/agent-monitor/{agent_id}.status 2>/dev/null || cat /tmp/agent-monitor/{agent_id}.status 2>/dev/null"
);
run_command(&["/bin/sh", "-lc", &command], context);
```

While `agent_id` is generated internally via UUID (`Uuid::new_v4().to_string()[..8]`), this implicit safety could be bypassed if the source changes or event context is manipulated.

## Findings

1. **Security Sentinel Agent**: Identified as HIGH severity - direct shell interpolation without sanitization
2. **Pattern Recognition Agent**: Noted as architectural concern - inconsistent with safer pattern used in `write_debug_to_host()`
3. The safer pattern (environment variables) is already used elsewhere in the codebase

## Proposed Solutions

### Option 1: Add explicit validation (Recommended)
```rust
fn is_valid_agent_id(id: &str) -> bool {
    id.len() <= 16 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn request_status_via_command(&self, agent_id: &str) {
    if !is_valid_agent_id(agent_id) {
        return; // Silently reject invalid IDs
    }
    // ... rest unchanged
}
```
- **Pros**: Minimal change, explicit defense-in-depth
- **Cons**: Slightly more code
- **Effort**: Small
- **Risk**: Low

### Option 2: Use environment variables like write_debug_to_host
```rust
fn request_status_via_command(&self, agent_id: &str) {
    let mut env = BTreeMap::new();
    env.insert("AGENT_ID".to_string(), agent_id.to_string());
    run_command_with_env_variables_and_cwd(
        &["/bin/sh", "-c",
          "cat /private/tmp/agent-monitor/$AGENT_ID.status 2>/dev/null || cat /tmp/agent-monitor/$AGENT_ID.status 2>/dev/null"],
        env,
        PathBuf::from("."),
        context,
    );
}
```
- **Pros**: Consistent with existing pattern, eliminates injection entirely
- **Cons**: Slightly more verbose
- **Effort**: Small
- **Risk**: Low

### Option 3: Both validation AND environment variables
- **Pros**: Maximum defense-in-depth
- **Cons**: Overkill for current threat model
- **Effort**: Small
- **Risk**: Low

## Recommended Action

Option 2 - Refactor to use environment variables. This matches the pattern already used in `write_debug_to_host()` and completely eliminates the injection vector.

## Technical Details

**Affected files:**
- `src/lib.rs:564-579` (request_status_via_command)

**Related code:**
- `src/lib.rs:456-477` (write_debug_to_host - correct pattern)

## Acceptance Criteria

- [ ] `request_status_via_command` uses environment variables instead of string interpolation
- [ ] No shell metacharacters can reach command execution
- [ ] Existing functionality preserved (status reading still works)

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-01-31 | Created | Identified by security-sentinel agent during code review |

## Resources

- PR: Current branch `feat/status-detection-hooks`
- Security review: Shell injection OWASP guidelines
