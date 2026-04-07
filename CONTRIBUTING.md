# Contributing to logbox

Thanks for your interest in contributing to logbox!

## Development setup

```bash
git clone https://github.com/struct-dot-ai/logbox.git
cd logbox
cargo build
cargo test
```

## Running tests

```bash
# Unit tests + integration tests
cargo test

# With coverage
cargo llvm-cov --all-features
```

## Project structure

```
src/
├── main.rs          # CLI entry point (clap subcommands)
├── db.rs            # SQLite schema, queries
├── collector.rs     # stdin reader → SQLite writer → stdout echo
├── git.rs           # Git branch/repo/sha detection
└── server.rs        # MCP server (rmcp tool definitions)
tests/
└── integration.rs   # End-to-end CLI and MCP tests
```

## Submitting changes

1. Fork the repo and create a branch
2. Make your changes
3. Run `cargo test` and `cargo clippy -- -D warnings`
4. Open a pull request against `main`

Please keep PRs focused — one feature or fix per PR.

## Reporting issues

Open an issue on GitHub with:
- What you expected
- What happened
- Steps to reproduce
