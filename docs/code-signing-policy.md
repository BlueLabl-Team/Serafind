# Code signing policy

Free code signing for the Windows releases of Serafind is provided by [SignPath.io](https://about.signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).

## What is signed

Only files built by the project's own [release workflow](../.github/workflows/release.yml) on GitHub Actions, from a tag on the `main` branch of this repository:

- `serafind.exe`, the program;
- `serafind-X.Y.Z-windows-x86_64-setup.exe`, its installer.

Nothing built on a personal computer is ever signed. The source code of every signed file is public in this repository, at the tag of its release.

## Team roles

| Role | Members |
|---|---|
| Committers and reviewers | [ArcheoPsy](https://github.com/ArcheoPsy) |
| Approvers | [ArcheoPsy](https://github.com/ArcheoPsy) |

- **Committers and reviewers** may change the source code. Every change goes through a pull request; the `main` and `dev` branches are protected (no direct or forced pushes, and the checks must pass).
- **Approvers** review each signing request before it is signed. A request is only approved for a release built by the release workflow from a tag on `main`.

All members use multi-factor authentication on GitHub and on SignPath.

## Privacy policy

This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

Serafind works entirely on the computer it runs on. It indexes the user's own files to search them, keeps that index in memory, and talks only to its own processes, through a local socket that other users of the computer cannot reach. It has no telemetry, no update check and no network access.
