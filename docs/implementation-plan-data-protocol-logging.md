# Implementation Plan: Data Protocol Logging for Coordinator

## Goal

Enable the Coordinator to publish log entries over the data protocol (ZMQ PUB/SUB),
so that subscribers (e.g. a Recorder component) can receive structured log messages
over the network.

## Context

- Current logging: `log` facade + `env_logger` backend, stderr-only
- Data protocol spec: `docs/data_protocol.md` (topic + 17-byte header + JSON content)
- Log message format (spec §Log message content): JSON array `["asctime", "levelname", "name", "text"]`
- Legacy `DataMessage`/`DataPublisher` in `ruleco-legacy` is rough and will be replaced
- Architecture: hexagonal (Core → Ports → Adapters)

## Phases

### Phase 1: Data protocol types in `ruleco-core`

Add shared protocol types, independent of ZMQ, following the pattern of existing
`message.rs` / `protocol_constants.rs`.

**Files to create:**

- `ruleco-core/src/data_message.rs`
  - `DataMessage` struct: `topic: Vec<u8>`, `header: [u8; 17]`, `payload: Vec<Vec<u8>>`
  - `DataMessage::new(topic: &str, message_type: DataMessageType, content: Vec<Vec<u8>>)` — generates a new `ConversationId`
  - `DataMessage::with_conversation_id(...)` — for test vectors
  - `DataMessage::into_frames(self) -> Vec<Vec<u8>>`
  - `DataMessage::conversation_id() -> &[u8]`
  - `DataMessage::message_type() -> DataMessageType`
  - `DataHeader` struct (16-byte conversation_id + 1-byte message_type) with `from_slice` / `to_bytes`
  - Parse constructor `DataMessage::from_frames(frames: Vec<Vec<u8>>) -> Result<Self, DataMessageError>`

- `ruleco-core/src/log_record.rs`
  - `LogRecord` struct: `asctime: String`, `levelname: LogLevel`, `name: String`, `text: String`
  - `LogLevel` enum: `DEBUG`, `INFO`, `WARNING`, `ERROR`, `CRITICAL` (matching Python logging levels per spec)
  - `LogLevel::from_log_level(log::Level)` — mapping: Trace→DEBUG, Debug→DEBUG, Info→INFO, Warn→WARNING, Error→ERROR
  - `LogRecord::from_log_record(record: &log::Record)` — convenience constructor
  - `LogRecord::to_json_bytes() -> Vec<u8>` — serialize as `["2025-04-24 12:00:00","INFO","name","text"]`
  - `LogRecord::from_json_bytes(bytes: &[u8]) -> Result<Self, LogRecordError>`

- `ruleco-core/src/protocol_constants.rs` — add:
  - `DATA_HEADER_SIZE: usize = 17`
  - `DataMessageType` enum with `Undefined = 0`, `Json = 1` (values 2–127 reserved, 128–255 user-defined)
  - Implement `From<DataMessageType> for u8` and `From<u8> for DataMessageType`

- `ruleco-core/src/lib.rs` — add `pub mod data_message;` and `pub mod log_record;`

- `ruleco-core/Cargo.toml` — add `log = "0.4"` dependency (needed for `LogLevel::from_log_level`)

**Tests:**

- DataMessage frame construction roundtrip
- DataMessage spec test vector: topic "N1.Recorder", conversation_id `0190a2b3c4d5e6f7a8b9c0d1e2f3a4b5`, message_type 1 → verify frames match spec wire format
- LogRecord serialization: verify `["2025-04-24 12:00:00","INFO","recorder","Measurement started"]`
- LogRecord deserialization roundtrip
- LogLevel mapping from `log::Level`

### Phase 2: DataProtocolPublisher adapter in `ruleco-coordinator`

A hexagonal adapter that wraps a ZMQ PUB socket for publishing data protocol messages.

**Files to create:**

- `ruleco-coordinator/src/adapters/data_publisher_adapter.rs`
  - `DataPublisherAdapter` struct:
    - `socket: zmq::Socket`
    - `topic: String` (Coordinator's full name, e.g. "N1.Coordinator")
  - `DataPublisherAdapter::new(topic: &str, xsub_addr: &str) -> Result<Self, DataPublisherError>`
    - Creates `zmq::Context`, `zmq::PUB` socket, connects to `xsub_addr`
  - `DataPublisherAdapter::publish(&self, message: &DataMessage) -> Result<(), DataPublisherError>`
    - Calls `socket.send_multipart(message.into_frames(), 0)`
  - `DataPublisherAdapter::publish_log(&self, record: &LogRecord) -> Result<(), DataPublisherError>`
    - Constructs `DataMessage` with `message_type=Json`, serializes `LogRecord` as content, calls `publish`
  - `DataPublisherError` enum: `Zmq`, `Serialization`

- `ruleco-coordinator/src/adapters.rs` — add `pub mod data_publisher_adapter;` and re-export

**Tests:**

- Unit test: `publish_log` constructs correct frames (mock the ZMQ socket or test frame construction only)

### Phase 3: Custom `log::Log` implementation in `ruleco-coordinator`

Replace `env_logger::init()` with a custom logger that dispatches to both stderr
and the data protocol publisher.

**Files to create:**

- `ruleco-coordinator/src/logging.rs`
  - `DualLogger` struct implementing `log::Log`:
    - `stderr_logger: env_logger::Logger` (built from env_logger Builder)
    - `publisher: Option<DataPublisherAdapter>` (behind `Mutex` for thread safety)
    - `stderr_level: log::LevelFilter`
    - `publish_level: log::LevelFilter`
  - `DualLogger::new(stderr_logger: env_logger::Logger, publisher: Option<DataPublisherAdapter>, publish_level: log::LevelFilter) -> Self`
  - `log::Log` impl:
    - `enabled()` — returns true if level meets either filter
    - `log()` — dispatch to stderr if level >= stderr_level; dispatch to publisher if level >= publish_level and publisher is Some
    - `flush()` — flush stderr logger
  - `init_logger(config: &LoggingConfig)` — creates and installs the global logger
  - `LoggingConfig` struct:
    - `stderr_level: log::LevelFilter`
    - `publish_level: log::LevelFilter`
    - `data_publisher_addr: Option<String>` — None = no network publishing
    - `topic: String` — Coordinator full name for data protocol topic

- `ruleco-coordinator/src/lib.rs` — add `pub mod logging;`

**Tests:**

- Unit test: DualLogger with no publisher writes to stderr only
- Unit test: DualLogger with mock publisher dispatches to both sinks
- Unit test: Level filtering works independently for stderr vs publish

### Phase 4: CLI configuration

Wire the new logging into the Coordinator's CLI and config.

**Files to modify:**

- `ruleco-coordinator/src/main.rs`
  - Add CLI args:
    - `--data-publisher-addr <ADDR>` — XSUB address of data proxy (enables log publishing)
    - `--log-publish-level <LEVEL>` — min level for network publishing (default: "info")
  - Replace `env_logger::Builder::...init()` with `logging::init_logger(config)`
  - Build `LoggingConfig` from CLI args

- `ruleco-coordinator/src/config.rs`
  - Add to `CoordinatorConfig`:
    - `data_publisher_addr: Option<String>`
    - `log_publish_level: log::LevelFilter`
  - Add to `ConfigFile` (if applicable)
  - Add `apply_cli_overrides` handling for new fields

**Tests:**

- Verify CLI args parse correctly
- Verify defaults: no data publisher addr → no network publishing; publish level defaults to info

### Phase 5: Remove legacy data protocol code

**Files to modify:**

- `ruleco-legacy/src/data_protocol.rs` — delete file
- `ruleco-legacy/src/lib.rs` — remove `pub mod data_protocol;` and the `ContentTypes` re-export from `core`
- `ruleco-legacy/src/main.rs` — update if it references data_protocol

### Phase 6: Integration tests

**Files to create/modify:**

- `ruleco-core/tests/` or inline `#[cfg(test)]` — verify DataMessage + LogRecord wire format matches spec test vector exactly
- `ruleco-coordinator/tests/` — integration test: start coordinator with data publisher, verify log frames published

## Design decisions

1. **`log` crate, not `tracing`** — sufficient for this use case, consistent with existing codebase
2. **Keep `env_logger` for stderr** — preserves existing formatting and `RUST_LOG` support
3. **Independent log levels** — network publishing typically at higher level than stderr to avoid flooding
4. **`set_log_level` RPC** controls stderr level only; publish level is startup-configured
5. **Data protocol types in `ruleco-core`** — they are protocol definitions, shared across crates
6. **ZMQ adapter in coordinator only** — not in core (core has no ZMQ dependency)
7. **Drop legacy data protocol** — rough implementation, replaced by proper core types
8. **`subpackage.rs` convention** — new files follow existing naming convention

## Dependencies to add

- `ruleco-core/Cargo.toml`: `log = "0.4"`
- `ruleco-coordinator/Cargo.toml`: no new deps (already has `zmq`, `log`, `env_logger`)
