---
title: Captured WebSocket transport
content-type: reference
audience: developers
status: focused-linux-qualified
updated: 2026-10-12
references:
  - target: {type: task, id: tsk-bcf2}
    relation: implemented_by
  - target: {type: task, id: tsk-4970}
    relation: implemented_by
  - target: {type: task, id: tsk-60d0}
    relation: supports
  - target: {type: task, id: tsk-72bc}
    relation: derived_from
---

# Captured WebSocket transport

`graphit_http::websocket` provides a native WebSocket connection for the Runtime
Now consumer. It uses the same private constructor as the
[bounded HTTP client](bounded_http_transport.md): captured origin, Rustls trust,
explicit proxy and additional DER roots. HTTP/1 only applies to the WebSocket
client. The buffered and streaming HTTP APIs keep their existing behavior.

The transport does not authenticate a principal or grant PROJECT access. The
consumer retains its actual Auth selection, verifier and borrowed credential
lease and checks them around protected operations. Remote DATA is not authority.
This transport's tests do not qualify Runtime Now, the Broker's SQL authorization
or installed agent hosts.

## Connection and limits

`WebSocketClient::new(ClientPolicy, Limits, WebSocketLimits)` captures the policy
once. `WebSocketLimits` supplies positive frame payload, reassembled message and
outgoing payload byte caps. Incoming frame and complete-message bounds are
configured in the mature SDK before application decoding. They are separate:
several permitted fragments can still exceed the complete-message limit.

`connect(destination, headers, NetworkRequestContext)` accepts an HTTP(S)
destination in the captured origin. It rejects userinfo, fragments, other origins
and overrides of managed upgrade or proxy headers. Authorization must be unique;
caller header values are marked sensitive. Canonical Host and a default Accept
when absent are added before logical admission. The header budget also includes
the SDK's four HTTP/1 upgrade headers and its 24-byte nonce. The SDK performs the
handshake; Graphit does not implement framing, TLS, proxy tunnels or nonce hashing.

Ambient proxies, redirects, retries, referer and decompression stay disabled.
Only the captured proxy is used. HTTPS verifies the hostname and certificate with
platform trust plus the captured roots. The explicit literal-loopback HTTP
exception remains a constructor policy input. There is no public raw-client
injection, environment lookup, trust bypass or fallback transport.

These caps bound payloads and Graphit's logical headers. They do not guarantee
process RSS or bound every vendor DNS, HTTP parser, TLS or allocator buffer.
Response header admission follows vendor parsing. The SDK write buffer is derived
with checked arithmetic from the outgoing frame/control size and eager writes;
it is not an application queue or a resource reservation system.

## One Original and one socket owner

`BoundedWebSocket` borrows the cancellation signal and optional absolute deadline
supplied at connection. Ordinary requests retain `Some(Instant)`; a continuous
service uses `NetworkRequestContext::service` with no artificial deadline.
`send`, `receive`, `receive_until` and consuming `close` cannot replace that context.
Every await and successful return checks the same Original. Cancellation ends a
service socket too. Signed credential expiry and the consumer's actual admission
remain independent; a heartbeat or read wakeup never extends either lifetime.

`receive()` waits for native data, close, cancellation or an actual Original deadline.
It adds no timer. `receive_until(wakeup)` adds only the caller's real negotiated
heartbeat wakeup while retaining the same socket and Original.

`send(Message)` checks the outgoing payload cap. `receive_until(wakeup)` returns
`WebSocketRead::Message`, `Wakeup` or `Closed`. Wakeup lets the Now consumer check
its real admission and send its negotiated heartbeat. Pending SDK read state
stays with the same socket, including partial fragmented messages. An elapsed
wakeup takes priority over queued messages, so inbound traffic cannot starve the
consumer's heartbeat. Close returns
the peer's finite code and ends that socket. `close(CloseCode)` consumes the owner
and sends a bounded native close with no application reason string.

Each operation takes the socket out of the owner before its first await and
restores it only after success and a final Original check. An error, cancellation,
deadline or abandoned operation future therefore ends the owner. A failed or
abandoned send drops any SDK DATA still buffered; it does not attempt another
close or flush. Bytes already sent are not rolled back. After termination, another
operation returns `Closed`; explicit recovery needs a new connection and the
consumer's current real Auth capture.

The owner spawns no jobs and has no Arc cycle. Dropping it drops its native socket;
there is no Graphit worker to join. Internal HTTP library upgrade/resolver tasks
retain the vendor's lifecycle, which this API does not claim to own.

## Errors and qualification

`WebSocketError` distinguishes Original/network rejection, terminal closure and
an actual SDK cause. `WebSocketSdkCause` retains that cause privately and exposes
only a finite category: handshake, transport, protocol, capacity or closed.
An abrupt TCP reset without a Close handshake and recognized connection I/O
failures map to Closed. Invalid frames, reserved opcodes and invalid UTF-8 remain
Protocol; TLS trust and hostname failures remain Transport. This distinction
allows the continuous owner to decide whether a new operation is appropriate.
The SDK never reconnects or reuses a failed socket. The buffered/streaming HTTP
client retains the same conservative distinction as NetworkError::Connection
versus Transport, and Broker admission preserves it. Generic Transport alone
does not identify a recoverable connection loss.
Debug, Display and the exposed `Error::source` chain use the redacted wrapper;
raw SDK URLs, header values, protocol strings and driver messages do not escape.
`WebSocketSdkCause::http_status()` exposes only the actual numeric status retained
by an SDK HTTP failure, such as 401, 403, 503 or 504 before upgrade. A malformed
101 handshake has no failing HTTP status; the accessor returns `None`. Consumers
can distinguish admission denial from service unavailability using this metadata
without examining diagnostic strings or exposing raw SDK causes.
`WebSocketRead` has no Debug implementation. Consumers must project received
messages according to their own wire and privacy contracts.

The [implementation](../../crates/graphit-http/src/websocket.rs) and
[native transport tests](../../crates/graphit-http/tests/websocket.rs) cover SDK
TCP handshakes, fragmented reads across wakeup/heartbeat, distinct frame/message
bounds, Original expiry/cancellation, operation abandonment, pending send cleanup,
TLS roots/hostname and captured versus ambient proxy selection. The ambient proxy
child probe is executed by its parent fixture; it changes only the child process
environment. Focused Linux qualification passed 32 existing HTTP functional tests
and 11 WebSocket functional tests. Each suite's child-only proxy probe is ignored
by the default harness and executed by its passing parent test. Current scoped
strict Clippy, formatting, diff and local-link checks passed; the affected negative
handshake fixture passed again after its read-byte assertion changed. The doc-test
command passed with zero tests and is not additional functional evidence. Final
Tracker review found no remaining Source/documentation fault. Receipts and earlier
failed attempts are retained under `.build/rust-runtime-websocket-4970/`. Commands:

```text
cargo test --locked --offline -p graphit-http --lib --test websocket
cargo clippy --locked --offline -p graphit-http --lib --tests --no-deps -- -D warnings
```

Primary SDK sources: [reqwest-websocket 0.6.0](https://docs.rs/reqwest-websocket/0.6.0/reqwest_websocket/)
and [tungstenite 0.28 WebSocketConfig](https://docs.rs/tungstenite/0.28.0/tungstenite/protocol/struct.WebSocketConfig.html).
The Runtime consumer in `tsk-60d0` must use this actual API and the Broker's
admitted subscribe/welcome/heartbeat protocol. No Runtime, non-Linux or native
agent qualification is inferred from this transport library.
