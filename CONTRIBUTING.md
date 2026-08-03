# Contributing to elio

Thank you for your interest in contributing to elio!

Bug reports, feature requests, documentation improvements, preview fixes, and
code changes are welcome.

This guide covers the basics for contributing to the project.

## Getting Started

Install Rust with [rustup](https://rustup.rs/), fork the repository, and clone
your fork:

```bash
git clone https://github.com/<your-username>/elio.git
cd elio
git checkout -b your-branch-name
```

The repository includes [`rust-toolchain.toml`](rust-toolchain.toml), so rustup
will use the expected toolchain and components automatically.

## Project Structure

A brief overview of the repository layout:

```text
.
├── .github/                    # GitHub workflows and repository automation
├── assets/                     # Bundled assets such as the logo, themes, and syntax data
├── docs/                       # Architecture notes
├── examples/                   # Example config and theme files
├── packaging/                  # Distribution packaging files
├── src/
│   ├── app/                    # Application state, jobs, and user actions
│   ├── archive/                # Archive creation, extraction, and format handling
│   ├── config/                 # Config and theme loading/parsing
│   ├── core/                   # Shared model types used across layers
│   ├── file_info/              # File classification and metadata discovery
│   ├── fs/                     # Filesystem access and path-level operations
│   ├── preview/                # Preview construction and preview-specific tests
│   ├── runtime/                # App runner, terminal lifecycle, drawing, and session output
│   ├── shell_integration/       # Shell scripts and install/uninstall support
│   ├── ui/                     # Terminal rendering, layout, theming, and interaction
│   ├── cli.rs                  # Command-line parsing and command dispatch
│   ├── lib.rs                  # Public library API entrypoints
│   └── main.rs                 # Binary entrypoint
├── tests/                      # Integration tests and architecture guardrails
├── build.rs                    # Build-time asset preparation
├── CHANGELOG.md                # Release notes and unreleased user-facing changes
├── CONTRIBUTING.md             # Contributor guide
├── Cargo.toml                  # Package manifest and dependency configuration
├── rust-toolchain.toml         # Rust toolchain and component configuration
└── README.md                   # Project overview and user documentation
```

If you are not sure where a change belongs, start by reading
[`docs/architecture.md`](docs/architecture.md).

## Development Workflow

Build the project:

```bash
cargo build
```

Run `elio`:

```bash
cargo run
```

For configuration and theme work, use the examples in
[`examples/config.toml`](examples/config.toml) and [`examples/themes/`](examples/themes/).

## Local Checks

Before opening a pull request, run:

```bash
cargo fmt --check
cargo test --locked --test architecture_guardrails
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps
```

## Pull Requests

Keep pull requests focused and easy to review. If you are proposing a larger
feature or behavior change, open an issue or discussion first so the approach can
be discussed before implementation.

Preview and platform behavior can vary by OS, terminal, and available helper
tools. Avoid broad changes in this area, manually test affected behavior, and
mention the OS and terminal in the pull request.

Follow the pull request template and make sure your description explains what
changed, why it changed, and how it was tested.

For user-visible changes, add a short entry under `## [Unreleased]` in
[`CHANGELOG.md`](CHANGELOG.md).

## Security

For vulnerability reporting and supported-version policy, see
[`SECURITY.md`](SECURITY.md).
