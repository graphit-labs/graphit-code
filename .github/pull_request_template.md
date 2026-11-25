## Outcome

Describe the user-visible or architectural result and why it belongs in Graphit.

## Scope

- What changed:
- What deliberately did not change:
- Related issue or Graphit Task:

## Evidence

List the affected checks, builds, screenshots, or manual flows used to verify the change. Distinguish compilation/waiting from test execution and name any remaining functional limitation.

## Contract review

- [ ] Current behavior is covered without historical regression narratives or external-service test dependencies.
- [ ] New or changed configuration keys include defaults, environment spelling, validation, scope, and documentation.
- [ ] New or changed paths, files, watchers, hooks, caches, ports, or runtime state are documented and safely owned.
- [ ] CLI, MCP, UI, adapter, and documentation surfaces agree where the capability is exposed.
- [ ] Agent-dependent behavior is distinguished from deterministic graph, retrieval, memory, and task behavior.
- [ ] No secret, bearer key, private source, machine-specific absolute path, or generated runtime artifact is included.
- [ ] Versioned content is in English and code comments explain only non-obvious invariants.

## Validation

- [ ] Affected Rust suites: `cargo xtask test <selector>` or `make test TEST=<selector>`
- [ ] Formatting: `cargo fmt --all -- --check`
- [ ] Applicable package/build checks, such as `make build` or `make native-test`
- [ ] UI lint, tests, and build when frontend code changed
- [ ] Documentation links, examples, and screenshots when public behavior changed
