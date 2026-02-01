---
status: pending
priority: p2
issue_id: "006"
tags: [code-review, architecture]
dependencies: []
---

# Extract Event Handlers from Monolithic update() Method

## Problem Statement

The `update()` method at `src/lib.rs:216-372` is 150+ lines handling 8 different event types inline. This violates single responsibility principle and makes the code harder to maintain.

## Findings

1. **Architecture Strategist Agent**: Identified as maintainability concern
2. Each event handler has distinct logic that could be isolated
3. Current structure makes it hard to understand individual event handling

## Proposed Solutions

### Option 1: Extract to separate methods (Recommended)
```rust
fn update(&mut self, event: Event) -> bool {
    match event {
        Event::PermissionRequestResult(r) => self.handle_permission_result(r),
        Event::CommandPaneOpened(id, ctx) => self.handle_pane_opened(id, ctx),
        Event::CommandPaneExited(id, code, ctx) => self.handle_pane_exited(id, code, ctx),
        Event::RunCommandResult(exit, out, err, ctx) => self.handle_command_result(exit, out, err, ctx),
        Event::PaneClosed(id) => self.handle_pane_closed(id),
        Event::Timer(_) => self.handle_timer(),
        Event::PaneUpdate(manifest) => self.handle_pane_update(manifest),
        Event::Key(key) => self.handle_key(key),
        _ => false,
    }
}

fn handle_timer(&mut self) -> bool { ... }
fn handle_pane_update(&mut self, manifest: PaneManifest) -> bool { ... }
// etc.
```
- **Pros**: Clear separation, easier to understand and test
- **Cons**: More methods
- **Effort**: Medium
- **Risk**: Low

## Recommended Action

Option 1 - Extract event handlers to separate methods.

## Acceptance Criteria

- [ ] Each event type has its own handler method
- [ ] `update()` is a simple match dispatch
- [ ] No logic changes, only refactoring
- [ ] All tests pass

## Work Log

| Date | Action | Notes |
|------|--------|-------|
| 2026-01-31 | Created | Identified by architecture-strategist agent |
