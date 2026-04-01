# logbox

Capture dev server logs to SQLite so AI coding agents can search and query them.

Dev server logs get lost in terminal scrollback, disappear on restart, and can't be searched across branches or sessions. logbox fixes that — pipe your dev server through it and every line is stored with timestamps, git branch, commit SHA, and repo info. Agents can then query the logs via MCP tools or CLI.

## Install

```bash
npm install -g @struct-ai/logbox
```

Or with cargo:

```bash
cargo install logbox
```

## Capture logs

Pipe your dev server output through logbox. It stores every line and passes it through to your terminal:

```bash
npm run dev 2>&1 | logbox collect
```

Use `--quiet` to store without terminal output:

```bash
npm run dev 2>&1 | logbox collect --quiet
```

## Browse logs

View recent log lines, newest first. Use time ranges to focus on a window:

```bash
logbox logs
logbox logs --since 1h
logbox logs --since 2h --until 1h
logbox logs --session <session-id> --limit 100
logbox logs --offset 50          # paginate
```

## Search logs

Find log lines matching a keyword (case-insensitive):

```bash
logbox search "error"
logbox search "connection refused" --last 1h
logbox search "404" --branch main
logbox search "error" --session <session-id>
```

**Tip:** Use `search` to find a specific event, then `logs --since/--until` around that timestamp to see context.

## Sessions

Each `logbox collect` invocation creates a session tagged with git repo, branch, and commit SHA:

```bash
logbox sessions
logbox sessions --branch feature/auth --since 2d
logbox sessions --commit e30d239
```

## Session stats

```bash
logbox stats                     # latest session
logbox stats <session-id>
```

## MCP server

logbox includes an MCP server so AI coding agents automatically discover the log tools:

```bash
# Add to Claude Code
claude mcp add logbox -- logbox serve
```

Or add to your Claude Code settings manually:

```json
{
  "mcpServers": {
    "logbox": {
      "command": "logbox",
      "args": ["serve"]
    }
  }
}
```

This exposes 4 tools to the agent:
- **list_logs** — browse consecutive log lines in a time range
- **search_logs** — find lines matching a keyword
- **list_sessions** — list recorded sessions
- **session_stats** — get stats for a session

## How it works

- Logs are stored in `~/.logbox/logs.db` (SQLite with WAL mode)
- Each `logbox collect` invocation creates a session tagged with git repo, branch, and commit SHA
- The collector batches writes (every 100 lines or 500ms) for efficiency
- All query commands support `--json` for structured output

## License

MIT
