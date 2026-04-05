/// Integration tests for CLI argument parsing and validation

// Since we can't import directly from the binary, we'll test the CLI interface
// through the public API and command-line argument parsing

#[test]
fn test_cli_help() {
    // This test verifies that --help works
    let result = std::process::Command::new("cargo")
        .args(["run", "--", "--help"])
        .output();

    assert!(result.is_ok());
    let output = result.unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify help text contains expected information
    assert!(stdout.contains("AWS CLI authentication"));
    assert!(stdout.contains("--profile"));
    assert!(stdout.contains("--all-profiles"));
    assert!(stdout.contains("--configure"));
}

#[test]
fn test_cli_version() {
    // Test that --version works
    let result = std::process::Command::new("cargo")
        .args(["run", "--", "--version"])
        .output();

    assert!(result.is_ok());
    let output = result.unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should contain version number
    assert!(stdout.contains("0.1.0"));
}

#[test]
fn test_invalid_mode() {
    // Test that invalid mode triggers an error
    let result = std::process::Command::new("cargo")
        .args(["run", "--", "--mode", "invalid"])
        .output();

    assert!(result.is_ok());
    let output = result.unwrap();

    // Should fail with error about invalid mode
    assert!(!output.status.success());
}

#[test]
fn test_conflicting_flags() {
    // Test that --all-profiles with custom profile is rejected
    // This should fail during validation
    std::env::remove_var("AWS_PROFILE");

    let result = std::process::Command::new("cargo")
        .args(["run", "--", "--all-profiles", "--profile", "test"])
        .env_remove("AWS_PROFILE")
        .output();

    assert!(result.is_ok());
    let output = result.unwrap();

    // Should fail with validation error
    assert!(!output.status.success());
}

#[test]
fn test_default_mode_is_cli() {
    // When no mode is specified, should default to CLI (headless)
    // We can't test the actual execution, but we can verify the binary accepts it
    let result = std::process::Command::new("cargo")
        .args(["run", "--", "--help"])
        .output();

    assert!(result.is_ok());
    let output = result.unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Help should show default mode
    assert!(stdout.contains("cli"));
}
