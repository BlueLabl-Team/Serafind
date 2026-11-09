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

## Continuous integration and releases

- **Every pull request** to `dev` or `main` (and every push to them) is checked by GitHub Actions: formatting, Clippy with warnings as errors and all the tests on Linux; Clippy and the tests on Windows; the dependencies with [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) (known vulnerabilities, licenses compatible with Apache-2.0, crates.io only — see `deny.toml`).
- **A release** is made by pushing a tag `vX.Y.Z` on `main` (it must match the version of `Cargo.toml`). The workflow builds the `dist` profile and makes a **draft** GitHub Release, to review then publish, with:
  - `serafind-X.Y.Z-linux-x86_64.tar.gz` (built on Ubuntu 22.04: runs on distributions from 2022 on);
  - `serafind_X.Y.Z_amd64.deb` and `serafind-X.Y.Z-1.x86_64.rpm`;
  - `serafind-X.Y.Z-windows-x86_64.zip` and `serafind-X.Y.Z-windows-x86_64-setup.exe` (installs for the current user, no administrator rights);
  - `SHA256SUMS.txt`.

  Every archive and package also holds `THIRD_PARTY_LICENSES.html` (the licenses of the crates built in, made by [cargo-about](https://github.com/EmbarkStudios/cargo-about)) and the licenses of the embedded fonts.

```bash
git switch main && git pull
git tag vX.Y.Z && git push origin vX.Y.Z   # the version of Cargo.toml
```

## Fedora package (COPR)

The Fedora package (`packaging/fedora/serafind.spec`) repackages the program of the GitHub release:
Fedora's Rust is older than the one Serafind needs. COPR makes its source package with
`.copr/Makefile` (the version comes from `Cargo.toml`), and `.github/workflows/copr.yml` starts a
build when a release is published, through the COPR webhook stored in the `COPR_WEBHOOK` secret.

To try it in a container:

```bash
podman run --rm -v "$PWD":/src:ro,Z -w /src registry.fedoraproject.org/fedora:42 bash -c \
  'dnf -y install make git-core && make -f .copr/Makefile srpm outdir=/tmp/out && rpmbuild --rebuild /tmp/out/*.src.rpm'
```

## Project layout

See [docs/architecture.md](docs/architecture.md): the crates, the libraries used and how the resident process and the bar work together.
