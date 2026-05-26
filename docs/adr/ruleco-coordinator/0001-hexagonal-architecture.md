# 1. Hexagonal Architecture for the Coordinator

Date: 2026-05-23

## Status

Accepted

## Context

The Coordinator is a central message router in the RuLECO system. It must:
- Route messages between local components and remote coordinators
- Manage a directory of connected components
- Handle different transport mechanisms (ZMQ sockets)
- Be testable without requiring actual network infrastructure
- Remain adaptable if transport or storage technologies change

Early development mixed routing logic with ZMQ-specific code, making it difficult to test routing decisions in isolation and tightly coupling the domain to a specific transport.

## Decision

The Coordinator follows a hexagonal (ports and adapters) architecture with three layers:

1. **Domain Layer** (`core/`) — pure business logic with no external dependencies. Contains `CoordinatorCore` for routing decisions and entity types.

2. **Ports** (`core/ports/`) — trait interfaces defining what the domain needs from the outside world:
   - `MessagePort` — send and receive messages, returning `Identity` to indicate source context
   - `ConnectionManagementPort` — manage transport connections (bind, connect, disconnect)
   - `RoutingPort` — route messages based on sender/receiver names and transport context
   - `DirectoryPort` — manage the component/coordinator directory with identity tracking
   - `ClockPort` — time-related operations

3. **Adapters** (`adapters/`) — concrete implementations of ports:
   - `ZmqAdapter` — `MessagePort` + `ConnectionManagementPort` via ZeroMQ
   - `MockAdapter` — same ports for testing without network
   - `InMemoryDirectoryAdapter` — `DirectoryPort` with in-memory storage
   - `SystemClockAdapter` — `ClockPort` using system time

4. **Application Layer** (`app.rs`) — orchestrates components, runs the main event loop, and translates `CoordinatorCore` routing decisions into actual message sends via adapters.

## Consequences

- Core routing logic is fully testable with mock implementations, no ZMQ or network required
- New transport mechanisms (e.g. TLS, different message brokers) can be added as new adapters without changing core logic
- Directory storage can be swapped (e.g. persistent storage) by implementing `DirectoryPort`
- The `Identity` enum (Local/Remote/SelfTarget) is a transport-layer concept that flows into routing decisions — this is an intentional leak that allows the domain to distinguish between trust levels of message sources
- More files and indirection than a monolithic approach, but each piece has a clear responsibility
