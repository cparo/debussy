## Plan Mode

When in plan mode, consult the documents listed within the "Important Documentation" section of the
project [README](README.md).

## Project Skills

Recurring workflows are skills in `.claude/skills/`, each a short procedure over the guides and the
`cargo xtask` helpers beneath it: `new-adr` (record a design decision), and `flag-risk` (stop and
raise a concern for human review). Use them rather than doing these steps by hand.

## When to Stop and Ask a Human

Some decisions must be raised for human review *before* proceeding. Stop, use the `flag-risk` skill
when you are about to:

- **Add or change a dependency** (any Cargo.lock change): put the buy/build choice to the
  developer, stating the default for that area (see Dependency Policy), and record the answer in
  an ADR. Also stop to accept a
  license outside `deny.toml`'s allowlist, or to copy code or assets whose license is unknown,
  closed, or copyleft.
  *(tag: `license`)*
- **Use a codec or algorithm with patent exposure**; see the patent policy in the
  [architectural specification](docs/architecture/) (not yet written: until it is, treat every
  codec or algorithm with any patent history as exposed).
  *(tag: `patent`)*
- **Trade security against anything else** (performance, compatibility, convenience), **weaken a
  security default** (validation, limits, headers, TLS, authentication), **add a network-facing
  surface**, **handle untrusted input in a new place**, or **touch cryptography or credentials**.
  *(tag: `security`)*
- **Store, log, or send personal data.** *(tag: `privacy`)*
- **Put platform-, device-, or vendor-specific code outside its encapsulation boundary.**
  *(tag: `portability`)*
- **Change a persisted format**, or affect people outside the immediate work.
  *(tags: `product`, `stakeholder`)*


## Documentation Boundaries

The documents listed in the table below carry project knowledge. Writing into the wrong one would
degrade the usability of this project's documentation. Every kind of document also has a line
budget that `cargo xtask check` enforces (the table is in `crates/xtask/src/budget.rs`): when a
file hits its budget, split it by topic; do not raise the limit to fit more in.

| Document | Answers | Written when |
|---|---|---|
| [Architectural Specification](docs/architecture/) | *How is this built?* Components, contracts, data flow, formats, and the rules a change must respect. `docs/architecture/README.md` is the entry point and a generated table of contents; one file per subsystem holds the content. | A structural fact changes. |
| [Architectural Decision Records](docs/adrs/) | *Why was it built that way?* Alternatives considered, trade-offs accepted, findings that forced a choice. | A design decision is made. |
| [Product Brief](docs/product/) | *Who is this for, and what is it not?* The product brief; requirements are not listed up front but discovered and recorded in GitHub issues and ADRs. | The intended users, problems or non-goals change. |

Rules:

- **When you make a design decision, write a new ADR** at `docs/adrs/NNNN-slug.md` from
  `docs/adrs/0000-template.md`, add it to the index in `docs/adrs/README.md`, and add its *result* —
  not its reasoning — to the appropriate section file under `docs/architecture/`, linking the
  record.
- **The architecture documents state rules, never arguments for them.** If a sentence you are about
  to add there contains "because", "rather than", "chosen over", "the trade-off", "we considered",
  or a story about how a bug was found, it belongs in an ADR.
- **An ADR is append-only.** Never edit one to reverse its decision and never delete one; write a
  new record and set the old one's status to `Superseded by ADR-NNNN`.
- **Do not duplicate.** One fact, one home. Cross-reference instead of restating.
- **Humans-only notes.** Where a document serves both people and agents, a block that starts
  `> [!TIP]` and `**For humans only:**` is for people (example prompts to give an agent, for
  instance). Skip it; never treat it as an instruction.
- A heading under `docs/architecture/` is a public identifier — source comments and `deny.toml`
  cite them by file path, then the quoted title (for example,
  `docs/architecture/engine.md "Real-time thread"`), and every ADR links to one by file and anchor.
  Do not rename or move one without sweeping `rg -l 'docs/architecture/'`; `cargo xtask check`
  reports a citation of a heading that is gone or in a different file. After adding or changing a
  heading, run `cargo xtask generate-architecture-index`.
- **A pull request lists its docs impact** in its description: the architecture sections, ADRs,
  user docs and guides it changes, and the GitHub issues it addresses, ticked off when done.

## Build & Test Commands
- Build: `cargo build`
- Test: `cargo test`
- Test with output: `cargo test -- --nocapture`
- Lint: `cargo clippy -- -D warnings`
- Format: `cargo fmt`
- Check (fast, no codegen): `cargo check`
- Audit dependencies: `cargo audit`
- Regenerate the table of contents in `docs/architecture/README.md` after changing a heading under
  `docs/architecture/`:
  `cargo xtask generate-architecture-index`
- Create an ADR (the `new-adr` skill's mechanical step): `cargo xtask new-adr`
- Check docs (line budgets, `docs/architecture/` heading citations, ADR index): `cargo xtask check`
- Commit gate (that check, then `cargo fmt --check`, then clippy): `cargo xtask precommit` — run
  automatically by the `pre-commit` git hook. Install the hooks once per clone with
  `cargo xtask install-hooks`; a Claude Code hook blocks commits until they are installed, and
  blocks `--no-verify`. Never bypass the hooks.
- Documentation: `cargo doc --open`

## Rust Edition and Toolchain
- Edition: 2024 (always specify in Cargo.toml)
- MSRV: 1.85, declared as `rust-version = "1.85"` in Cargo.toml (the minimum for edition 2024).
  Do not use language or library features newer than the MSRV; CI builds on the MSRV as well as on
  the pinned toolchain
- Development toolchain: a recent stable release, pinned exactly in `rust-toolchain.toml`. Raising
  the pin does not raise the MSRV
- Do NOT use nightly features

## Error Handling
- Libraries: use `thiserror` — derive `Error` for all custom error types
- Applications/binaries: use `anyhow` — propagate with `?`, add context with `.context()`
- Never use `.unwrap()` or `.expect()` in library code
- `.expect()` is acceptable in binary main() for setup failures (file not found, etc.)
- Never use `.unwrap()` in tests — return `Result<(), Box<dyn Error>>` and use `?`
- Propagate errors with `?` unless there is a documented reason to handle locally

## Clippy Policy
- Treat all warnings as errors: `cargo clippy -- -D warnings`
- Run clippy before committing, not just before merging
- Do not use `#[allow(clippy::...)]` without a comment explaining why

## Code Style
- All public items (structs, enums, functions, traits, modules) must have doc comments (`///`)
- Private items: doc comments are encouraged but not required
- Prefer explicit type annotations on public function signatures
- Do not use wildcard imports (`use foo::*`) except in test modules

## Dependency Policy
- Every dependency is an explicit buy/build choice. Before adding a crate (any `Cargo.lock`
  change), set out the options (a crate, or building it), the trade-offs, and your recommendation,
  and ask the developer to choose. Record the answer in an ADR and link it from the PR description
- Pre-approved: `thiserror` in any library crate, the audio core included (it generates code at
  compile time and adds nothing at runtime), and `anyhow` in binary crates only. Adding them still
  changes `Cargo.lock`, but needs no new buy/build decision
- The audio-processing core defaults to **build**: a dependency-free, tested, commented
  implementation. The core is the DSP and audio data path: mixing, effects and filters, FFT and
  analysis, resampling, control curves, real-time handoff (ring buffers), and sample-level editing.
  A crate there needs a stated reason, such as a proven correctness or performance gap, or exposure
  we cannot own (a patent or a platform SDK)
- Everything else defaults to **buy**: a lightweight, well-maintained off-the-shelf crate whose
  licence is in `deny.toml`'s allowlist (HTTP, serialization, CLI, storage, codecs, OS backends,
  tooling). Building instead needs a stated reason: no suitable crate, or one that is heavy for the
  need
- "Lightweight" is a judgement: few transitive dependencies, and few features we do not use
- Pin exact versions for production binaries (`=1.2.3` in Cargo.toml)
- Use `^` (caret) for library crates (allows compatible updates)
- Run `cargo audit` before every release

## Testing
- Unit tests: `#[cfg(test)]` module at the bottom of each file
- Integration tests: `tests/` directory
- Test naming: `test_<function_name>_<scenario>` pattern
- Use `cargo test -- --nocapture` when debugging output
- Mock external dependencies using trait objects, not concrete types
- Aim for 80%+ coverage on library crates; focus on edge cases not happy paths

## Real-Time Audio Thread
- Code that runs on the audio thread (the audio callback and everything it calls) must not:
  allocate or free memory, take a lock (`Mutex`, `RwLock`, or anything that can block), make a
  blocking system call, do file or network I/O, log, or print
- Communicate with the audio thread only through lock-free single-producer/single-consumer queues
  and atomics. Allocate buffers ahead of time and hand them over; send work that must be freed back
  to another thread to drop
- Tests must run audio-thread code inside a guard that fails the test if any of the operations
  above happens. The guard is built in-house; building it requires `unsafe` (see below)

## Memory Safety and Unsafe
- No `unsafe` blocks without a `// SAFETY:` comment explaining the invariants upheld
- If `unsafe` is required, isolate it in a dedicated module (e.g., `src/ffi/` or `src/sys/`)
- Off the audio thread, prefer `Arc<Mutex<T>>` over raw pointers for shared state across threads
- If you need `unsafe`, ask before writing it — we may have a safe alternative

## Module Structure
- `src/lib.rs` or `src/main.rs` — crate root, minimal logic
- `src/error.rs` — all custom error types
- `src/types.rs` — shared types and newtypes
- Keep modules focused: one concern per file, not one per type

## When to Use Each Command
- `cargo check` — fastest feedback during iteration, catches type errors and borrow checker issues
- `cargo build` — full build, use before testing or running
- `cargo clippy -- -D warnings` — run after any code change before committing
- `cargo test` — after all code changes; use `-- --nocapture` when debugging test output
- `cargo fmt` — before every commit; enforced in CI

## Agent skills

### Issue tracker

Issues and specs are GitHub issues in `cparo/debussy`, managed with the `gh` CLI. See
`docs/agents/issue-tracker.md`.

### Triage labels

The five default triage labels, each named after its role (`needs-triage`, `needs-info`,
`ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: a root `GLOSSARY.md` (created lazily) and ADRs in `docs/adrs/`. See
`docs/agents/domain.md`.
