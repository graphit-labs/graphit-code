# Run Graphit as a server in a container

The repository `Dockerfile` runs the global daemon as PID 1, serves the Observatory, and publishes
the streamable HTTP MCP listener. Runtime configuration and authentication are deliberately
separate.

```bash
docker build -t graphit-code .
docker volume create graphit-global
docker run -d --name graphit \
  -p 127.0.0.1:8080:8080 -p 127.0.0.1:8081:8081 \
  -v graphit-global:/opt/graphit graphit-code
```

The image runs `graphit setup --non-interactive` at build time for runtime, event, Agent, and CLI
preferences. Setup also writes the non-secret default provider `local` with ONNX `auto`/`0`; it does
not write an account profile, identity, MCP key, S3 location, or credential. A fresh persistent
volume may hide the image-layer state, so the first no-profile AI resolution recreates `local`
there. To override execution, update `local` in the persistent volume, or create and log in to a
separate provider/profile.

## Configure authentication in the persistent volume

For a service identity backed by the organizational broker:

```bash
docker exec graphit graphit --non-interactive provider add server --type local \
  --mcp-endpoint http://127.0.0.1:8081/mcp \
  --broker-endpoint https://broker.example.com \
  --embedding-mode broker --rerank-mode broker

docker exec graphit graphit --non-interactive login \
  --profile service --provider server --username graphit-service \
  --mcp-key "$MCP_KEY" \
  --broker-key "$BROKER_KEY"
```

Avoid passing secrets in a shared shell history. In orchestration, inject them from the platform's
secret store into the one-shot login process. `auth.json` is stored in the mounted global volume,
mode `0600`, under a mode-`0700` directory.

For enterprise identity, configure an OIDC provider as documented in [Authentication](authentication.md).
A non-interactive workload supplies access/ID/refresh tokens to `login`; Graphit verifies the ID
token and renews the OIDC session. Storage topology and S3 credentials exist only in the
separately deployed broker.

## MCP authentication

The local daemon creates a new runtime key at each start; it is visible in **System → Daemon** and
`/opt/graphit/daemon/mcp.key`. A local provider may also supply a static MCP key. With an active
direct OIDC or Broker-managed provider, every remote client sends its own access token as
`Authorization: Bearer ...`; Graphit verifies it against the issuer or Broker userinfo and uses
that request identity for all broker calls. Direct OIDC may use relay or configured RFC 8693
exchange. It never substitutes the service profile's token for an inbound user.

The Observatory UI has no built-in authentication. CORS does not authenticate non-browser clients.
Keep both ports on loopback/private networking or place an authenticated reverse proxy in front.

## Multiple accounts

All profiles remain in the mounted global volume. `login` activates its profile atomically;
`account use NAME` changes the active account and therefore the identity, MCP Bearer, broker access,
Hub ACL view, and user caches together.

```bash
docker exec graphit graphit account list
docker exec graphit graphit account use service
docker exec graphit graphit logout --profile old-service
```

## Health and lifecycle

The image exposes UI port 8080 and MCP port 8081 by default. Its health check calls the UI health
endpoint. Stop/restart through the container runtime; provider and profile state survives in the
volume. A daemon restart rotates only the generated runtime key, not profile credentials.

See [Authentication](authentication.md), [Configuration](configuration.md), and
[Filesystem contract](filesystem_contract.md).
