# 3. End-to-End Testing with ruleco-actor

Date: 2026-05-23

## Status

Accepted

## Context

The Coordinator currently has two levels of testing:

1. **Unit tests** (in-source, using `MockAdapter`/`InMemoryDirectoryAdapter`) — test domain logic like routing decisions, directory management, and JSON-RPC dispatch in isolation.
2. **Integration tests** (`tests/`) — use `TestCoordinator` (real `CoordinatorApp` in a thread) + `TestClient` (raw ZMQ DEALER socket) to test through real ZMQ sockets.

The gap: `TestClient` is a low-level socket wrapper that can send and receive frames, but it **cannot act autonomously**. It cannot:
- Receive and respond to incoming JSON-RPC requests (e.g., coordinator-initiated pings)
- Run an event loop that dispatches incoming messages to handlers
- Act as a protocol-compliant LECO Component/Actor

This means we cannot test:
- Coordinator→component requests (ping, remote procedure calls)
- Component→component RPC via the coordinator
- Heartbeat/timeout from the component side
- The full actor lifecycle as specified in the control protocol

Additionally, there is no reusable Component/Actor implementation in the workspace — every consumer of the Coordinator would need to build their own ZMQ+JSON-RPC stack from scratch.

The control protocol defines a Component type hierarchy with mandatory and optional methods (`pong`, `shut_down`, `get_parameters`, `set_parameters`, `call_action`, etc.). A test component should implement these methods to be protocol-compliant.

### Alternatives Considered

1. **Extend `TestClient` in `tests/common.rs`** — add event-loop behavior to the existing test utility. Stays in test code, can't evolve into a real actor library. Test utility becomes complex. Not reusable outside tests.

2. **New `ruleco-actor` crate only** — a proper library crate implementing a LECO Component/Actor. Clean separation, reusable foundation, but doesn't test the shipped binary.

3. **CLI subprocess black-box tests only** — start `ruleco-coordinator` binary via `std::process::Command`. Truest black-box but slowest, complex cleanup, hard to orchestrate multi-coordinator scenarios.

4. **Hybrid: `ruleco-actor` crate + CLI smoke tests** (chosen) — get the best of both: fast in-process tests with `TestCoordinator` + `TestActor` for most scenarios, plus a few CLI smoke tests for binary-level confidence.

## Decision

We will create a new workspace crate `ruleco-actor` and add CLI smoke tests for the coordinator binary.

### `ruleco-actor` Crate

A library crate implementing a protocol-compliant LECO Component/Actor with:

- **ZMQ DEALER transport** — connects to a Coordinator's ROUTER socket
- **Sign-in/sign-out lifecycle** — follows the control protocol handshake
- **Event loop** — receives messages, dispatches JSON-RPC requests to registered handlers
- **Built-in handlers** for Component/Actor methods:
  - `pong` (mandatory Component method)
  - `shut_down` (optional Component method)
  - `get_parameters` / `set_parameters` (mandatory Actor methods)
  - `call_action` dispatch to registered action handlers
- **Custom method registration** — `register_method()` for arbitrary LECO-compatible methods
- **`call()` method** — send JSON-RPC requests to other components via the coordinator
- **`spawn()` / `step()`** — run the event loop in a background thread or step manually

### `TestActor`

A convenience type extending `Actor` with:
- `get_test` / `set_test` custom methods backed by a shared state store
- `TestActorHandle` with convenience methods for remote state access
- Suitable for coordinator E2E tests where you need a component that holds and returns state

### CLI Smoke Tests

A small set of tests that start the `ruleco-coordinator` binary as a subprocess and verify it starts, binds, and responds to sign-in over ZMQ. These catch CLI parsing, config loading, and binary packaging issues that in-process tests cannot.

### Test Levels After This Change

| Level | Tool | Scope |
|-------|------|-------|
| Unit | `MockAdapter`, `rstest` | Domain logic (routing, directory) |
| Integration | `TestCoordinator` + `TestClient` | Low-level protocol edge cases (malformed frames, raw ZMQ) |
| E2E | `TestCoordinator` + `TestActor`/`TestActorHandle` | Full protocol scenarios (sign-in, routing, RPC, heartbeat) |
| Smoke | `std::process::Command` | Binary starts, binds, responds |

### Existing Tests

- Unit tests and `MockAdapter` tests remain unchanged.
- Integration tests with `TestClient` remain for low-level protocol edge cases.
- E2E tests are **new**, not replacing existing tests.
- The 3 currently-ignored timeout tests can be unblocked once `TestActor` responds to coordinator pings.

## Consequences

- A reusable `ruleco-actor` crate becomes the foundation for all future LECO actors and components.
- Coordinator behavior can be tested from the outside with protocol-compliant actors, regardless of internal implementation changes.
- `TestActor` makes writing E2E tests natural: spawn coordinator, spawn actor, call methods, assert responses.
- Two test modes to maintain (in-process E2E + CLI smoke), but the smoke test set is small.
- The `ruleco-actor` crate's public API will need careful design — it becomes a public interface for LECO consumers.
- Adding a new workspace member increases build time slightly, but `ruleco-actor` shares `ruleco-core` dependencies.
