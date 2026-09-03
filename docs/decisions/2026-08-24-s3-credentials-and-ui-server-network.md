# Explicit S3 credentials are optional; UI server network uses layered config

- **Date:** 2026-08-24
- **Status:** partially superseded — the host default is superseded by [`2026-08-31-default-ui-host-loopback.md`](2026-08-31-default-ui-host-loopback.md)
- **Scope:** `cmd/graphit/commands`, `internal/config`, `internal/s3store`,
  `internal/lancestore`, `internal/ast`, `internal/hub`, `internal/uiserver`,
  `internal/netutil`
- **Task history:** Graphit Task `tsk-ba2a127c3b75`

## Context

The Hub already used S3 and left authentication entirely to the AWS provider chain. That is the safest behavior in environments with profiles and roles, but it made first-time setup harder for S3-compatible installations that hand out an access key and secret directly.

The UI server also had two local assumptions: the CORS policy accepted only loopback origins and the page injected `localhost` as the API base. The CORS policy is a deliberate protection because endpoints expose project data, but network installations need to be able to declare trusted origins. The listen address, in turn, needed to be explicit and configurable, with all interfaces as the new default.

## Decision

1. `graphit setup` optionally prompts for `hub.access_key_id` and `hub.secret_access_key`. Only a complete pair is persisted in global config; any blank field removes the explicit pair and keeps the existing provider chain. The prompt warns about this fallback and does not echo the secret.
2. All S3 consumers use the configured pair only when it is complete. Otherwise, AWS SDK, LanceDB, and LadybugDB keep their current environment/profile/role mechanisms.
3. `ui.host` follows normal inline → env → project → global → default resolution. The original
   `0.0.0.0` default was superseded on 2026-08-31; the current default is `127.0.0.1`.
4. `ui.allowed_origins` accepts a comma-separated list via the same mechanism. With no value, the localhost allowlist remains. With a value, the explicit list replaces the default. `*` is an explicit and dangerous opt-in for any origin.
5. The unified UI uses `/api` on the same origin instead of injecting `localhost`, so it works by remote hostname or reverse proxy.

## Consequences

- A simple setup now serves MinIO and S3-compatible providers without requiring the operator to prepare environment variables before the first command.
- The preferable alternative remains leaving the fields blank and using temporary credentials from the provider chain. When provided, the secret stays in global config as readable text, protected by `0600` permissions; read commands show it as `[REDACTED]`.
- Operators can still publish the UI server on `0.0.0.0` explicitly. CORS remains a separate
  browser-origin policy and is not network access control.
- A project value overrides the global one, allowing a corporate default with versioned per-project exceptions. Explicit origin values must also include any localhost origin that should still work.
- The UI server does not implement authentication. CORS is a browser policy, not network access control; a reachable instance needs firewall, VPN, or authenticated TLS proxy, and must not be published directly to the Internet.

The operational procedure, examples, and troubleshooting are in [`docs/guides/s3-and-ui-network.md`](../guides/s3-and-ui-network.md).
