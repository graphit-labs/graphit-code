---
name: graphit-improvements
description: Autonomous code improvement, audit, review, refactoring methodology, and dream subjects. Use after completing any significant task for reflection, when the user asks to improve/audit/review code, or when you discover improvement opportunities to queue for later processing.
---

# Code Improvement Methodology Rule

## When to Use

When the user asks you to **autonomously improve**, **audit**, **review**,
or **refactor** the codebase (or parts of it), you MUST follow the
engineering analysis methodology detailed below.

---

# Code Improvement Analysis Methodology

This document defines the engineering principles and analysis methodology
for evaluating and improving a codebase. Follow each section systematically.

## Clean Code Principles

Your analysis and improvements MUST be guided by these established engineering principles.
Clean Code is the philosophy that **code is read far more often than it is written**. Therefore, code must be written for humans first, compilers second.

### DRY — Don't Repeat Yourself
- Every piece of knowledge (logic, business rules, constants) must have a **single** representation
- Hunt for duplicated code blocks, copied logic, and repeated patterns
- Extract shared logic into reusable functions, methods, or packages
- If a bug fix would require changing the same thing in multiple places, the code is WET — fix it
- Consolidate duplicated string literals, magic numbers, and configuration values into named constants

### YAGNI — You Aren't Gonna Need It
- Identify over-engineered abstractions, unused parameters, and dead code paths
- Remove code that was written "for the future" but serves no current purpose
- Flag interfaces with only one implementation that add complexity without value
- Simplify structures that anticipate requirements that don't exist yet
- Delete unused exports, variables, types, and functions — dead code is a liability

### Avoid Over-Engineering
- **Solve the problem you HAVE, not the problem you IMAGINE you might have** — speculative abstractions are the #1 source of unnecessary complexity
- Flag "architecture astronaut" patterns: deep inheritance hierarchies, abstract factories of abstract factories, frameworks-within-frameworks, and plugin systems with a single plugin
- Prefer composition over inheritance — favor small, composable functions and structs over deep class trees
- Avoid premature generalization: if something is used in exactly one place, it doesn't need to be generic, configurable, or pluggable
- **Three-strikes rule**: wait until a pattern appears three times before extracting a shared abstraction — two is a coincidence, three is a pattern
- Flag wrapper layers that add no logic — if a function just calls another function with the same signature, the wrapper is overhead
- Question every indirection layer: each hop between "call site" and "actual logic" must justify its existence with a concrete benefit (testability, decoupling, reuse)
- Beware of "configuration-driven" designs where a simple `if` statement would suffice — not everything needs to be a YAML file, a database row, or a feature flag
- Avoid creating microservices for what should be a function call — network boundaries are expensive and should only exist for independent scaling, deployment, or team ownership

### Proven Design Patterns
- **Reach for a proven pattern before inventing a custom solution** — decades of industry experience have produced battle-tested structures for recurring problems
- When you identify a structural problem, check if a GoF (Gang of Four) or domain-specific pattern addresses it: Strategy, Observer, Decorator, Repository, Circuit Breaker, etc.
- **Apply patterns to simplify, never to impress** — if the pattern makes the code harder to understand than the problem it solves, it's the wrong pattern or the wrong time
- Common patterns to look for and recommend where they fit naturally:
  - **Strategy**: Replace `switch/case` on type with interchangeable behavior implementations
  - **Repository**: Separate data access from business logic — all persistence behind an interface
  - **Factory**: Centralize complex object creation to avoid scattered `new` calls with the same boilerplate
  - **Observer/Event Bus**: Decouple producers from consumers for notifications, logging, and side-effects
  - **Decorator/Middleware**: Layer cross-cutting concerns (auth, logging, metrics) without modifying core logic
  - **Circuit Breaker**: Protect the system from cascading failures when calling unreliable external services
  - **Builder**: Simplify construction of complex objects with many optional parameters
  - **Retry with Backoff**: Handle transient failures in network calls, DB writes, and queue publishes
- **Never force a pattern where it doesn't fit** — applying a pattern to a problem it wasn't designed for IS over-engineering
- When recommending a pattern, explain **why** it fits the specific situation, not just **what** the pattern is

### Decoupling & Modularization
- **Depend on interfaces, not implementations** — modules should communicate through well-defined contracts (interfaces, protocols, message schemas), not by reaching into each other's internals
- **Enforce clear module boundaries**: each module/package should have a single, well-defined responsibility and a minimal public API surface — if everything is exported, nothing is encapsulated
- Flag tight coupling indicators: direct struct field access across packages, circular imports/dependencies, one module's tests needing to set up another module's internal state
- **Dependency Injection**: pass dependencies as constructor parameters or function arguments instead of importing and instantiating them inline — this enables testing, swapping implementations, and respecting the Dependency Inversion Principle
- **Isolated Business Logic**: domain rules, calculations, validations, and workflow transitions MUST live in a dedicated layer (service, domain, or use-case) that has NO dependency on transport (HTTP, gRPC), persistence (SQL, ORM), or UI frameworks
  - A handler/controller should only parse input, call the business layer, and format the response — never contain domain rules
  - A repository should only read/write data — never enforce business invariants
  - Business logic must be testable with plain unit tests, without needing an HTTP server, a database, or a frontend running
  - **Design for testability**: business functions should accept inputs and return outputs (pure functions where possible) — avoid hidden dependencies on global state, singletons, or environment
  - External dependencies (DB, APIs, file system, clock, randomness) must be injected via interfaces so they can be replaced with mocks/stubs in tests
  - Flag business logic that can only be tested through integration/E2E tests because it's tangled with infrastructure — this is a design flaw, not a testing strategy
  - Flag business rules scattered across controllers, middleware, templates, or stored procedures — centralize them
- Flag "God objects" and "God packages" — modules that know about everything, import everything, or accumulate unrelated responsibilities are a coupling magnet and should be decomposed
- Check for feature envy: code that constantly reaches into another module's data or methods probably belongs in that other module
- **Minimize the blast radius of changes**: if modifying a module forces changes in 5+ other files across different packages, the boundaries are wrong — refactor to isolate the change
- Prefer event-driven or message-based communication between subsystems over direct function calls when the caller doesn't need the result — this decouples lifecycle and deployment

### KISS — Keep It Simple, Stupid
- Prefer the most direct, readable solution over clever or abstract ones
- Replace complex multi-layer abstractions with simpler alternatives when they solve the same problem safely
- Flatten deeply nested conditionals and early-return where possible
- If a function needs a multi-line comment to explain what it does, the function should be rewritten
- Choose clarity over brevity — a few more lines of readable code beat a one-liner no one understands

### Meaningful Names
- Variables, functions, and types should have names that explain exactly what they do or contain
- Rename single-letter variables (except loop iterators), cryptic abbreviations, and misleading names
- Function names should describe their action: `calculateTotalPrice()` not `calc()`, `isUserAuthorized()` not `check()`
- Boolean variables should read as questions: `isValid`, `hasPermission`, `canRetry`

### Small Functions, Single Responsibility
- Functions should do **one thing** and do it well
- If a function exceeds ~40 lines, look for opportunities to decompose it
- Extract nested logic into named helper functions that describe their purpose
- Each function should operate at a single level of abstraction

### Robust Error Handling
- Errors should be handled explicitly, never silently swallowed
- Error messages should provide context: what failed, why, and what input caused it
- Distinguish between recoverable and unrecoverable errors
- Never use generic error messages like "something went wrong" — be specific
- Ensure cleanup logic runs even on error paths (defer, finally, etc.)

### Avoid Unnecessary Comments — Documentation Lives in `docs/`
- **Code documentation belongs in `docs/`, NOT in code comments** — this is a mandatory rule, not a suggestion
- Clean code is self-documenting — if you need a multi-line comment to explain logic, rewrite the logic to be self-explanatory
- Remove commented-out code blocks — version control handles history
- Remove TODO/FIXME comments for issues that can be fixed right now
- **Only critical comments belong in code**: safety warnings (`// SAFETY:`), non-obvious gotchas (`// NOTE:`), legal headers, compiler/linter directives, and intentional deviation markers (`// DECISION: see docs/decisions/...`)
- **Never write comments explaining WHAT the code does** — rename variables, extract functions, and simplify logic so the code speaks for itself
- **Never document architecture, behavior, or business logic in code comments** — put it in `docs/architecture/`, `docs/specs/`, or `docs/decisions/`
- Verbose docstrings that describe implementation details are clutter — the code should be readable without them
- The golden rule: if a comment explains *what*, rewrite the code; if it explains *why* (non-obvious), keep a one-liner pointing to `docs/`

## Security Audit

Perform a thorough security review as if preparing for a **penetration test**:

### Never Trust the Frontend
- **All validation must be enforced server-side** — frontend validation is UX, not security
- **Never place business logic in the frontend** — pricing calculations, authorization decisions, discount rules, workflow transitions, and any logic that affects data integrity or money MUST live on the backend
- Assume every frontend request is forged: validate all inputs, recalculate all derived values, and re-check all permissions on the server
- Never rely on hidden form fields, client-side feature flags, or frontend state to enforce security boundaries
- Check for client-side-only checks that an attacker can bypass by calling the API directly (e.g., role checks in JavaScript, disabled buttons as the only protection)
- Verify that the frontend does not receive data it shouldn't display — filter sensitive fields on the server before serialization, not in the UI layer
- Flag any pattern where the frontend sends computed results (totals, discounts, permissions) that the server accepts without re-deriving

### Sensitive Data Exposure
- **Logs**: Search for API keys, tokens, passwords, secrets, or PII being logged at any level (debug, info, error)
- **Frontend**: Ensure sensitive data (internal IDs, user emails of other users, admin flags, full credit card numbers, secret keys) is never sent to the client — filter at the API serialization layer, not in the UI
- **Comments**: Find hardcoded credentials, connection strings, or secrets in code comments
- **Debug output**: Identify debug/development code that dumps sensitive structs, request bodies, or headers
- **API responses**: Check that API endpoints don't return more data than necessary (internal IDs, stack traces, config values, adjacent user data)
- **Error messages**: Ensure error responses to clients don't leak implementation details, file paths, or database schemas
- **Environment variables**: Verify that secrets are loaded from env/vault, never hardcoded in source
- **Browser storage**: Flag sensitive data stored in localStorage, sessionStorage, or cookies without Secure/HttpOnly flags

### Input Validation & Injection
- Check for SQL injection, command injection, and path traversal vulnerabilities
- Ensure all external input (HTTP params, headers, file paths, env vars) is validated and sanitized
- Verify that user input is never interpolated directly into queries, commands, or templates
- Check for proper escaping in HTML output, JSON construction, and shell commands

### Authentication & Authorization
- Verify that endpoints enforce proper authentication checks
- Check for authorization bypass — ensure users can only access their own resources
- Look for timing attacks in authentication comparisons (use constant-time comparison)
- Ensure session tokens, JWT secrets, and API keys are handled securely

### Cryptography
- Flag usage of weak or deprecated algorithms (MD5, SHA1 for security, DES, RC4)
- Verify that random values used for security purposes come from crypto/rand, not math/rand
- Check that TLS configurations don't allow insecure cipher suites or skip certificate verification

### Penetration Testing Readiness
- **Denial of Service (DoS)**: Check for missing rate limiting on public endpoints, unbounded resource allocation (unlimited file uploads, unbounded query results, unrestricted pagination), and operations with no timeout that could be exploited to exhaust server resources
- **Race Conditions**: Identify security-sensitive operations vulnerable to race conditions — double-spending in payment flows, TOCTOU in permission checks, concurrent coupon/voucher redemption, and parallel account creation bypassing uniqueness constraints
- **Resource Exhaustion**: Flag endpoints that accept large payloads without size limits, recursive processing without depth limits, or regex patterns vulnerable to ReDoS (Regular Expression Denial of Service)
- **API Abuse**: Check for missing rate limiting, lack of request throttling per user/IP, and absence of CAPTCHA or proof-of-work on expensive operations (registration, password reset, search)
- **Mass Assignment**: Verify that API endpoints don't accept arbitrary fields that could overwrite internal attributes (isAdmin, role, balance) — use explicit allowlists for bindable fields
- **Broken Object-Level Authorization (BOLA/IDOR)**: Check that every data-access endpoint validates that the requesting user owns or has permission to access the specific resource ID

### Security Testing
- **Tests MUST cover vulnerability scenarios**, not just happy-path functionality — if a security concern exists in the code, a test must prove it's defended
- Verify that test suites include negative security cases: injection attempts (SQL, XSS, command), authentication bypass, authorization escalation, and invalid/malformed input
- Check for BOLA/IDOR test coverage: tests should attempt to access resources belonging to other users and assert denial
- Flag security-critical endpoints with zero test coverage — auth flows, payment processing, password reset, token validation, and admin operations are NOT optional to test
- Verify that secrets and credentials used in tests are synthetic/test-only — never use production credentials in test fixtures or CI pipelines
- Recommend integration with SAST (Static Application Security Testing) and DAST (Dynamic Application Security Testing) tools in the CI pipeline where applicable
- Check that security-related dependencies (auth libraries, crypto packages, TLS configs) are covered by version-pinning and automated vulnerability scanning (e.g., Dependabot, Snyk, Trivy)

## Concurrency & Idempotency Review

Analyze the codebase for concurrency safety and transactional correctness:

### Race Conditions
- Identify shared mutable state accessed from multiple goroutines/threads without synchronization
- Check that maps, slices, and counters used concurrently are protected by mutexes or use atomic operations
- Verify that channel operations won't deadlock (check for missing close, unbuffered sends without receivers)
- Look for TOCTOU (Time-of-check-to-time-of-use) bugs in file operations and cache lookups

### Idempotency & Atomic Transactions
- Identify operations that should be idempotent but aren't (API handlers, message consumers, retryable tasks)
- Check for create-or-update patterns that may cause duplicates on retry
- Verify that database writes in retry loops use proper upsert semantics or deduplication keys
- Ensure payment, notification, and state-transition operations are safe to replay
- **Atomic transactions**: multi-step operations that must succeed or fail as a unit MUST be wrapped in a database transaction or equivalent mechanism — partial writes corrupt state
  - Flag code that performs multiple related writes (e.g., create order + deduct inventory + charge payment) without transactional guarantees
  - For distributed systems where a single DB transaction is not possible, verify that a **saga pattern** or **compensation mechanism** exists to roll back partial progress on failure
  - Check that transaction scopes are as small as possible — holding a transaction open during network calls or user interaction causes lock contention and timeouts
- Verify that read-after-write consistency is guaranteed where required — a user who creates a resource must see it immediately, not after a replication lag
- Flag fire-and-forget writes to critical data paths — if the write can fail silently, the system state becomes inconsistent

### Resource Management
- Check for resource leaks: unclosed files, HTTP bodies, database connections, channels
- Verify that context cancellation is properly propagated and respected
- Ensure goroutines have a termination path — no orphaned goroutines that leak forever
- Check for proper connection pool sizing and timeout configuration

## Cloud Readiness — Twelve-Factor App Review

Evaluate the codebase against the **Twelve-Factor App** methodology to ensure it is 
cloud-native, horizontally scalable, and portable across infrastructure providers:

### I. Codebase
- Verify a single codebase tracked in version control that produces multiple deployments
- Flag any patterns where environment-specific code branches exist instead of configuration

### II. Dependencies
- All dependencies must be explicitly declared (go.mod, package.json, requirements.txt, etc.)
- The app must never rely on implicit system-level packages or tools being pre-installed
- Check for vendored or pinned dependency versions to ensure reproducible builds

### III. Config
- Configuration that varies between environments (DB URLs, API keys, feature flags) must come from **environment variables**
- Flag any hardcoded connection strings, service URLs, ports, or credentials in source code
- Ensure there is a clear separation between code and config — no config files committed with secrets

### IV. Backing Services
- Databases, caches, message queues, and external APIs must be treated as **attached resources**
- Swapping a backing service (e.g., local PostgreSQL → managed RDS) should require only a config change, no code change
- Check that service connections use URLs or credentials from config, not hardcoded addresses

### V. Build, Release, Run
- Verify strict separation between build (compile), release (build + config), and run (execute) stages
- Flag any code that modifies itself at runtime or depends on build-time side effects

### VI. Processes
- The app should run as **stateless processes** — no local filesystem or in-memory session state between requests
- Any data that must persist across requests must go to a backing service (database, cache, object store)
- Flag sticky sessions, local file caches used as primary storage, or in-process state that would be lost on restart

### VII. Port Binding
- The app should be self-contained, exporting HTTP (or other protocol) by binding to a port
- Check that the app does not depend on an external web server (Apache, Nginx as runtime dependency)

### VIII. Concurrency
- Verify the app can scale horizontally by running multiple identical processes
- Flag any singleton patterns, global locks, or shared in-process state that would prevent horizontal scaling
- Check that work distribution (queue consumers, scheduled tasks) supports multiple competing workers

### IX. Disposability
- Processes should start fast and shut down gracefully (handle SIGTERM, drain connections)
- Check for proper graceful shutdown: finish in-flight requests, close DB connections, flush buffers
- Flag long startup sequences that would hinder rapid scaling or recovery

### X. Dev/Prod Parity
- Development, staging, and production environments should use the same backing services and versions
- Flag any "dev-only" code paths that skip validation, authentication, or use different storage backends
- Check for SQLite-in-dev/PostgreSQL-in-prod anti-patterns or similar divergences

### XI. Logs
- The app should write logs to **stdout/stderr** as an unbuffered event stream
- Flag any code that writes log files directly, manages log rotation, or creates log directories
- Log aggregation and storage is the platform's responsibility, not the app's

### XII. Admin Processes
- One-off admin tasks (DB migrations, data cleanup scripts) should run as isolated processes
- They must use the same codebase and config as the main application
- Flag admin operations embedded in the main process lifecycle that could affect availability

## Observability Review (MELT)

Evaluate the codebase against the **MELT framework** (Metrics, Events, Logs, Traces) 
and its integration principles to ensure the system is debuggable, monitorable, 
and diagnosable in production:

### Metrics
- Check that key operations expose quantitative measurements (request count, latency, error rate, queue depth)
- Verify that custom metrics use proper libraries (Prometheus, StatsD, OpenTelemetry) instead of ad-hoc counters
- Flag critical code paths that have no metrics instrumentation (payment processing, auth, external API calls)
- Ensure metrics follow naming conventions and include relevant labels/dimensions for filtering
- Check that metric cardinality is bounded — no unbounded label values (user IDs, request URLs)

### Events
- Check that high-level business and infrastructure events are emitted (deploys, feature toggles, config changes)
- Verify that events carry structured context: timestamp, actor, affected resource, before/after state
- Flag state transitions that happen silently without any observable event (cron triggers, background migrations)
- Ensure events are distinct from logs — they are high-signal, low-volume markers, not verbose debug output
- Check that events can be correlated with metrics on a timeline (to answer "what changed right before the error spike?")

### Logs
- Verify structured logging is used (JSON or key-value) instead of unstructured printf-style messages
- Check that log levels are used correctly: DEBUG for development, INFO for normal operations, WARN for recoverable issues, ERROR for failures
- Ensure every log entry includes correlation context: request ID, trace ID, user ID where applicable
- Flag excessive logging that would create noise in production (logging every request body, every SQL query)
- Flag insufficient logging on error paths — every catch/error handler should log with enough context to diagnose
- Verify that logs go to stdout/stderr (12-Factor XI) and never manage their own files or rotation
- Check that sensitive data is NEVER logged (passwords, tokens, PII) — ties back to the Security audit

### Distributed Tracing
- Check if the application propagates trace context (W3C Trace Context, B3 headers) across service boundaries
- Verify that key operations create spans with meaningful names and relevant attributes
- Flag outbound HTTP/gRPC calls that don't propagate the trace context to downstream services
- Check that database queries, cache operations, and queue publishes are instrumented as child spans
- Ensure error spans are properly annotated with error status and messages

### Context Propagation
- Verify that a correlation ID (trace ID or request ID) is injected at the system entry point (middleware, interceptor)
- Check that the correlation ID flows through all layers: HTTP headers, queue message attributes, gRPC metadata
- Flag any code path where context is lost: new goroutines spawned without context, fire-and-forget background jobs, async message handlers
- Ensure error responses include a request/correlation ID so users can reference it in support tickets
- Verify that the same ID connects metrics → logs → traces for a single request (the Correlation golden rule)

### OpenTelemetry Standardization
- Check if the project uses vendor-specific instrumentation SDKs (Datadog, New Relic, Jaeger clients) that create lock-in
- Recommend migration to OpenTelemetry (OTel) APIs/SDKs where applicable — instrument once, export anywhere
- Verify that OTel exporters (if present) are configured via environment variables, not hardcoded
- Check for mixed instrumentation: some code using OTel, other code using legacy vendor APIs — flag inconsistency
- Ensure OTel resource attributes are properly set (service.name, service.version, deployment.environment)

## General Code Quality

Beyond the principles above, also check for:

- **Architecture**: Spot structural improvements that align with the project's established patterns; recommend proven design patterns where they naturally simplify the code
- **Simplicity over cleverness**: If a solution requires a README to explain how to use it internally, it's too complex — simplify first, abstract later
- **Documentation**: Find undocumented or poorly documented components — create or update documentation in `docs/` (NOT in code comments). All explanations, specs, and guides MUST live in `docs/`, keeping the code clean and readable with only critical comments
- **Testing**: Identify untested or under-tested areas; improve test coverage where impactful — prioritize business logic: every domain rule, calculation, and state transition should have a dedicated unit test that proves correctness without external dependencies
- **Dependencies**: Flag outdated, deprecated, or vulnerable dependencies
- **Consistency**: Ensure coding style, naming conventions, and patterns are consistent throughout the codebase

### Performance
- Look for obvious performance improvements (unnecessary allocations, N+1 queries, blocking I/O)
- **Database Indexing** — whenever you create or modify data schemas, queries, or access patterns in ANY database (relational, document, graph, key-value, search engine), you MUST consider indexing:
  - Every frequent query pattern — filters, lookups, sorts, aggregations, joins, traversals — must be backed by an appropriate index. A missing index on a high-traffic access pattern turns O(log n) lookups into O(n) full scans, regardless of the database engine
  - **Design indexes based on actual query patterns**, not on the data model alone — understand how the application reads data and create indexes that serve those access patterns directly
  - For relational databases: consider composite indexes (leftmost prefix rule), covering indexes, and indexes on foreign key columns. For document stores: index fields used in filters and sorts. For graph databases: index node properties used in traversal start points. For search engines: ensure field mappings and analyzers match query expectations
  - Avoid **over-indexing**: every index carries a write-cost penalty (inserts, updates, and deletes become slower) and consumes storage — only index fields and patterns that are actually queried
  - For uniqueness constraints, prefer database-enforced unique indexes over application-level checks — the database enforces atomically and serves as documentation
  - Flag queries or access patterns that operate on unindexed fields, especially on collections expected to grow beyond thousands of records
  - Consider the database's native strengths: partition keys in distributed stores, shard keys in sharded clusters, TTL indexes for expiring data, full-text indexes for search — use the right indexing primitive for each use case
- **Algorithms & Data Structures** — prefer proven, well-understood structures over custom inventions:
  - Before implementing any non-trivial data processing, check if a **standard algorithm or data structure** already solves the problem: hash maps, B-trees, heaps, tries, bloom filters, LRU caches, topological sort, BFS/DFS, binary search, etc.
  - **Know the time and space complexity** of the structures you use — choosing an O(n²) approach when an O(n log n) alternative exists is a correctness issue, not just a style issue, at scale
  - Flag hand-rolled sorting, searching, deduplication, or graph traversal when the language's standard library already provides battle-tested implementations
  - Prefer **hash maps for lookups** (O(1) average) over linear scans of arrays/slices (O(n)) when the collection is non-trivial
  - When dealing with ordered data, use balanced trees, sorted arrays with binary search, or skip lists — not repeated linear searches
  - For problems involving shortest paths, dependency ordering, cycle detection, or connectivity — use the canonical graph algorithms (Dijkstra, Kahn's, Tarjan's, Union-Find) instead of ad-hoc recursive traversals
  - Flag O(n²) or worse algorithms operating on datasets that could realistically grow to thousands+ of elements — these are ticking time bombs even if they work today
- **Non-Blocking & Async I/O** — always prefer non-blocking, asynchronous execution for I/O-bound operations:
  - **Never block a thread waiting for I/O** (network calls, disk reads, database queries, external API requests) — use async/await, futures, promises, channels, event loops, or non-blocking I/O primitives provided by the language and runtime
  - Flag synchronous HTTP calls, blocking file reads, and sequential database queries inside request handlers — these waste threads and limit throughput under load
  - When multiple independent I/O operations are needed (e.g., fetching data from 3 services), execute them **concurrently**, not sequentially — the total latency should be the max of the individual calls, not the sum
  - Use non-blocking queues, event-driven architectures, and reactive streams for high-throughput data pipelines — blocking producers/consumers create backpressure bottlenecks
  - Ensure that async code properly propagates errors and cancellation — fire-and-forget patterns that silently swallow failures are a reliability risk
  - Flag callback-hell and deeply nested async chains — refactor into clean sequential-looking async/await or pipeline patterns for readability
- **Parallelism for CPU-Intensive Processing** — when the language/runtime supports true parallelism and the workload is compute-intensive, exploit parallel execution:
  - **Batch processing, data transformations, aggregations, and heavy computations** MUST be parallelized when the dataset size justifies it — processing millions of records sequentially on a multi-core machine is wasting hardware
  - Use the language's native parallelism primitives: goroutines + WaitGroup (Go), parallel streams (Java), multiprocessing/concurrent.futures (Python), worker threads (Node.js), Rayon (Rust), etc.
  - **Partition work into independent chunks** and process them in parallel — ensure each chunk operates on its own data to avoid shared-state contention and locks
  - Flag sequential loops over large datasets that perform CPU-bound work (parsing, encoding, hashing, image processing, ML inference) — these are prime candidates for parallelization
  - Set parallelism limits (worker pool size, semaphores) to avoid resource exhaustion — unbounded goroutine/thread creation under load causes memory spikes and scheduler thrashing
  - Distinguish between **I/O-bound** (use async) and **CPU-bound** (use parallelism) workloads — applying the wrong strategy wastes resources or adds unnecessary complexity
  - For pipelines that combine both (read data → transform → write), use a **fan-out/fan-in** or **pipeline pattern** where I/O stages are async and compute stages are parallelized
## Decision Validation Gate

**Before changing ANY code, you MUST verify that no prior decision justifies the current implementation.**

For every improvement you intend to make, check:
1. **Memories**: Search project and user memories for mentions of the module, pattern, or technology you want to change. Look for `tension`, `decision`, and `convention` types.
2. **ADRs**: Check `docs/decisions/` for an architectural decision record that explains why the current approach was chosen.
3. **Past reports**: Check if a previous analysis already documented a reason NOT to change it.
4. **Code comments**: Look for comments like `// NOTE:`, `// IMPORTANT:`, `// DECISION:` that explain intentional choices.

If you find a deliberate decision that justifies the current implementation:
- **Do NOT change it** — respect the decision
- Document that you reviewed it and why the decision stands
- If you believe the decision is outdated, recommend it for human review with your reasoning

Examples of things that might look "wrong" but are intentional:
- Using SQLite instead of PostgreSQL (might be a deliberate choice for embedded deployment)
- Not using OTel (might be a team decision to stick with a specific vendor)
- Duplicated code across modules (might be intentional to avoid coupling)
- Hardcoded values (might be compile-time constants for performance reasons)

## Dealing with Complex Problems

**When facing a complex, unfamiliar, or persistent problem, follow a disciplined approach: investigate systematically, rule out common causes, and escalate to internet search when needed.**

### Systematic Step-by-Step Investigation
- **When facing a large or complex problem, DO NOT try to solve everything at once** — break it down and investigate step by step to understand WHERE the problem actually is
- **Isolate variables**: change ONE thing at a time and observe the result. Making multiple changes simultaneously makes it impossible to know which change fixed (or broke) something
- **Trace the execution path**: start from the entry point and follow the data flow step by step — add logging, print statements, or breakpoints at each stage to see exactly where the behavior diverges from expectation
- **Narrow the scope progressively**: use binary search debugging — if the problem could be in 10 places, test the middle first to eliminate half the possibilities, then repeat
- **Validate assumptions explicitly**: don't assume a function returns what you think, a config value is what you expect, or a file exists where it should — verify each assumption with a concrete check
- **Reproduce first, fix second**: before attempting ANY fix, make sure you can reliably reproduce the problem. If you can't reproduce it, you can't confirm your fix works
- **Document what you've tried**: keep a mental (or written) log of each step, what you tested, and what the result was — this prevents going in circles and helps identify patterns

### Before You Debug — Is It Cache?
- **Before diving into complex debugging, ALWAYS ask yourself: could this be a cache problem?** — stale caches are one of the most common root causes of 'mysterious' bugs that waste hours of investigation
- Check for: **build cache** (stale compiled artifacts), **dependency cache** (old versions of packages still resolved), **browser cache** (serving outdated JS/CSS/HTML), **CDN/proxy cache** (edge nodes serving stale responses), **Docker layer cache** (old base images or intermediate layers), **IDE/LSP cache** (stale analysis, phantom errors), **DNS cache**, **ORM/query cache**, and **OS file system cache**
- Common symptoms of cache problems: 'it works on a clean machine but not here', code changes that don't seem to take effect, intermittent behavior that clears after restart, errors referencing functions/types that no longer exist in the source
- When in doubt, **clear the relevant cache and retry BEFORE investigating further** — a 30-second cache clear can save hours of debugging

### When to Search the Internet
- **Complex or unfamiliar errors**: When you encounter an error message, stack trace, or behavior that you cannot resolve after 2-3 attempts — search for the exact error message or symptoms online
- **Library/framework-specific issues**: When dealing with third-party library quirks, version incompatibilities, or undocumented behavior — the solution is often in a GitHub issue, Stack Overflow answer, or official documentation
- **Known problems with known solutions**: Many programming problems are well-documented. Before spending time reinventing a solution, check if someone has already solved it
- **Configuration and environment issues**: Build failures, dependency conflicts, toolchain problems, and platform-specific bugs are frequently documented online with precise fixes
- **API changes and deprecations**: When a previously working approach suddenly fails, search for recent API changes, breaking updates, or migration guides
- **Uncertainty about the correct approach**: When you are NOT sure how to implement something, which API to use, or what the best practice is — **search first, then code**. Guessing wastes time and produces fragile solutions; a 30-second search grounds your decision in real-world usage and official guidance
- **Unfamiliar territory**: When working with a technology, tool, or domain you have limited knowledge about — do NOT improvise. Search for official documentation, tutorials, and canonical examples before writing a single line of code
- **Before building from scratch**: Before implementing non-trivial functionality yourself, **search for existing, battle-tested libraries** that already solve the problem. Reinventing the wheel wastes time, introduces bugs, and ignores years of community-hardened edge-case handling. Search for 'best [language] library for [problem]' and evaluate maturity (stars, maintenance activity, download count) before committing to a dependency — or deciding to build your own

### How to Search Effectively
- **Search with the exact error message** (in quotes) — this yields the most precise results
- **Include the technology stack** in the query (e.g., 'Go', 'React', 'PostgreSQL') to filter irrelevant results
- **Prioritize official documentation** and reputable sources (official docs, GitHub issues, Stack Overflow) over blog posts
- **Cross-reference multiple sources** before applying a solution — a single answer may be outdated or context-specific
- **Check the date** of the solution — programming ecosystems evolve rapidly, and a 3-year-old answer may no longer be accurate

### Anti-patterns
- ❌ **Do NOT keep retrying the same approach** in circles — if you've tried 3+ variations without progress, stop and search
- ❌ **Do NOT assume you know the answer** to every problem from model knowledge alone — your training data has a cutoff, and new bugs, patches, and best practices emerge constantly
- ❌ **Do NOT ignore search results** because they seem too simple — often the fix for a complex-seeming problem IS a one-line change documented in a GitHub issue
- ❌ **Do NOT spend more than 15 minutes debugging** a problem that has common symptoms before searching — time is a resource, and internet search is your highest-ROI escalation tool

## Applying Changes

- Apply safe, non-breaking changes that improve the codebase
- Prefer small, focused improvements over large refactors
- Always maintain backward compatibility
- Update documentation to match any code changes
- Follow the project's established patterns and conventions
- **Workspace Hygiene**: ALWAYS remove any temporary files, scratch scripts, or experimental code created to test a hypothesis, validate an idea, or assist during the task. Do not leave unversioned artifacts behind that are not part of the final system.
- Prioritize fixes by severity: **security > concurrency > 12-factor > observability > correctness > quality > style**

## Post-Task Reflection & Knowledge Generation

**After completing ALL code changes, you MUST perform a mandatory reflection phase.**
This is NOT optional — the reflection phase is as important as the code changes themselves.
It is the mechanism through which the system learns, evolves, and accumulates institutional knowledge.

### Step 1: Reflect on What You Did

Before creating any artifacts, take a moment to deeply reflect on the session:

- **What patterns did you encounter?** — Were there recurring code structures, naming conventions, or architectural decisions that shaped your work?
- **What surprised you?** — Did anything contradict your expectations? Was the codebase better or worse than anticipated in specific areas?
- **What decisions did you make and why?** — Document the trade-offs you evaluated, even for decisions that seem obvious. Future agents won't have your context.
- **What did you NOT change and why?** — Equally important: what did you consider changing but deliberately left alone? What was the reasoning?
- **What was harder than expected?** — Were there areas where the code's complexity or coupling made improvements difficult? These are signals of deeper structural issues.
- **What recurring workflows did you observe?** — Did you notice processes, sequences of operations, or patterns that the team repeats across the codebase or across tasks?
- **What business rules or domain logic did you discover?** — Were there implicit business rules embedded in the code that are not documented anywhere?

### Step 2: Review & Update Memories

Memories are the system's long-term knowledge. After reflecting, you MUST:

#### 2a. Review Existing Memories
- Read ALL project memories: `graphit memory search ""` (empty search lists all)
- For each memory, ask: **Is this still accurate given what I just did?**
- If a memory is now outdated or contradicted by your changes, **update it immediately**:
  ```bash
  graphit memory delete <old-id>
  graphit memory insert "<updated title>" --type <type> --content "<corrected body>"
  ```
- Confirm: "Updated memory: '<title>' — reason: <why it changed>"

#### 2b. Create New Memories from Inferences
Based on your reflection, create new memories for any of the following:

| Type | When to Create | Example |
|------|---------------|---------|
| `convention` | You discovered an implicit pattern the team follows consistently | "Error handling uses wrapped errors with `fmt.Errorf` and `%w` verb" |
| `decision` | You made (or discovered) a deliberate architectural choice | "Chose file-based storage over SQLite for memory persistence — portability over features" |
| `tension` | You identified a trade-off or unresolved conflict in the codebase | "Module X tightly couples to module Y but they serve different concerns" |
| `skill` | You learned a non-obvious technique specific to this project | "AST graph requires reindexing after any file rename or move" |
| `correction` | You corrected a misunderstanding or common mistake | "Don't use `go build` for testing — always `make install` first" |

For each new memory:
```bash
graphit memory insert "<descriptive title>" --type <type> --content "<detailed body explaining the insight, context, and reasoning>"
```
- Confirm: "Memorized: '<title>'"
- **Minimum**: Create at least ONE new memory per session — if you learned nothing, you weren't paying attention

### Step 3: Identify Recurring Patterns & Codify Them

Look for recurring workflows, coding patterns, and business processes that could be **codified as reusable artifacts**.
The goal is to transform tacit knowledge into explicit, shareable tools.

#### What to Look For

- **Recurring coding patterns**: Do you keep applying the same transformation, fix, or refactor across different parts of the codebase? → **Codify as a Skill**
- **Repeated multi-step processes**: Do certain tasks always require the same sequence of commands or operations? → **Codify as a Command**
- **Common review criteria**: Are there project-specific quality checks that should always be enforced? → **Codify as a Rule**
- **Business domain logic**: Are there domain-specific patterns, validations, or workflows that need to be understood to work correctly? → **Codify as Knowledge**
- **Integration patterns**: Does the project interact with external systems in a specific way that needs to be documented? → **Codify as Knowledge**
- **Agent workflows**: Are there multi-step agent behaviors that could be orchestrated more efficiently? → **Codify as an Agent or Workflow**

#### How to Codify

For each pattern you identify, create the artifact **directly in the IDE's artifact directory**.
Use the `graphit hub type-path` command to resolve the correct path for the current IDE:

**Step 3a: Resolve the artifact path**

```bash
# Get the path where the artifact should be created
graphit hub type-path <type> <artifact-name>
```

This returns the exact path where the artifact should live. Examples:

```bash
$ graphit hub type-path skill error-handling-patterns
/path/to/project/.agents/skills/error-handling-patterns

$ graphit hub type-path rule no-direct-db-access
/path/to/project/.agents/rules/no-direct-db-access.md

$ graphit hub type-path command pre-deploy-check
/path/to/project/.agents/workflows/pre-deploy-check.md
```

**Step 3b: Create the artifact content**

For **file-based types** (rule, command, agent) — write directly to the returned path:

```bash
ARTIFACT_PATH=$(graphit hub type-path rule no-direct-db-access)
cat > "$ARTIFACT_PATH" << 'EOF'
# No Direct DB Access Outside Repository Layer

All database access MUST go through the repository layer.
Never import the database package directly from handlers or services.
...
EOF
```

For **folder-based types** (skill) — create the directory and write SKILL.md inside:

```bash
ARTIFACT_PATH=$(graphit hub type-path skill error-handling-patterns)
mkdir -p "$ARTIFACT_PATH"
cat > "$ARTIFACT_PATH/SKILL.md" << 'EOF'
# Error Handling Patterns

## Purpose
Codifies the project's wrapped error pattern with context enrichment.

## Instructions
<Step-by-step guidance for the agent>

## Examples
<Concrete examples showing the pattern in action>
EOF
```

**Step 3c: Report created artifacts to the user**

After creating artifacts, inform the user:

```
📦 Created IDE Artifacts:

  1. [skill] error-handling-patterns
     → Codifies the project's wrapped error pattern with context enrichment
     Path: .agents/skills/error-handling-patterns/

  2. [rule] no-direct-db-access
     → Enforces repository layer as the only DB access point
     Path: .agents/rules/no-direct-db-access.md

To submit to the Hub (optional):
  graphit hub submit <artifact-id> <path> --type <type> --version 1.0.0
```

#### Artifact Type Guide

| Type | Mode | Use Case | Example |
|------|------|----------|---------|
| `skill` | folder | Teach the agent HOW to do something project-specific | Error handling patterns, testing methodology |
| `command` | file | Automate a repeatable sequence of operations | Pre-deploy checks, database migration flow |
| `rule` | file | Enforce a constraint or quality gate | "No direct DB access", "All inputs validated server-side" |
| `agent` | file | Define a multi-step autonomous workflow | Code review agent, security auditor |
| `workflow` | file | Orchestrate multiple agents or skills | Full CI pipeline, incident response |

### Step 4: Update Existing Artifacts (When Applicable)

If the project already has IDE artifacts (skills, rules, commands), check whether your session's insights warrant **updating** them:

- Look in the IDE artifact directories for existing artifacts
- If an existing skill/rule is incomplete or could benefit from what you learned, **update it directly**
- Report what you changed in the Reflection Summary

To submit updates to the Hub:
```bash
graphit hub submit <artifact-id> <path> --type <type> --version <new-version>
```

### Step 5: Reflection Summary

At the end of your response, include a **Reflection Summary** section:

```
## 🪞 Reflection Summary

### Memories
- Created: <count> new memories
  - "<title 1>" (type: <type>)
  - "<title 2>" (type: <type>)
- Updated: <count> existing memories
  - "<title>" — reason: <why>
- Deleted: <count> stale memories

### IDE Artifacts
- Created: <count>
  - [<type>] <name> — <one-line description>
- Updated: <count>
  - [<type>] <name> — <what changed>

### Key Insights
- <Insight 1: the most important thing you learned>
- <Insight 2: a pattern worth watching>
- <Insight 3: a risk or concern for the future>
```

**If you have nothing to reflect on, nothing to memorize, and no patterns to codify — state that explicitly.**
An empty reflection is acceptable; a skipped reflection is NOT.

