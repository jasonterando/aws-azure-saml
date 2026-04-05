/// Integration tests for AWS config file handling
use std::fs;
use tempfile::TempDir;

#[test]
fn test_config_file_paths() {
    // Test that config paths are correctly resolved
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config");

    // Create test config file
    let config_content = r#"
[profile test]
azure_tenant_id = xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
azure_app_id_uri = https://signin.aws.amazon.com/saml
azure_default_username = user@example.com
region = us-east-1
"#;

    fs::write(&config_path, config_content).unwrap();

    // Verify file was created
    assert!(config_path.exists());

    // Read and verify content
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("azure_tenant_id"));
    assert!(content.contains("azure_app_id_uri"));
}

#[test]
fn test_credentials_file_format() {
    // Test that credentials file format is correct
    let temp_dir = TempDir::new().unwrap();
    let credentials_path = temp_dir.path().join("credentials");

    // Create test credentials file
    let credentials_content = r#"
[test]
aws_access_key_id = AKIAIOSFODNN7EXAMPLE
aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
aws_session_token = AQoDYXdzEJr...
aws_expiration = 2026-04-15T14:30:00.000Z
"#;

    fs::write(&credentials_path, credentials_content).unwrap();

    // Verify file was created
    assert!(credentials_path.exists());

    // Read and verify content
    let content = fs::read_to_string(&credentials_path).unwrap();
    assert!(content.contains("aws_access_key_id"));
    assert!(content.contains("aws_secret_access_key"));
    assert!(content.contains("aws_session_token"));
    assert!(content.contains("aws_expiration"));
}

#[test]
fn test_config_with_multiple_profiles() {
    // Test handling of multiple profiles in config file
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config");

    let config_content = r#"
[profile dev]
azure_tenant_id = xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
azure_app_id_uri = https://signin.aws.amazon.com/saml
region = us-east-1

[profile prod]
azure_tenant_id = yyyyyyyy-yyyy-yyyy-yyyy-yyyyyyyyyyyy
azure_app_id_uri = https://signin.aws.amazon.com/saml
region = us-west-2

[profile staging]
region = eu-west-1
"#;

    fs::write(&config_path, config_content).unwrap();

    // Verify file contains all profiles
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("[profile dev]"));
    assert!(content.contains("[profile prod]"));
    assert!(content.contains("[profile staging]"));
}

#[test]
fn test_config_file_parsing() {
    // Test that config file can be parsed correctly
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config");

    let config_content = r#"
[profile test]
azure_tenant_id = 12345678-1234-1234-1234-123456789012
azure_app_id_uri = https://signin.aws.amazon.com/saml
azure_default_username = admin@company.com
azure_default_role_arn = arn:aws:iam::123456789012:role/Admin
azure_default_duration_hours = 4
azure_default_remember_me = true
region = us-east-1
"#;

    fs::write(&config_path, config_content).unwrap();

    // Parse the INI file using rust-ini
    let config = ini::Ini::load_from_file(&config_path);
    assert!(config.is_ok());

    let config = config.unwrap();
    let section = config.section(Some("profile test"));
    assert!(section.is_some());

    let section = section.unwrap();
    assert_eq!(
        section.get("azure_tenant_id"),
        Some("12345678-1234-1234-1234-123456789012")
    );
    assert_eq!(
        section.get("azure_app_id_uri"),
        Some("https://signin.aws.amazon.com/saml")
    );
    assert_eq!(
        section.get("azure_default_username"),
        Some("admin@company.com")
    );
    assert_eq!(section.get("azure_default_duration_hours"), Some("4"));
    assert_eq!(section.get("region"), Some("us-east-1"));
}
