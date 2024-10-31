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
    fn test_run_with_complex_shadowenv_mutations() {
        let (dir, path) = setup_test_dir();
        
        // Test multiple mutations including set, unset, and append
        let shadowenv_data = r#"{
            "version": 1,
            "mutations": [
                {"op":"set","name":"TEST_SET","value":"set_value"},
                {"op":"append","name":"TEST_APPEND","value":"append_value"},
                {"op":"unset","name":"TEST_UNSET"}
            ]
        }"#;
        
        // Set a variable that should be unset
        env::set_var("TEST_UNSET", "should_be_removed");
        // Set a variable that should be appended to
        env::set_var("TEST_APPEND", "original_");
        
        let result = run(path, shadowenv_data.to_string(), vec!["nonexistent_command"]);
        
        // Verify all mutations were applied
        assert_eq!(env::var("TEST_SET").unwrap(), "set_value");
        assert_eq!(env::var("TEST_APPEND").unwrap(), "original_append_value");
        assert!(env::var("TEST_UNSET").is_err());
        
        // Cleanup
        env::remove_var("TEST_SET");
        env::remove_var("TEST_APPEND");
        
        assert!(result.is_err());
        drop(dir);
    }

    #[test]
    fn test_run_with_unicode_env_vars() {
        let (dir, path) = setup_test_dir();
        
        let shadowenv_data = r#"{
            "version": 1,
            "mutations": [
                {"op":"set","name":"TEST_UNICODE","value":"🦀 rust"}
            ]
        }"#;
        
        let result = run(path, shadowenv_data.to_string(), vec!["nonexistent_command"]);
        
        assert_eq!(env::var("TEST_UNICODE").unwrap(), "🦀 rust");
        
        env::remove_var("TEST_UNICODE");
        assert!(result.is_err());
        drop(dir);
    }

    #[test]
    fn test_run_with_path_modifications() {
        let (dir, path) = setup_test_dir();
        
        // Save original PATH
        let original_path = env::var("PATH").unwrap_or_default();
        
        let shadowenv_data = r#"{
            "version": 1,
            "mutations": [
                {"op":"set","name":"PATH","value":"/test/bin:/usr/bin"}
            ]
        }"#;
        
        let result = run(path, shadowenv_data.to_string(), vec!["nonexistent_command"]);
        
        // Verify PATH was modified
        assert_eq!(env::var("PATH").unwrap(), "/test/bin:/usr/bin");
        
        // Restore original PATH
        env::set_var("PATH", original_path);
        
        assert!(result.is_err());
        drop(dir);
    }

    #[test]
    fn test_run_with_empty_shadowenv_data() {
        let (dir, path) = setup_test_dir();
        
        // Test with minimal valid shadowenv data that makes no mutations
        let shadowenv_data = r#"{
            "version": 1,
            "mutations": []
        }"#;
        
        let result = run(path, shadowenv_data.to_string(), vec!["nonexistent_command"]);
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No such file"));
        drop(dir);
    }

    #[test]
    fn test_run_with_invalid_version() {
        let (dir, path) = setup_test_dir();
        
        let shadowenv_data = r#"{
            "version": 999,
            "mutations": []
        }"#;
        
        let result = run(path, shadowenv_data.to_string(), vec!["echo", "test"]);
        
        assert!(result.is_err());
        // The exact error message might vary depending on your version handling,
        // adjust the contains string accordingly
        assert!(result.unwrap_err().to_string().contains("version"));
        drop(dir);
    }

    #[test]
    fn test_run_with_special_chars_in_env() {
        let (dir, path) = setup_test_dir();
        
        let shadowenv_data = r#"{
            "version": 1,
            "mutations": [
                {"op":"set","name":"TEST_SPECIAL","value":"value with spaces and $pecial ch@rs"},
                {"op":"set","name":"TEST_QUOTES","value":"value \"with\" 'quotes'"}
            ]
        }"#;
        
        let result = run(path, shadowenv_data.to_string(), vec!["nonexistent_command"]);
        
        assert_eq!(env::var("TEST_SPECIAL").unwrap(), "value with spaces and $pecial ch@rs");
        assert_eq!(env::var("TEST_QUOTES").unwrap(), "value \"with\" 'quotes'");
        
        env::remove_var("TEST_SPECIAL");
        env::remove_var("TEST_QUOTES");
        
        assert!(result.is_err());
        drop(dir);
    }

    #[test]
    fn test_run_with_pathlist_operations() {
        let (dir, path) = setup_test_dir();
        
        // Test pathlist operations (append/prepend/remove)
        let shadowenv_data = r#"{
            "version": 1,
            "mutations": [
                {"op":"set","name":"TEST_PATH","value":"/initial/path"},
                {"op":"append","name":"TEST_PATH","value":"/appended/path"},
                {"op":"prepend","name":"TEST_PATH","value":"/prepended/path"}
            ]
        }"#;
        
        let result = run(path, shadowenv_data.to_string(), vec!["nonexistent_command"]);
        
        // Verify pathlist operations were applied correctly
        assert_eq!(env::var("TEST_PATH").unwrap(), "/prepended/path:/initial/path:/appended/path");
        
        env::remove_var("TEST_PATH");
        assert!(result.is_err());
        drop(dir);
    }

    #[test]
    fn test_run_with_no_clobber() {
        let (dir, path) = setup_test_dir();
        
        // Set up initial environment
        env::set_var("PROTECTED_VAR", "original_value");
        
        // Create shadowenv data with no_clobber flag
        let shadowenv_data = r#"{
            "version": 1,
            "mutations": [
                {"op":"set","name":"PROTECTED_VAR","value":"new_value","no_clobber":true}
            ]
        }"#;
        
        let result = run(path, shadowenv_data.to_string(), vec!["nonexistent_command"]);
        
        // Verify protected variable wasn't modified
        assert_eq!(env::var("PROTECTED_VAR").unwrap(), "original_value");
        
        env::remove_var("PROTECTED_VAR");
        assert!(result.is_err());
        drop(dir);
    }

    #[test]
    fn test_run_with_multiple_shadowenv_data() {
        let (dir, path) = setup_test_dir();
        
        // First set of mutations
        let shadowenv_data1 = r#"{
            "version": 1,
            "mutations": [
                {"op":"set","name":"TEST_VAR","value":"initial_value"}
            ]
        }"#;
        
        // Run first command
        let _ = run(path.clone(), shadowenv_data1.to_string(), vec!["nonexistent_command"]);
        
        // Second set of mutations
        let shadowenv_data2 = r#"{
            "version": 1,
            "mutations": [
                {"op":"set","name":"TEST_VAR","value":"updated_value"}
            ]
        }"#;
        
        // Run second command
        let result = run(path, shadowenv_data2.to_string(), vec!["nonexistent_command"]);
        
        // Verify final state
        assert_eq!(env::var("TEST_VAR").unwrap(), "updated_value");
        
        env::remove_var("TEST_VAR");
        assert!(result.is_err());
        drop(dir);
    }

    #[test]
    fn test_run_with_features() {
        let (dir, path) = setup_test_dir();
        
        // Create shadowenv data that includes features
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
        
        // We can't directly test features as they're internal to Shadowenv,
        // but we can verify the environment variable was set
        assert_eq!(env::var("TEST_VAR").unwrap(), "test_value");
        
        env::remove_var("TEST_VAR");
        assert!(result.is_err());
        drop(dir);
    }
}
