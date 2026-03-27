# logbox

A dev log black box. Captures dev server logs into SQLite so you can search, compare, and query them later.

Logs get lost in terminal scrollback, disappear on restart, and can't be searched across branches or sessions. logbox fixes that — pipe your dev server through it and every line is stored with timestamps, git branch, and repo info.

## Install

```bash
cargo install logbox
```

## Usage

### Capture logs

Pipe your dev server output through logbox. It stores every line and passes it through to your terminal:

```bash
npm run dev 2>&1 | logbox collect
```

Use `--quiet` to store without terminal output:

```bash
npm run dev 2>&1 | logbox collect --quiet
```

### Search logs

```bash
# Regex search across all logs
logbox search "error|panic"

# Filter by time
logbox search "connection refused" --last 1h

# Filter by branch or session
logbox search "error" --branch main
logbox search "error" --session abc12345

# JSON output
logbox search "error" --json
```

### List sessions

Each time you run `logbox collect`, a new session is created.

```bash
logbox sessions
logbox sessions --branch feature/auth --since 2d
```

### Session stats

```bash
# Latest session
logbox stats

# Specific session
logbox stats <session-id>
```

### Compare sessions

See what changed between two server runs:

```bash
logbox compare <session-a> <session-b>
logbox compare <session-a> <session-b> --pattern "error"
```

## How it works

- Logs are stored in `~/.logbox/logs.db` (SQLite with WAL mode)
- Each `logbox collect` invocation creates a session tagged with the current git branch and repo
- The collector batches writes (every 100 lines or 500ms) for efficiency
- All query commands support `--json` for structured output

## Use with Claude Code

Claude Code can query your logs directly via the CLI:

```bash
# Claude runs this via Bash tool
logbox search "ERROR" --last 1h --json
```

## License

MIT
