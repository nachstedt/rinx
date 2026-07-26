use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetLocation {
    Internal(String), // doc_path
    External(String), // URL
}
