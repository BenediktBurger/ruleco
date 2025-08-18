//! Core domain entities, services, errors

pub mod component_entry;
pub mod coordinator_entry;
pub mod routing_decision;

pub use component_entry::ComponentEntry;
pub use coordinator_entry::CoordinatorEntry;
pub use routing_decision::RoutingDecision;
