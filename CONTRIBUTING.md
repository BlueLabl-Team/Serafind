# Contributing to Serafind

Thank you for helping. Bug reports, ideas and pull requests are welcome.

## Reporting a bug or proposing an idea

Open an issue with the matching template. For a bug, the output of `serafind status` and your
system (distribution and desktop, or Windows version) help a lot. Security problems are reported
privately: see [SECURITY.md](SECURITY.md).

## Building and testing

You need [rustup](https://rustup.rs/) and Git. rustup installs the Rust version of
`rust-toolchain.toml` by itself.

```bash
cargo run --release -p serafind                          # start Serafind, then press Super+F
cargo test --workspace                                   # all the tests
cargo clippy --workspace --all-targets -- -D warnings    # lints, as the CI runs them
cargo fmt --all                                          # formatting
cargo deny check                                         # dependency audit (cargo install cargo-deny)
```

The Windows code can be checked from Linux:
`rustup target add x86_64-pc-windows-msvc`, then
`cargo clippy --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings`.

## Pull requests

- Branch from `dev`, name the branch `feature/<name>`, and open the pull request against `dev`.
  `main` only receives `dev`, for releases.
- Code, comments, documentation and commit messages are in English.
- Every behavior is a default that the user can change in `config.toml` or a theme: a new
  option comes with its key in `crates/serafind-settings/defaults/config.toml`, documented.
- The CI must pass: formatting, Clippy with warnings as errors, and the tests on Linux and
  Windows. New behavior comes with tests.
- Keep pull requests focused: one feature or fix each.

## Project layout

See the *Project layout* section of the README.
