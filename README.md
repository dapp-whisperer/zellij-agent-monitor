# Zellij Agent Monitor

A Zellij plugin to spawn, monitor, and switch between Claude Code agents.

## Features

- **Spawn agents** - Press `n` to create a new Claude agent as a floating pane
- **Monitor status** - See agent states: Idle, Working, NeedsInput, Completed, Failed
- **Switch focus** - Press Enter to focus the selected agent's pane
- **Kill agents** - Press `x` to terminate an agent

## Status Indicators

| Status | Indicator | Description |
|--------|-----------|-------------|
| Idle | `[.]` | Waiting for user input |
| Working | `[-\|/]` | Tool in progress (ASCII spinner) |
| NeedsInput | `[?]` | Waiting for approval |
| Unread | `[!]` | Completed but not viewed |
| Completed | `[v]` | Exited successfully |
| Failed | `[X]` | Exited with error |

**Note:** Unread status indicates the agent finished a task but you haven't viewed the output yet. When you focus the agent's pane, status transitions from Unread to Idle.

## Installation

```bash
# Build the plugin
cargo build --release --target wasm32-wasip1

# Copy to Zellij plugins directory
cp target/wasm32-wasip1/release/agent_monitor.wasm ~/.config/zellij/plugins/
```

Add to your Zellij config (`~/.config/zellij/config.kdl`):

```kdl
keybinds {
    normal {
        bind "Alt m" {
            LaunchOrFocusPlugin "file:~/.config/zellij/plugins/agent_monitor.wasm" {
                floating true
            }
        }
    }
}
```

## Status Detection Setup

For accurate status detection (Idle vs Working vs NeedsInput), configure Claude Code hooks.

**Important:** The plugin sets `AGENT_MONITOR_ID` when spawning agents. Hooks must use this variable for status file naming, with fallbacks for standalone usage.

Add to `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "mkdir -p /tmp/agent-monitor && F=/tmp/agent-monitor/${AGENT_MONITOR_ID:-${ZELLIJ_PANE_ID:-$$}}.status && echo \"W:PreToolUse:$(pwd)\" > \"$F.tmp\" && mv \"$F.tmp\" \"$F\"",
            "timeout": 5000
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "F=/tmp/agent-monitor/${AGENT_MONITOR_ID:-${ZELLIJ_PANE_ID:-$$}}.status && echo \"I:PostToolUse:$(pwd)\" > \"$F.tmp\" && mv \"$F.tmp\" \"$F\"",
            "timeout": 5000
          }
        ]
      }
    ],
    "Stop": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "F=/tmp/agent-monitor/${AGENT_MONITOR_ID:-${ZELLIJ_PANE_ID:-$$}}.status && echo \"I:Stop:$(pwd)\" > \"$F.tmp\" && mv \"$F.tmp\" \"$F\"",
            "timeout": 5000
          }
        ]
      }
    ],
    "Notification": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "F=/tmp/agent-monitor/${AGENT_MONITOR_ID:-${ZELLIJ_PANE_ID:-$$}}.status && echo \"?:Notification:$(pwd)\" > \"$F.tmp\" && mv \"$F.tmp\" \"$F\"",
            "timeout": 5000
          }
        ]
      }
    ]
  }
}
```

**Variable priority:** `AGENT_MONITOR_ID` (set by plugin) > `ZELLIJ_PANE_ID` (Zellij default) > `$$` (shell PID fallback)

**Without hooks configured:** Agents will stay in `Idle` state. This is safe - no crashes, just less accurate status.

## Keybindings

| Key | Action |
|-----|--------|
| `n` | Spawn new agent |
| `j` / `↓` | Select next agent |
| `k` / `↑` | Select previous agent |
| `Enter` | Focus selected agent |
| `x` | Kill selected agent |
| `q` / `Esc` | Hide monitor panel |

## Troubleshooting

### Status always shows Idle

Check that hooks are configured in `~/.claude/settings.json` and that `/tmp/agent-monitor/` exists and is writable.

### Status files not updating

When spawned by the plugin, verify `AGENT_MONITOR_ID` is set. The plugin automatically sets this variable. For standalone usage, check that `ZELLIJ_PANE_ID` is set in your shell by running `echo $ZELLIJ_PANE_ID` in a Zellij pane.

## How It Works

1. Plugin spawns Claude as a floating pane with `AGENT_MONITOR_ID` set to a unique UUID
2. Claude Code hooks write status to `/tmp/agent-monitor/<agent_id>.status`
3. Plugin polls status files every 100-500ms (adaptive)
4. Status file is deleted when pane closes

### Status File Format

The status file supports multiple formats for flexibility:

| Format | Example | Description |
|--------|---------|-------------|
| New format | `W:PreToolUse:/home/user/project` | Status + event name + working directory |
| Current format | `W:/home/user/project` | Status + working directory (backward compatible) |
| Legacy format | `W` | Status only (backward compatible) |

**Status characters:** `W` (Working), `I` (Idle), `?` (NeedsInput), `U` (Unread)

**Event names:** `PreToolUse`, `PostToolUse`, `Stop`, `Notification` (or any custom event)

## License

MIT
