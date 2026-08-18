#[derive(Debug, Clone)]
pub struct Fail {
    pub id: String,
    pub error: String,
}

#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub succeeded: Vec<String>,
    pub failed: Vec<Fail>,
}

impl Summary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ok(id: impl Into<String>) -> Self {
        Self {
            succeeded: vec![id.into()],
            failed: Vec::new(),
        }
    }

    pub fn err(id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            succeeded: Vec::new(),
            failed: vec![Fail {
                id: id.into(),
                error: error.into(),
            }],
        }
    }

    pub fn merge(&mut self, other: Summary) {
        self.succeeded.extend(other.succeeded);
        self.failed.extend(other.failed);
    }

    pub fn is_ok(&self) -> bool {
        self.failed.is_empty()
    }

    pub fn total(&self) -> usize {
        self.succeeded.len() + self.failed.len()
    }
}
