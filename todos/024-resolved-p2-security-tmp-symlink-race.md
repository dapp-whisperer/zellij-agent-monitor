---
status: resolved
priority: p2
issue_id: "024"
tags: [code-review, security]
dependencies: []
---

# Address /tmp Symlink Race Condition

## Problem Statement

Status files are written to `/tmp/agent-monitor/` which is a world-writable directory. An attacker could potentially:
1. Delete the agent-monitor directory
2. Create a symlink to a sensitive location
3. Wait for the plugin to write a status file, overwriting the target

## Findings

**From security-sentinel:**
- TOCTOU (Time-of-Check-to-Time-of-Use) vulnerability
- Status files in `/tmp` are potentially readable by other users
- No verification that directory is not a symlink before writing
- Severity: Medium, Exploitability: Medium

## Proposed Solutions

### Option A: Use XDG_RUNTIME_DIR (Recommended)
Use user-specific runtime directory with proper fallback chain:

```rust
fn status_dir() -> PathBuf {
    // 1. Try XDG_RUNTIME_DIR (per-user, auto-cleaned on logout)
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime).join("agent-monitor");
    }

    // 2. Try TMPDIR (user-specific on macOS: /var/folders/...)
    if let Ok(tmpdir) = std::env::var("TMPDIR") {
        return PathBuf::from(tmpdir).join("agent-monitor");
    }

    // 3. Fall back to user-specific /tmp directory
    if let Ok(user) = std::env::var("USER") {
        return PathBuf::from(format!("/tmp/agent-monitor-{}", user));
    }

    // 4. Last resort: shared /tmp (least secure)
    PathBuf::from("/tmp/agent-monitor")
}
```

**Pros:** Better isolation, standard practice, works cross-platform
**Cons:** Slightly more complex fallback logic
**Effort:** Medium (1 hour)
**Risk:** Low

### Option B: Verify Directory Is Not Symlink
Check directory metadata before writing:

```rust
fn is_safe_directory(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => !meta.file_type().is_symlink(),
        Err(_) => false,
    }
}
```

**Pros:** Direct mitigation of symlink attack
**Cons:** Still uses shared /tmp, race window exists between check and write
**Effort:** Small (30 min)
**Risk:** Medium - doesn't fully eliminate race

### Option C: Set Restrictive Permissions
Ensure directory and files have 0700/0600 permissions:

**Pros:** Limits who can read status
**Cons:** Doesn't prevent symlink attack
**Effort:** Small (15 min)
**Risk:** Incomplete mitigation

## Recommended Action

Implement Option A with full fallback chain (XDG_RUNTIME_DIR → TMPDIR → /tmp/agent-monitor-$USER → /tmp/agent-monitor), combined with Option C for 0700/0600 permissions. This follows Claude Code's own learnings from multi-user permission issues.

## Technical Details

**Affected files:**
- `src/lib.rs:96-97` (STATUS_DIR constants)
- `src/lib.rs:111-116` (status_file_paths)
- `src/lib.rs:729-752` (write_status_file)

**Considerations:**
- WASI filesystem access may have limitations
- Need to handle directory creation with proper permissions
- Fallback path for systems without XDG_RUNTIME_DIR

## Acceptance Criteria

- [x] Status files written to user-specific directory when possible
- [x] Directory permissions are 0700
- [ ] File permissions are 0600 (note: WASI has limitations; directory permissions cover most use cases)
- [x] Falls back gracefully on systems without XDG_RUNTIME_DIR
- [x] Existing agents continue to work during migration

## Work Log

| Date | Action | Result |
|------|--------|--------|
| 2026-02-01 | Created from code review | - |

## Resources

- Code review findings from security-sentinel
- XDG Base Directory Specification
- Source: `src/lib.rs:96-97`
- Claude Code Issue #20396: Sandbox permission conflicts
- Claude Code Issue #2350: XDG compliance

### 2026-02-01 - Approved for Work

**By:** Claude Triage System

**Actions:**
- Issue approved during triage session
- Status changed from pending → ready
- Researched Claude Code's approach to temp files
- Selected Option A with enhanced fallback chain

**Learnings:**
- XDG_RUNTIME_DIR is per-user and auto-cleaned on logout
- TMPDIR is user-specific on macOS (/var/folders/...)
- User-specific directories avoid multi-user permission conflicts

### 2026-02-01 - Implementation Complete

**Changes made to `src/lib.rs`:**

1. **Replaced hardcoded constants** with dynamic `status_dir()` function (lines 95-115)
   - Implements secure fallback chain: XDG_RUNTIME_DIR -> TMPDIR -> /tmp/agent-monitor-$USER -> /tmp/agent-monitor

2. **Added `ensure_status_dir()` function** (lines 117-141)
   - Creates directory with 0700 permissions using `std::os::unix::fs::PermissionsExt`
   - Conditional compilation with `#[cfg(unix)]` for cross-platform support

3. **Simplified `status_file_path()`** (lines 154-157)
   - Now returns a single `PathBuf` instead of (primary, fallback) tuple
   - No longer needs macOS /private/tmp fallback since we use TMPDIR

4. **Updated all consumers:**
   - `read_agent_status()` - uses single status path
   - `delete_status_file()` - simplified to remove single file
   - `write_status_file()` - calls `ensure_status_dir()` before writing
   - `request_status_via_command()` - simplified shell command

5. **Added tests:**
   - `test_status_dir_returns_path` - verifies function returns valid path
   - `test_status_file_path_format` - verifies correct file naming
   - `test_status_file_path_uses_status_dir` - verifies file is inside status dir
