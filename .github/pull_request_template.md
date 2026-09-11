## Outcome

Describe the user-visible or architectural result and why it belongs in Graphit.

## Scope

- What changed:
- What deliberately did not change:
- Related issue or Graphit Task:

## Evidence

List the commands, focused tests, full-suite checks, builds, screenshots, or manual flows used to verify the change.

## Contract review

- [ ] Current behavior is covered without historical regression narratives or external-service test dependencies.
- [ ] New or changed configuration keys include defaults, environment spelling, validation, scope, and documentation.
- [ ] New or changed paths, files, watchers, hooks, caches, ports, or runtime state are documented and safely owned.
- [ ] CLI, MCP, UI, adapter, and documentation surfaces agree where the capability is exposed.
- [ ] Agent-dependent behavior is distinguished from deterministic graph, retrieval, memory, and task behavior.
- [ ] No secret, bearer key, private source, machine-specific absolute path, or generated runtime artifact is included.
- [ ] Versioned content is in English and code comments explain only non-obvious invariants.

## Validation

- [ ] `make test` (lightweight local unit tier)
- [ ] `make test-full` (automatic local cgroup on Linux, or an isolated CI runner)
- [ ] `make lint`
- [ ] `make build-local`
- [ ] UI lint, tests, and build when frontend code changed
- [ ] Documentation links, examples, and screenshots when public behavior changed
