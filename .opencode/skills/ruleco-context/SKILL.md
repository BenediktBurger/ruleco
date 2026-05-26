---
name: ruleco-context
description: Provide RuLECO architecture and coding patterns when auto-detected
---

## What I do

Provide RuLECO patterns when auto-detected in ruleco-coordinator/ or ruleco-core/

## Key patterns

- Hexagonal architecture: Core (business logic) → Ports (interfaces) → Adapters (implementations)
- Naming: `subpackage.rs` not `mod.rs`
- JSON-RPC: `jsonrpsee_types` crate (Request, Id, Response, ResponsePayload)
- Messages: MessageBuilder for creating, MessageView for zero-copy inspection
- Testing: mockall for mocks, unit tests for core, integration tests for full stack
- Code style: imperative comments, concise error messages

## Helpful commands

- for tests run `cargo test`, `cargo clippy`

## When used

Auto-loaded by other skills when RuLECO code is detected (file paths in ruleco-coordinator/ or ruleco-core/)
