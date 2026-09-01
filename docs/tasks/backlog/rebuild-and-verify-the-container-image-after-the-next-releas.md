# Rebuild and verify the container image after the next release

The root `Dockerfile` installs a published GitHub release and then runs `graphit setup` answering
every question with a flag. Those flags landed in the working tree in
`docs/tasks/dockerfile-and-non-interactive-setup.md` but are not in any release yet: `GRAPHIT_VERSION`
defaults to `latest`, which currently resolves v0.1.26, and that binary has no `--hub-bucket`.

The Dockerfile detects this deliberately rather than failing obscurely. The setup RUN probes
`graphit setup --help` for `--hub-bucket` and, when it is absent, exits with a message naming the
missing flag and telling the operator to pin a newer tag. It explicitly does NOT fall back to piping
newlines into the prompts, because piped answers are positional: one prompt added upstream shifts
every later answer onto the wrong key and setup then reports success having stored, say, the region
as an endpoint.

**What to do once a release contains the flags:**

1. `docker build -t graphit-code .` — with `GRAPHIT_VERSION` left at `latest`, or pinned to that tag.
2. Confirm the setup step completes: local-only hub (no bucket configured at build time), the
   embedding model downloaded into the image, and zero prompts — no question may be reached without
   its flag, or it silently takes the default from the build's empty stdin.
3. `docker run -d -p 127.0.0.1:8080:8080 -v graphit-global:/opt/graphit -v <projects>:/workspace graphit-code`
   and check `/health`, the healthcheck status, and that the UI answers on 8080.
4. Register a project inside the container (`graphit init` + `graphit sync`) and confirm it appears
   in the ecosystem view, then restart the container and confirm it is still there — that is what
   proves the `/opt/graphit` volume is carrying `global.lock.json`.
5. Authenticate the bundled CLI and run one live search end to end. Live search IS the agent CLI, so
   it is the only check that exercises `AI_CLI` beyond "the binary exists".
6. Build at least one other `AI_CLI` value, and one with a remote embedding provider
   (`--build-arg EMBEDDING_PROVIDER=…` plus `EMBEDDING_MODEL` and, for `openai-compatible`,
   `EMBEDDING_BASE_URL`) to confirm the three extra questions are all answered.

**Then remove the guard, or keep it?** Keep it. It costs one `--help` invocation and it is the
difference between a named cause and `unknown flag` for anyone who pins an older tag.

Relevant paths: `Dockerfile`, `install.sh`, `cmd/graphit/commands/setup.go`,
`docs/guides/container.md`, `docs/tasks/dockerfile-and-non-interactive-setup.md`.
