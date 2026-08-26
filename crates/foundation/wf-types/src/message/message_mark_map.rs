use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageMarkMap {
    pub marks: HashMap<String, Vec<super::super::Id>>,
}
