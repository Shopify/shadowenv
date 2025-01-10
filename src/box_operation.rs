#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct BoxOperation {
    operation: String,
    path: String,
}

impl BoxOperation {
    pub fn new(operation: String, path: String) -> Self {
        BoxOperation { operation, path }
    }
}
