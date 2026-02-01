---
status: ready
priority: p2
issue_id: "028"
tags: [code-review, security, architecture]
dependencies: []
---

# Reduce plugin permission scope

## Problem Statement

The plugin requests broad permissions that may exceed what is required for status monitoring. This increases the impact of potential vulnerabilities.

## Findings

- `src/lib.rs:78-86` requests `FullHdAccess`, `OpenFiles`, `RunCommands`, and `OpenTerminalsOrPlugins`.
- Current functionality appears to need `RunCommands` and `OpenTerminalsOrPlugins`, but `FullHdAccess` and `OpenFiles` may be avoidable depending on status file access strategy.

## Proposed Solutions

### Option 1: Apply least-privilege audit (Recommended)

**Approach:** Identify which permissions are strictly required for current behavior; remove or conditionally request the rest.

**Pros:**
- Smaller attack surface
- Clearer security posture

**Cons:**
- Requires validation with Zellij permission model

**Effort:** 2-4 hours

**Risk:** Low

---

### Option 2: Gate permissions by feature flag

**Approach:** Request higher permissions only when debug or fallback paths are enabled.

**Pros:**
- Flexible for dev vs prod

**Cons:**
- Adds configuration complexity

**Effort:** 3-5 hours

**Risk:** Medium

## Recommended Action

Option 1: Apply least-privilege audit. Identify strictly required permissions and remove the rest.

## Technical Details

**Affected files:**
- `src/lib.rs:78`

**Related components:**
- Permission request flow

**Database changes (if any):**
- No

## Resources

- Review notes from security-sentinel

## Acceptance Criteria

- [ ] Permissions are reduced to the minimum required
- [ ] Plugin still functions as expected with reduced permissions
- [ ] Documentation updated if permissions change

## Work Log

### 2026-02-01 - Initial Discovery

**By:** Claude Code

**Actions:**
- Reviewed requested permissions
- Noted potential excess permissions
- Drafted least-privilege options

**Learnings:**
- Security scope is larger than required for basic status polling

### 2026-02-01 - Approved for Work

**By:** Claude Triage System

**Actions:**
- Issue approved during triage session
- Status changed from pending → ready
- Selected Option 1: Least-privilege audit

**Learnings:**
- Minimizing permissions reduces blast radius of vulnerabilities

## Notes

- Revisit after deciding on bounded file reads and shell usage
