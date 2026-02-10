# RuLECO

Rust implementation of the [Laboratory Experiment COntrol (LECO) protocol](https://github.com/pymeasure/leco-protocol). For a fully featured implementation (in Python), see [PyLECO](https://github.com/pymeasure/pyleco).

**Note**: LECO is still under development, such that the code and API might change.
The LECO protocol branch [pyleco-state](https://github.com/pymeasure/leco-protocol/tree/pyleco-state) contains the assumptions used in this project, which are not yet accepted into the LECO main branch.
See this [documentation](https://leco-laboratory-experiment-control-protocol--69.org.readthedocs.build/en/69/) for the LECO definitions including these assumptions.
These things might change, if LECO defines them differently.

You are welcome to contribute, especially commenting on code improvements, as this is my first contact with Rust.

## Structure

This repository is a workspace and contains several crates.

- _ruleco-core_ contains some useful elements to create LECO applications in Rust, e.g. a `Message` struct.
- _ruleco-coordinator_ contains a Coordinator implementation. Supports configuration via TOML config file (see [CONFIG.md](CONFIG.md)).
- _ruleco-legacy_ contains my initial trials of ruleco code.

## Coordinator Architecture

The coordinator follows a hexagonal (ports and adapters) architecture pattern:

### Core Components

1. **CoordinatorCore** (`ruleco-coordinator/src/core/coordinator_core.rs`)
   - Contains the pure business logic for message routing
   - Uses dependency injection for ports (DirectoryPort, ClockPort)
   - Makes routing decisions based on message destinations and directory information
   - Independent of any specific implementation details (ZMQ, storage, etc.)

2. **Ports** (`ruleco-coordinator/src/core/ports/`)
    - Define interfaces for external dependencies:
      - `MessagePort`: Interface for sending and receiving messages (handles local/remote/error responses)
      - `ConnectionManagementPort`: Interface for transport connection lifecycle management
      - `DirectoryPort`: Interface for component/coordinator directory management
      - `ClockPort`: Interface for time-related operations

3. **Adapters** (`ruleco-coordinator/src/adapters/`)
    - Implement the port interfaces with concrete technologies:
      - `ZmqAdapter`: Implements `MessagePort` and `ConnectionManagementPort` using ZeroMQ sockets
      - `InMemoryDirectoryAdapter`: Implements `DirectoryPort` with in-memory storage
      - `SystemClockAdapter`: Implements `ClockPort` using system time

4. **Application Layer** (`ruleco-coordinator/src/app.rs`)
   - Orchestrates the components
   - Contains the main event loop that polls for messages
   - Uses the adapters to communicate with the outside world
   - Translates routing decisions from CoordinatorCore into actual message sends

### Testing Strategy

For testing the coordinator's message handling and routing:

1. **Unit Testing CoordinatorCore**
    - Test the `route_message` method directly with various message scenarios
    - Use mock implementations of `DirectoryPort` and `ClockPort` to control test conditions
    - Verify correct `RoutingDecision` is returned for different message types

2. **Testing with Mocked MessagePort**
    - Use `mockall` crate to generate mock implementations of `MessagePort`
    - Test that the coordinator sends the correct messages to the right destinations
    - Isolate testing of routing logic from actual network communication

3. **Integration Testing**
    - Test with real ZMQ sockets for end-to-end verification
    - Verify the complete message flow from client to coordinator to destination

### Key Design Principles

1. **Dependency Inversion**: Core logic depends on abstractions (ports), not concrete implementations
2. **Single Responsibility**: Each component has a well-defined responsibility
3. **Testability**: Core logic can be tested independently of external systems
4. **Extensibility**: New implementations of ports can be added without changing core logic
