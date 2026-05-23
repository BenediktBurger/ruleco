# 2. Identity Enum for Transport-Layer Context

Date: 2026-05-23

## Status

Accepted

## Context

The Coordinator receives messages from two distinct sources via ZeroMQ:
- **ROUTER socket**: connected local components. ZMQ assigns identity frames to each peer, but anyone can connect — the identity alone does not prove the component is who it claims to be.
- **DEALER sockets**: connections to remote coordinators. These are established by the coordinator itself, so the peer is already authenticated by virtue of the connection.

Without distinguishing between these sources at the routing level, the coordinator cannot:
1. Validate local components against the directory (prevent spoofing)
2. Trust remote coordinators (avoid redundant authentication)
3. Handle self-targeted messages correctly

## Decision

Introduce an `Identity` enum that tags every incoming message with its transport context:

```rust
pub enum Identity {
    Local { identity: Vec<u8> },   // From ROUTER socket, requires directory validation
    Remote { identity: Vec<u8> },  // From DEALER socket, already authenticated
    SelfTarget,                     // Internal loopback
}
```

`MessagePort::recv()` returns this enum alongside the message, so routing logic in `CoordinatorCore` can make trust decisions based on transport origin.

## Consequences

- Routing logic can enforce different validation rules for local vs. remote messages
- The domain layer is aware of a transport-level concept (`Identity`), which is a deliberate compromise — full isolation would require the adapter to handle validation, but then routing logic could not make context-dependent decisions
- Any new transport adapter must populate `Identity` correctly, which is an implicit contract
