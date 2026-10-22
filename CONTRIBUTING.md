# Contributing to addok-rs

Thank you for your interest in contributing to **addok-rs**! ⚡

## Development Setup

1. **Clone the repository**:
   ```bash
   git clone https://github.com/carapace-foundry/addok-rs.git
   cd addok-rs
   ```

2. **Build the workspace**:
   ```bash
   cargo build
   ```

3. **Run the test suite**:
   ```bash
   cargo test --workspace --all-targets
   ```

4. **Check code formatting and linter**:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   ```

## Pull Requests

- Keep PRs focused on a single topic or bug fix.
- Ensure all automated tests pass before submitting.
- Follow Rust standard formatting (`cargo fmt`).
- Update documentation in `docs/` or `README.md` if adding or modifying features.
