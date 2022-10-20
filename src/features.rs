use std::fmt;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Feature {
    name: String,
    version: Option<String>,
}

impl Feature {
    pub fn new(name: String, version: Option<String>) -> Self {
        Feature { name, version }
    }
}

impl fmt::Display for Feature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.version {
            Some(v) => write!(f, "{name}:{version}", name = self.name, version = v),
            None => write!(f, "{}", self.name),
        }
    }
}
