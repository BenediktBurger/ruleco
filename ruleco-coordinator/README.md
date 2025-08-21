# RuLECO Coordinator

The coordinator is a central component in the RuLECO system that routes messages between different components and coordinators.

## Architecture

The coordinator follows a hexagonal (ports and adapters) architecture pattern:

### Core Logic

- **CoordinatorCore**: Contains the pure business logic for message routing, independent of any specific implementation details.

### Ports (Interfaces)

- **MessageSenderPort**: Interface for sending messages
- **DirectoryPort**: Interface for component/coordinator directory management
- **ClockPort**: Interface for time-related operations

### Adapters (Implementations)

- **ZmqAdapter**: Implements MessageSenderPort and MessageReceiverPort using ZeroMQ sockets and handles message polling
- **InMemoryDirectoryAdapter**: Implements DirectoryPort with in-memory storage
- **SystemClockAdapter**: Implements ClockPort using system time

## Testing

To test the coordinator's message handling and routing:

1. **Unit Testing CoordinatorCore**
   - Test the `route_message` method directly with various message scenarios
   - Use mock implementations of `DirectoryPort` and `ClockPort`

2. **Testing with Mocked MessageSenderPort**
   - Use `mockall` crate to generate mock implementations
   - Isolate testing of routing logic from actual network communication

3. **Integration Testing**
   - Test with real ZMQ sockets for end-to-end verification