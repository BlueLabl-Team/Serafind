# Security policy

## Reporting a vulnerability

Please do **not** open a public issue for a security problem.

Report it privately instead: on GitHub, go to the **Security** tab of this repository and choose
**Report a vulnerability**. Describe what an attacker can do, on which system (Linux desktop,
Windows version), and how to reproduce it.

You will get an answer within a week. Once a fix is released, the report is published with
credit to you, unless you prefer otherwise.

## Supported versions

Only the latest release gets security fixes.

## What Serafind protects against

- **Local only.** Serafind does not use the network. The resident process listens on a Unix socket in the user's private runtime folder, or on a named pipe on Windows that refuses remote clients. Both ends check that the other runs as the **same user**, so another user of the computer cannot read what is typed in the bar or drive Serafind.
- **No shell.** Programs are started with their arguments passed one by one: a file name cannot inject a command.
- **What the socket allows.** Any program of the same user can ask Serafind to search, open a file, start an application or open a terminal. That gives no more rights than the user already has, but do not give a sandboxed application (Flatpak, Snap) access to the socket.
- **Secrets stay out of the bar.** The Content mode never reads shell histories, `.netrc`, `.git-credentials`, `.pgpass`, `.env` files, keys or certificates (`[content] exclude_names`). Hidden folders such as `~/.ssh` are not indexed at all by default. The log and `usage.json` can only be read by the user.
- **Untrusted files.** Icons are drawn without external resources, size limits apply to images and to the files read, and regular expressions run in linear time.
- **Releases.** The CI pins its actions and tools to exact versions, and audits the dependencies of every change (`cargo-deny`). Check a download against `SHA256SUMS.txt`; for public releases, also check its build provenance with `gh attestation verify <file> --repo BlueLabl-Team/Serafind`.

## Scope

Serafind runs locally, with the rights of the user, and does not use the network. The section
above explains what it protects against. Reports are welcome about anything that
lets another user of the computer, a file, or a downloaded release do more than that section
says.
