# RuLECO Coordinator

The coordinator is a central component in the RuLECO system that routes messages between different components and coordinators.

## Architecture

The coordinator follows a hexagonal (ports and adapters) architecture pattern. See [ADR 1](../docs/adr/ruleco-coordinator/0001-hexagonal-architecture.md) for rationale.

```
┌─────────────────────────────────────────────┐
│          Application Layer (app)            │
└─────────────────┬───────────────────────────┘
┌─────────────────▼───────────────────────────┐
│             Domain Layer (core)             │
└─────────────────┬───────────────────────────┘
┌─────────────────▼───────────────────────────┐
│            Transport Layer                  │
│  - MessagePort, ConnectionManagementPort    │
│  - Implementations: ZmqAdapter, MockAdapter │
└─────────────────────────────────────────────┘
```

**Core**: `CoordinatorCore` — pure routing logic, independent of implementation details.

**Ports**: `MessagePort`, `ConnectionManagementPort`, `RoutingPort`, `DirectoryPort`, `ClockPort`.

**Adapters**: `ZmqAdapter`, `MockAdapter`, `InMemoryDirectoryAdapter`, `SystemClockAdapter`.

## Transport Layer: Identity Enum

Messages are tagged with an `Identity` enum (`Local`/`Remote`/`SelfTarget`) indicating their transport origin, which determines how routing validates them. See [ADR 2](../docs/adr/ruleco-coordinator/0002-identity-enum-for-transport-context.md) for rationale.

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
