# logbox

Capture dev server logs so coding agents can search them directly.

When your agent asks "what's in the logs?", you end up scrolling through your terminal, copying log lines, and pasting them into the chat.

Use logbox to pipe your dev server logs through it and query them directly via MCP. No more copy-pasting.

Previous dev sessions can also be searched, tagged with commit sha and branch.

## Quickstart

### 1. Pipe your dev server through logbox

```bash
npx @struct-ai/logbox collect
```

Add it to your dev scripts:

```json
{
  "scripts": {
    "dev": "npm run dev 2>&1 | npx @struct-ai/logbox collect"
  }
}
```

Logs pass through to your terminal as normal, but are also saved to `~/.logbox/logs.db`.

### 2. Connect your coding agent

**Claude Code:**

```bash
claude mcp add logbox -- npx @struct-ai/logbox serve
```

**Cursor:**

Add to your MCP config:

```json
{
  "mcpServers": {
    "logbox": {
      "command": "npx",
      "args": ["@struct-ai/logbox", "serve"]
    }
  }
}
```

### 3. Ask your agent to check the logs

```
❯ The test request failed, check the logs.
```


## Install

No install required — `npx @struct-ai/logbox` works out of the box.

Or, install globally:

```bash
npm install -g @struct-ai/logbox
```

Or with cargo:

```bash
cargo install logbox
```

And then invoke with `logbox`:
```bash
logbox collect
```

## CLI reference

### Capture logs

```bash
# Pipe dev server output (logs pass through to terminal)
npm run dev 2>&1 | logbox collect

# Silent mode (store only, no terminal output)
npm run dev 2>&1 | logbox collect --quiet
```

### Browse logs

View recent log lines, newest first:

```bash
logbox logs
logbox logs --since 1h
logbox logs --since 2h --until 1h
logbox logs --session <session-id> --limit 100
logbox logs --offset 50          # paginate
```

### Search logs

Find log lines matching a keyword (case-insensitive):

```bash
logbox search "error"
logbox search "connection refused" --last 1h
logbox search "404" --branch main
```

**Tip:** Use `search` to find a specific event, then `logs --since/--until` around that timestamp to see surrounding context.

### Sessions

Each `logbox collect` invocation creates a session tagged with git repo, branch, and commit SHA:

```bash
logbox sessions
logbox sessions --branch feature/auth --since 2d
logbox sessions --commit e30d239
```

### Session stats

```bash
logbox stats                     # latest session
logbox stats <session-id>
```

### MCP server

```bash
logbox serve                     # start MCP server on stdio
```

Tools:

- **list_logs** — browse consecutive log lines in a time range, or from the end
- **search_logs** — find lines matching a keyword
- **list_sessions** — list recorded dev server runs
- **session_stats** — get summary stats for a session


## How it works

- Logs are stored in `~/.logbox/logs.db` (SQLite with WAL mode)
- Each `logbox collect` creates a session tagged with git repo, branch, and commit SHA
- The collector batches writes (every 100 lines or 500ms) for efficiency
- All query commands support `--json` for structured output

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, testing, and PR guidelines.

## License

MIT
