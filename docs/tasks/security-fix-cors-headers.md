# Security Fix: CORS Headers and Security Hardening

## Summary

Replaced wildcard (`*`) CORS `Access-Control-Allow-Origin` headers with a strict localhost-only allowlist across all three HTTP servers. Added `X-Content-Type-Options: nosniff` and `X-Frame-Options: DENY` security headers to all CORS middleware.

## Motivation

All three UI servers (Hub, Wiki, AST Visualizer) used `Access-Control-Allow-Origin: *`, which allows any website to make cross-origin requests to the local server. Since these servers bind to `localhost` and expose project data (source code, AST graphs, wiki content), a malicious website could read sensitive project information from a user's machine via cross-origin fetch requests.

## Changes

### H-1a: Hub UI Server (`internal/hub/ui_server.go`)

- **Function:** `CorsWrap`
- **Before:** `w.Header().Set("Access-Control-Allow-Origin", "*")`
- **After:** Reads `Origin` header, validates against `isAllowedOrigin()`, sets origin dynamically only if allowed.
- Added `isAllowedOrigin()` helper to the `hub` package.

### H-1b: Wiki Handler (`internal/uiserver/wiki_handler.go`)

- **Function:** `corsJSON`
- **Before:** `w.Header().Set("Access-Control-Allow-Origin", "*")`
- **After:** Same localhost-only origin validation pattern.
- Added `isAllowedOrigin()` helper to the `uiserver` package.

### H-1c: AST Server (`internal/ast/server.go`)

- **Function:** `corsMiddleware`
- **Before:** `w.Header().Set("Access-Control-Allow-Origin", "*")`
- **After:** Same localhost-only origin validation pattern.
- Added `isAllowedOrigin()` helper to the `ast` package.

### L-2: Security Headers (all three servers)

Added to all CORS middleware functions:
- `X-Content-Type-Options: nosniff` — prevents MIME-type sniffing attacks.
- `X-Frame-Options: DENY` — prevents clickjacking via iframe embedding.

## Allowed Origins

The `isAllowedOrigin` function allows:
- `http://localhost` (with and without port)
- `http://127.0.0.1` (with and without port)
- `http://[::1]` (with and without port)
- Empty origin (same-origin requests don't send an `Origin` header)

## Design Decisions

- **Duplicated `isAllowedOrigin`** across three packages instead of creating a shared internal package. The function is small (10 lines), stable, and the packages are in different domains. Extracting to a shared package would add coupling for minimal benefit.
- **Dynamic origin reflection** instead of a hardcoded value: the `Access-Control-Allow-Origin` header is set to the actual request origin (when allowed) rather than a fixed string. This correctly handles all localhost variants without requiring `Access-Control-Allow-Credentials`.

## Testing

- `make ci` passes with all changes.
