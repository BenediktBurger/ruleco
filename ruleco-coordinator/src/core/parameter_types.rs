//! Parameter types for methods

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parameters for add_nodes method
#[derive(Serialize, Deserialize, Debug)]
pub struct AddNodesParams {
    /// Map of namespaces to their coordinator addresses (e.g., "N2" -> "tcp://127.0.0.1:5555")
    pub nodes: HashMap<String, String>,
}
