# Debussy - A Digital Audio Workstation

## Important Documentation

* [Instructions for Agents](CLAUDE.md)
* [Product Brief](docs/product/README.md)
* [Architectural Overview](docs/architecture/README.md)
* [Architectural Decision Records](docs/adrs/README.md)

## Building and Running

Debussy targets macOS on Apple Silicon. One-time setup per clone:

1. Install Rust with [rustup](https://rustup.rs). The pinned toolchain in `rust-toolchain.toml`
   installs itself on first build. If `cargo` is not found, run `. ~/.cargo/env`.
2. Install the git hooks: `cargo xtask install-hooks`. Every commit then runs
   `cargo xtask precommit`.
3. Make sure `python3` is available. It runs the Claude Code commit guard in `.claude/hooks/`.
4. Optional, matching CI: `cargo install cargo-deny --locked`, then `cargo deny check`.

Then `cargo build` and `cargo test`. CLAUDE.md lists the other commands. To run the application, see
the [user guide](docs/user/README.md); the roadmap is the GitHub issue labelled `wayfinder:map`.
