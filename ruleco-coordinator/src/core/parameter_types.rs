//! Parameter types for methods

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
pub struct AddNodesParams {
    pub nodes: HashMap<String, String>,
}
