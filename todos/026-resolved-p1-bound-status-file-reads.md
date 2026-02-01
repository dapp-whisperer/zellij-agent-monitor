---
status: ready
priority: p1
issue_id: "026"
tags: [code-review, security, reliability]
dependencies: []
---

# Bound status file reads to avoid DoS

## Problem Statement

Status files are read from world-writable temp directories without size limits. A large file can cause memory pressure or crashes in the plugin and Zellij.

## Findings

- `src/lib.rs:131-150` uses `std::fs::read_to_string` on `/tmp/...` and `/private/tmp/...` without size bounds.
- `src/lib.rs:808-813` shells out to `cat` which can stream arbitrarily large output into memory.
- These paths are world-writable, so a malicious or accidental oversized file can trigger unbounded reads.

## Proposed Solutions

### Option 1: Bounded Rust reads (Recommended)

**Approach:** Replace `read_to_string` with a bounded read (open file, read up to N bytes). If larger, treat as unrecognized and skip.

**Pros:**
- Avoids shell spawning
- Strongest control over memory use

**Cons:**
- Slightly more code

**Effort:** 2-3 hours

**Risk:** Low

---

### Option 2: Bound shell output

**Approach:** Keep `run_command_with_env_variables_and_cwd` but pipe through `head -c N` (or equivalent) to cap output.

**Pros:**
- Smaller diff

**Cons:**
- Still uses shell
- Harder to ensure portability

**Effort:** 1-2 hours

**Risk:** Medium

## Recommended Action

Option 1: Bounded Rust reads. Replace `read_to_string` with bounded read to cap memory usage.

## Technical Details

**Affected files:**
- `src/lib.rs:131`
- `src/lib.rs:808`

**Related components:**
- Status parsing (`parse_status_content`)

**Database changes (if any):**
- No

## Resources

- Review notes from security-sentinel

## Acceptance Criteria

- [ ] Status file reads are capped to a fixed maximum size
- [ ] Oversized files are handled gracefully (no crash)
- [ ] Tests updated or added for oversized input handling

## Work Log

### 2026-02-01 - Initial Discovery

**By:** Claude Code

**Actions:**
- Identified unbounded reads in `read_agent_status`
- Noted shell-based `cat` fallback also unbounded
- Drafted remediation options

**Learnings:**
- Temp files are untrusted inputs in this design

### 2026-02-01 - Approved for Work

**By:** Claude Triage System

**Actions:**
- Issue approved during triage session
- Status changed from pending → ready
- Ready to be picked up and worked on

**Learnings:**
- Critical security issue - unbounded reads from untrusted files

## Notes

- Keep MAX_CWD_LEN consistent with read cap if you choose Option 1
