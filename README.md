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
| Idle | `[·]` | Waiting for user input |
| Working | `[⠋]` | Tool in progress (spinner) |
| NeedsInput | `[?]` | Waiting for approval |
| Completed | `[✓]` | Exited successfully |
| Failed | `[X]` | Exited with error |

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
            "command": "mkdir -p /tmp/agent-monitor && F=/tmp/agent-monitor/${ZELLIJ_PANE_ID:-$$}.status && echo W > \"$F.tmp\" && mv \"$F.tmp\" \"$F\"",
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
            "command": "F=/tmp/agent-monitor/${ZELLIJ_PANE_ID:-$$}.status && echo I > \"$F.tmp\" && mv \"$F.tmp\" \"$F\"",
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
            "command": "F=/tmp/agent-monitor/${ZELLIJ_PANE_ID:-$$}.status && echo I > \"$F.tmp\" && mv \"$F.tmp\" \"$F\"",
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
            "command": "F=/tmp/agent-monitor/${ZELLIJ_PANE_ID:-$$}.status && echo '?' > \"$F.tmp\" && mv \"$F.tmp\" \"$F\"",
            "timeout": 5000
          }
        ]
      }
    ]
  }
}
```

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

Verify `ZELLIJ_PANE_ID` is set in your shell. Run `echo $ZELLIJ_PANE_ID` in a Zellij pane - it should show a number.

## How It Works

1. Plugin spawns Claude as a floating pane
2. Claude Code hooks write status to `/tmp/agent-monitor/<pane_id>.status`
3. Plugin polls status files every 100-500ms (adaptive)
4. Status file is deleted when pane closes

## License

MIT
