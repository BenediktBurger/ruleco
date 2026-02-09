# RuLECO Coordinator

The coordinator is a central component in the RuLECO system that routes messages between different components and coordinators.

## Architecture

The coordinator follows a hexagonal (ports and adapters) architecture pattern:

```
┌─────────────────────────────────────────────┐
│          Application Layer (app)            │
│  - Coordinator lifecycle management         │
│  - Request/response handling                │
│  - Uses ports defined below                 │
└─────────────────┬───────────────────────────┘
                  │
┌─────────────────▼───────────────────────────┐
│             Domain Layer (core)             │
│  - Routing logic (RoutingPort)              │
│  - Directory management (DirectoryPort)     │
│  - Error definitions                        │
│  - Entity types (FullName, entries, etc.)   │
└─────────────────┬───────────────────────────┘
                  │
┌─────────────────▼───────────────────────────┐
│            Transport Layer                  │
│  - MessagePort: send/recv with Identity     │
│  - ConnectionManagementPort: socket mgmt    │
│  - Implementations: ZmqAdapter, MockAdapter │
└─────────────────────────────────────────────┘
```

### Core Logic

- **CoordinatorCore**: Contains the pure business logic for message routing, independent of any specific implementation details.

### Ports (Interfaces)

- **MessagePort**: Interface for sending and receiving messages from ZMQ sockets. Returns `Identity` enum indicating message source context (Local/Remote/SelfTarget).
- **ConnectionManagementPort**: Interface for managing network connections (bind ROUTER, connect/disconnect DEALER).
- **RoutingPort**: Interface for routing messages based on sender/receiver names and transport context.
- **DirectoryPort**: Interface for component/coordinator directory management with identity tracking.
- **ClockPort**: Interface for time-related operations.

### Adapters (Implementations)

- **ZmqAdapter**: Implements `MessagePort` and `ConnectionManagementPort` using ZeroMQ sockets. Handles socket polling and identity tracking.
- **MockAdapter**: Implements same ports for testing without network dependencies.
- **InMemoryDirectoryAdapter**: Implements `DirectoryPort` with in-memory storage.
- **SystemClockAdapter**: Implements `ClockPort` using system time.

## Transport Layer: Identity Enum

The `Identity` enum provides critical transport-layer context for routing decisions:

```rust
pub enum Identity {
    Local { identity: Vec<u8> },      // From ROUTER socket, requires validation
    Remote { identity: Vec<u8> },     // From DEALER socket, already authenticated
    SelfTarget,                        // Internal loopback
}
```

**Why this matters:**

- **ROUTER socket** adds identity frames for connected components → `Identity::Local`
  - These components MUST be validated against directory (sign-in check, identity match)
- **DEALER sockets** connect to remote coordinators → `Identity::Remote`
  - Remote coordinators are already authenticated via DEALER connection
  - No validation needed - bypass local namespace checks
- **Internal** loopback can target coordinator itself → `Identity::SelfTarget`

Without this enum distinction, routing logic cannot properly:

1. Validate local components (prevent spoofing)
2. Trust remote coordinators (avoid duplicate authentication)
3. Handle self-targeted messages correctly

## Protocol Layering

```
┌────────────────────────────────────┐
│   Domain Protocol (MessageView)    │  ← ruleco_core crate
│   - Sender/receiver names          │
│   - JSON-RPC payload               │
├────────────────────────────────────┤
│   Transport Protocol (raw frames)  │  ← adapter layer
│   - ZMQ identity frames            │
│   - Message envelope               │
├────────────────────────────────────┤
│   Transport Layer (ZMQ sockets)    │  ← ZMQ library
│   - ROUTER socket (client auth)    │
│   - DEALER sockets (coordinator)   │
└────────────────────────────────────┘
```

## Configuration

The coordinator can be configured using a TOML config file. See [../CONFIG.md](../CONFIG.md) for detailed configuration options.

### Quick Start

To use a config file, create `ruleco.toml` in your project directory:

```toml
[coordinator]
namespace = ""  # Use hostname automatically
timeout_interval = 10
```

The coordinator will automatically load the config when started:

```bash
cargo run -p ruleco-coordinator
```

### Namespace Auto-Detection

For lab environments with one coordinator per machine, use `namespace = ""` to automatically use the machine's hostname. This avoids namespace conflicts without manual configuration.

### Config File Locations

Config is loaded from the first existing file in this order:

1. `./ruleco.toml` (current directory)
2. `~/.config/ruleco/config.toml` (XDG config home)

If no config file is found, sensible defaults are used (namespace: `Default_Namespace`).

### Settings

| Setting            | Type   | Default             | Description                                                         |
|--------------------|--------|---------------------|---------------------------------------------------------------------|
| `namespace`        | String | `Default_Namespace` | The coordinator\'s namespace. Use `""` to auto-detect from hostname |
| `timeout_interval` | u32    | `10`                | Timeout interval in seconds for device communication checks         |

## Testing

To test the coordinator's message handling and routing:

1. **Unit Testing CoordinatorCore**
   - Test the `route_message` method directly with various message scenarios
   - Use `Identity::Local { identity }` pattern for test fixtures
   - Use mock implementations of `DirectoryPort` and `ClockPort`

2. **Testing with MockAdapter**
   - Use `MockAdapter` to simulate transport layer without network
   - Test Identity enum handling with `add_local_message_raw()` and `add_remote_message_raw()`
   - Isolate testing of routing logic from actual network communication

3. **Integration Testing**
   - Test with real ZMQ sockets for end-to-end verification
   - Verify Identity tagging from ROUTER vs DEALER sockets
