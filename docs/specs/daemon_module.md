---
title: "Daemon Module Specification"
description: "Technical specification of the background Daemon service, project supervisors, OS schedulers, and replacement lifecycles."
content-type: reference
audience: developers
keywords:
  - daemon
  - scheduler
  - supervisor
  - crontab
  - LaunchAgent
  - task scheduler
prerequisites:
  - "docs/architecture/architecture_overview.md"
related:
  - "docs/specs/hub_collaboration.md"
  - "docs/specs/ai_engine.md"
references:
  - target:
      type: session
      id: ses-c98a
    relation: relates_to
  - target:
      type: task
      id: tsk-c847
    relation: supports
  - target:
      type: task
      id: tsk-c8cd
    relation: supports
  - target:
      type: task
      id: tsk-24fc
    relation: supports
  - target: {type: task, id: tsk-3ddb}
    relation: relates_to
  - target: {type: task, id: tsk-4323}
    relation: relates_to
---


# Daemon Module Specification

The daemon composes the Rust Source services for the monitored projects and
versions. Project discovery, filesystem watching, index work, model consumers,
Task/Memory maintenance, presence and shutdown share the existing runtime
owners. A project registration does not grant access to its data.

## Background daemon lifecycle

Startup captures installation/configuration and selected physical project
contexts, acquires the singleton and publishes authenticated discovery only
through the Source lifecycle. A launcher must verify the advertised owner,
not merely trust a PID file or a successful spawn.

[Foreground Source](rust_source_foreground.md) defines multi-project composition,
add/switch/retire, native discovery, request jobs and joined shutdown.
[Runtime project Now](rust_runtime_project_now.md) reads each module through its
own current admission. One project's errors or retirement do not authorize
access to a sibling or silently change its selection.

## Project supervisors and filesystem changes

[Native watchers](rust_daemon_watchers.md) observe actual filesystem changes,
apply captured ignore rules and coalesce work through the project supervisor.
A file notification is a hint to revalidate current inputs, not authority or
proof of content. Work already running retains its original cancellation and
selected generation. New updates can request a later pass without creating a
parallel writer for the same project pipeline.

AST and Knowledge have distinct source and publication contracts. Incremental
work must retain full-source validation where publication requires it. Failed
first publication cannot be mistaken for an existing complete publication.
Model consumers share the admitted model service; they do not load another
model for every project or retain a whole source corpus to service a watcher.

## OS-managed user service

[User services](rust_daemon_services.md) define Start/Status/Stop/restart and
manager-specific invocation. [Detached processes](rust_process_detached.md)
define the supported fallback. A present but failing manager is a causal error,
not permission to launch a second daemon. Installed service identity, authenticated
status and the selected process must agree.

Stop cancels accepted work and waits for owned jobs, descendants and transport
connections before withdrawing discovery, runtime authentication and the
singleton. Incomplete client input cannot hold shutdown indefinitely. An
unobserved or failed join is not a successful stop.

## Processes, diagnostics and logs

Finite children and persistent process owners retain actual exit, stdout/stderr
EOF and cleanup outcomes; see [process ownership](rust_process_detached.md).
[Output](output_module.md) defines runtime tracing, the file sink and
log-tail reader. File sinks require explicit native paths and preserve failures;
readers do not create, repair or change the permissions of missing log files.

The daemon does not reserve artificial aggregate CPU/memory quotas across
pipelines. Concrete request/file limits, writer exclusion, cancellation and
safe process retirement remain enforced by their actual consumers.

## Product entrypoints

See the [Source executable](rust_source_entrypoint.md),
[CLI reference](../guides/cli_reference.md) and
[daemon operations](../guides/daemon_operations.md) for current commands.
Tests use disposable installations and controlled processes; they must not
restart a user's resident service as a fixture.
