# Measure S3 traversal on WAN/cold cache and evaluate HTTP_CACHE_FILE opt-in with evidence

# Real WAN/cold-cache benchmark of on-the-fly traversal via S3 and opt-in evaluation of the httpfs cache

## Context
The 3-hop traversal over Icebug mounted directly from S3 came in at 429 ms (commit fcaa9d7) against MinIO LOCALHOST (`http://localhost:9000`). The on-the-fly requirement was validated without explicit download/rebuild, but LAN latency and cold object reads were never measured.

## Constraint that remains
`CALL HTTP_CACHE_FILE=TRUE` (docs.ladybugdb.com/extensions/httpfs/) must NOT become the default: the cache downloads the entire remote file, is visible only during the transaction and discarded on commit/rollback — each planner expansion is a separate query/autocommit, so it can repeatedly download files and worsen exactly the latency it was meant to improve.

## What to do
1. Benchmark against a REAL distant bucket (not localhost), measuring cold start (mount + first query) and warm (subsequent queries), separately recording the catalog mount cost.
2. Only with that number in hand, test `HTTP_CACHE_FILE=TRUE` as an opt-in EXPERIMENT (dedicated environment variable/config), comparing cold/warm with and without cache, including the cost of re-download between autocommits.
3. If the cache wins with evidence, document the chosen default in docs/specs/hub_collaboration.md and record the decision in memory; if it loses, record why it was discarded.

## How to know it worked
- There is a pair of cold/warm numbers from a distant bucket in the task log, and any decision about the cache is justified by them, not by assumption.
- The default path (without cache) remains the behavior of the installed binary.
