# Default UI host is the IPv4 loopback address

- **Date:** 2026-08-31
- **Status:** accepted and implemented
- **Scope:** `internal/config`, unified UI server defaults, network documentation
- **Task log:** [`docs/tasks/remove-improvements-module.md`](../tasks/remove-improvements-module.md)

## Context

The unified UI server previously listened on every IPv4 interface by default. CORS limits which
browser origins may read responses, but it does not prevent non-browser clients from reaching a
network-exposed server and is not a substitute for authentication or access control.

## Decision

`ui.host` continues to use the standard inline → environment → project → global → default
resolution chain, but its compiled default is `127.0.0.1`. Operators who deliberately need LAN,
container, or remote access can explicitly configure `0.0.0.0` or another interface address.

## Consequences

- A default installation is reachable only from the local machine.
- Remote access becomes an explicit configuration decision.
- Existing explicit `ui.host` values retain their behavior.
- CORS configuration remains separate from bind-address configuration.
