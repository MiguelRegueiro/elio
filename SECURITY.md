# Security Policy

## Supported Versions

Security fixes are provided for the latest released version of `elio`.

If possible, test suspected vulnerabilities against the newest release before
reporting. If you can only reproduce the issue on an older version, include that
version in the report.

## Reporting a Vulnerability

Please do not open a public GitHub issue for suspected security
vulnerabilities.

Report vulnerabilities through GitHub private vulnerability reporting for this
repository:

<https://github.com/elio-fm/elio/security/advisories/new>

When reporting, please include:

- the affected `elio` version or commit
- your operating system and terminal
- a clear description of the issue
- reproduction steps or a proof of concept, when possible
- whether the issue depends on a crafted file, archive, path, config, theme, or
  terminal escape sequence
- whether the issue depends on optional external preview tools

Reports will be reviewed and triaged as promptly as practical. Confirmed
vulnerabilities will be addressed based on their severity and impact, with fixes
and disclosures coordinated as appropriate.

## Project Security Scope

`elio` is a local terminal file manager. It does not run a server, accept remote
logins, or intentionally process untrusted network requests.

Security-sensitive areas include:

- previewing malformed or malicious local files, archives, documents, media, or
  images
- invoking optional external preview tools
- opening files or folders with platform launchers and discovered applications
- handling unusual filenames, paths, symlinks, mounts, and trash operations
- copying file metadata to the clipboard, including through OSC52 and platform
  clipboard helpers
- rendering terminal output, including terminal escape sequences
- parsing user-provided config and theme files

If you are unsure whether a behavior is security-sensitive, report it privately.

## Dependency Advisories

Rust dependency advisories are reviewed during maintenance and release work. If
your report concerns a dependency advisory, include the advisory ID, affected
versions, and any known impact on `elio`.
