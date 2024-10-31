use crate::hook;
use anyhow::Error;
use std::path::PathBuf;
use std::vec::Vec;

/// Execute the provided command (argv) after loading the environment from the current directory
pub fn run(pathbuf: PathBuf, shadowenv_data: String, argv: Vec<&str>) -> Result<(), Error> {
    if let Some(shadowenv) = hook::load_env(pathbuf, shadowenv_data, true)? {
        hook::mutate_own_env(&shadowenv)?;
    }

    // exec only returns if it was unable to start the new process, and it's always an error.
    let err = exec::Command::new(argv[0]).args(&argv[1..]).exec();
    Err(err.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;
    use std::fs;

    fn setup_test_dir() -> (TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        (dir, path)
    }

    #[test]
    fn test_run_with_no_shadowenv_file() {
        let (dir, path) = setup_test_dir();
        let result = run(path, String::new(), vec!["echo", "test"]);
        assert!(result.is_err());
        // Should fail because exec failed, not because of shadowenv loading
        assert!(result.unwrap_err().to_string().contains("No such file"));
        drop(dir);
    }

    #[test]
    fn test_run_with_invalid_shadowenv_data() {
        let (dir, path) = setup_test_dir();
        let result = run(path, "invalid{json".to_string(), vec!["echo", "test"]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid"));
        drop(dir);
    }

    #[test]
    fn test_run_with_valid_shadowenv_data() {
        let (dir, path) = setup_test_dir();
        
        // Create valid shadowenv data that sets an environment variable
        let shadowenv_data = r#"{"version":1,"mutations":[{"op":"set","name":"TEST_VAR","value":"test_value"}]}"#;
        
        // We expect this to fail with exec error, but the environment should be modified
        let result = run(path, shadowenv_data.to_string(), vec!["nonexistent_command"]);
        
        // Verify environment was modified before exec attempt
        assert_eq!(env::var("TEST_VAR").unwrap(), "test_value");
        
        // Verify exec failed as expected
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No such file"));
        
        env::remove_var("TEST_VAR");
        drop(dir);
    }

    #[test]
    fn test_run_with_empty_argv() {
        let (dir, path) = setup_test_dir();
        let result = run(path, String::new(), vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("index out of bounds"));
        drop(dir);
    }

    #[test]
    fn test_run_preserves_existing_env() {
        let (dir, path) = setup_test_dir();
        
        // Set an existing environment variable
        env::set_var("EXISTING_VAR", "original_value");
        
        let shadowenv_data = r#"{"version":1,"mutations":[{"op":"set","name":"TEST_VAR","value":"test_value"}]}"#;
        
        let result = run(path, shadowenv_data.to_string(), vec!["nonexistent_command"]);
        
        // Verify both variables are present
        assert_eq!(env::var("EXISTING_VAR").unwrap(), "original_value");
        assert_eq!(env::var("TEST_VAR").unwrap(), "test_value");
        
        env::remove_var("EXISTING_VAR");
        env::remove_var("TEST_VAR");
        
        assert!(result.is_err());
        drop(dir);
    }

    #[test]
    fn test_run_with_pathlist_operations() {
        let (dir, path) = setup_test_dir();
        
        let shadowenv_data = r#"{
            "version": 1,
            "mutations": [
                {"op":"set","name":"TEST_PATH","value":"/initial/path"},
                {"op":"append","name":"TEST_PATH","value":"/appended/path"},
                {"op":"prepend","name":"TEST_PATH","value":"/prepended/path"}
            ]
        }"#;
        
        let result = run(path, shadowenv_data.to_string(), vec!["nonexistent_command"]);
        
        assert_eq!(env::var("TEST_PATH").unwrap(), "/prepended/path:/initial/path:/appended/path");
        
        env::remove_var("TEST_PATH");
        assert!(result.is_err());
        drop(dir);
    }

    #[test]
    fn test_run_with_no_clobber() {
        let (dir, path) = setup_test_dir();
        
        env::set_var("PROTECTED_VAR", "original_value");
        
        let shadowenv_data = r#"{
            "version": 1,
            "mutations": [
                {"op":"set","name":"PROTECTED_VAR","value":"new_value","no_clobber":true}
            ]
        }"#;
        
        let result = run(path, shadowenv_data.to_string(), vec!["nonexistent_command"]);
        
        assert_eq!(env::var("PROTECTED_VAR").unwrap(), "original_value");
        
        env::remove_var("PROTECTED_VAR");
        assert!(result.is_err());
        drop(dir);
    }

    #[test]
    fn test_run_with_multiple_shadowenv_data() {
        let (dir, path) = setup_test_dir();
        
        let shadowenv_data1 = r#"{
            "version": 1,
            "mutations": [
                {"op":"set","name":"TEST_VAR","value":"initial_value"}
            ]
        }"#;
        
        let _ = run(path.clone(), shadowenv_data1.to_string(), vec!["nonexistent_command"]);
        
        let shadowenv_data2 = r#"{
            "version": 1,
            "mutations": [
                {"op":"set","name":"TEST_VAR","value":"updated_value"}
            ]
        }"#;
        
        let result = run(path, shadowenv_data2.to_string(), vec!["nonexistent_command"]);
        
        assert_eq!(env::var("TEST_VAR").unwrap(), "updated_value");
        
        env::remove_var("TEST_VAR");
        assert!(result.is_err());
        drop(dir);
    }

    #[test]
    fn test_run_with_features() {
        let (dir, path) = setup_test_dir();
        
        let shadowenv_data = r#"{
            "version": 1,
            "mutations": [
                {"op":"set","name":"TEST_VAR","value":"test_value"}
            ],
            "features": [
                {"name": "test_feature", "version": "1.0"},
                {"name": "another_feature"}
            ]
        }"#;
        
        let result = run(path, shadowenv_data.to_string(), vec!["nonexistent_command"]);
        
        assert_eq!(env::var("TEST_VAR").unwrap(), "test_value");
        
        env::remove_var("TEST_VAR");
        assert!(result.is_err());
        drop(dir);
    }
}
