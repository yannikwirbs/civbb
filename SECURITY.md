# Security policy

## Supported versions

CIVBB is pre-release. Only the latest `main` gets security fixes.

## Reporting a vulnerability

Do not open a public issue. Report privately through [GitHub private vulnerability reporting](https://github.com/yannikwirbs/civbb/security/advisories/new).

Include the affected component, steps or input to reproduce, and the impact you expect. You should get a first response within 7 days.

Most relevant: anything at a trust boundary (Wasm script sandbox, network protocol, content and file parsers, upload pipeline). Untrusted content must stay data and never become code; a way around that is a vulnerability.
