use std::fmt;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct Feature {
    name: String,
    version: Option<String>,
}

impl Feature {
    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[allow(dead_code)]
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_new() {
        let f1 = Feature::new("rust".to_string(), Some("1.70.0".to_string()));
        assert_eq!(f1.name(), "rust");
        assert_eq!(f1.version(), Some("1.70.0"));

        let f2 = Feature::new("python".to_string(), None);
        assert_eq!(f2.name(), "python");
        assert_eq!(f2.version(), None);
    }

    #[test]
    fn test_feature_display() {
        let f1 = Feature::new("rust".to_string(), Some("1.70.0".to_string()));
        assert_eq!(f1.to_string(), "rust:1.70.0");

        let f2 = Feature::new("python".to_string(), None);
        assert_eq!(f2.to_string(), "python");
    }

    #[test]
    fn test_feature_clone() {
        let f1 = Feature::new("rust".to_string(), Some("1.70.0".to_string()));
        let f2 = f1.clone();
        assert_eq!(f1, f2);
        assert_eq!(f1.name(), f2.name());
        assert_eq!(f1.version(), f2.version());
    }

    #[test]
    fn test_feature_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();

        let f1 = Feature::new("rust".to_string(), Some("1.70.0".to_string()));
        let f2 = Feature::new("rust".to_string(), Some("1.70.0".to_string()));
        let f3 = Feature::new("rust".to_string(), Some("1.71.0".to_string()));

        set.insert(f1.clone());
        assert!(set.contains(&f2));
        assert!(!set.contains(&f3));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_feature_empty_strings() {
        let f1 = Feature::new("".to_string(), Some("".to_string()));
        assert_eq!(f1.name(), "");
        assert_eq!(f1.version(), Some(""));
        assert_eq!(f1.to_string(), ":");

        let f2 = Feature::new("".to_string(), None);
        assert_eq!(f2.name(), "");
        assert_eq!(f2.version(), None);
        assert_eq!(f2.to_string(), "");
    }
}
