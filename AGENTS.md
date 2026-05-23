# RuLECO

## General instructions

- You can find documentation about structure and architectural decisinos in the global `README.md` or in the package `README.md` files.
- document relevant architecture decisions as ADRs in `docs/adr/` (workspace-wide) or `docs/adr/<crate-name>/` (crate-specific). Subdirectories are created on demand. See `docs/adr/0001-record-architecture-decisions.md` for format and conventions.
- document relevant packages in the appropriate `README.md` or in this file, as applicable.
- For the definition of the control protocol we implement here, see `docs/control_protocol.md`
- If I ask for guidance, maybe with a suggestion, give a honest response, whether that is a good idea or not.
- the `docs/schemas` folder contains OpenRPC definitions of methods

## Coding style

- Use imperative mode for comments according to Rust style
- use `subpackage.rs` instead of `subpackage/mod.rs` for subpackages

## JSON-RPC handling

- Use `jsonrpsee_types` crate for JSON-RPC (prefer [`Request::owned()`](ruleco-coordinator/src/jsonrpc_handler.rs:5)/[`Request::borrowed()`](ruleco-coordinator/src/jsonrpc_handler.rs:5) over `serde_json::json!()`)
- Use [`Id`](ruleco-coordinator/src/jsonrpc_handler.rs:9) enum for request IDs (supports `Number(u64)`, `Str`, and `Null`)
- Use [`Response`](ruleco-coordinator/src/jsonrpc_handler.rs:8) and [`ResponsePayload`](ruleco-coordinator/src/jsonrpc_handler.rs:8) for creating responses

## Instructions

- Use the orchestrator skill process: planner → implementer → reviewer loop for each phase
- Follow TDD (Test-Driven Development): write tests before/with implementation
- Follow ruleco architecture patterns: hexagonal architecture (Core → Ports → Adapters)
- Use `jsonrpsee_types` for JSON-RPC, MessageBuilder/MessageView for messages

## Relevant files / directories

- `docs/control_protocol.md` - Protocol specification
- `docs/schemas/` - OpenRPC method definitions (component, actor, coordinator)
- `ruleco-coordinator/src/jsonrpc_handler.rs` - JSON-RPC handlers including `handle_remove_expired_addresses`
- `ruleco-coordinator/src/core/parameter_types.rs` - Parameter types including `RemoveExpiredAddressesParams`

## Next steps

1. Phase 6 complete - all implementable tests passing
2. Create `ruleco-actor` crate (see `docs/implementation-plan-ruleco-actor.md`)
3. E2E tests with `TestActor` + CLI smoke tests
4. Remaining 3 ignored tests require heartbeat/timeout infrastructure:
   - Active background task for periodic timeout checking
   - Coordinator-initiated ping mechanism
   - Bidirectional coordinator timeout detection
5. These features are documented as TODO in app.rs:77
