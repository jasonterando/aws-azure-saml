use crate::config::{AwsConfig, ProfileConfig};
use crate::error::{AzureLoginError, Result};
use dialoguer::{Input, Confirm};

pub fn configure_profile(profile_name: &str) -> Result<()> {
    let aws_config = AwsConfig::new();

    // Load existing config if available (ignore errors - we're configuring it)
    let existing_config = aws_config.get_profile_config(profile_name).ok();

    println!("Configuring profile '{}'", profile_name);
    println!();

    // Prompt for tenant ID
    let tenant_id: String = Input::new()
        .with_prompt("Azure Tenant ID")
        .default(
            existing_config
                .as_ref()
                .map(|c| c.azure_tenant_id.clone())
                .unwrap_or_default(),
        )
        .interact_text()
        .map_err(|e| AzureLoginError::ConfigError(e.to_string()))?;

    // Prompt for App ID URI
    let app_id_uri: String = Input::new()
        .with_prompt("Azure App ID URI")
        .default(
            existing_config
                .as_ref()
                .map(|c| c.azure_app_id_uri.clone())
                .unwrap_or_else(|| "https://signin.aws.amazon.com/saml".to_string()),
        )
        .interact_text()
        .map_err(|e| AzureLoginError::ConfigError(e.to_string()))?;

    // Prompt for default username (optional)
    let default_username: String = Input::new()
        .with_prompt("Default Username (optional)")
        .allow_empty(true)
        .default(
            existing_config
                .as_ref()
                .and_then(|c| c.azure_default_username.clone())
                .unwrap_or_default(),
        )
        .interact_text()
        .map_err(|e| AzureLoginError::ConfigError(e.to_string()))?;

    // Prompt for default role ARN (optional)
    let default_role_arn: String = Input::new()
        .with_prompt("Default Role ARN (optional)")
        .allow_empty(true)
        .default(
            existing_config
                .as_ref()
                .and_then(|c| c.azure_default_role_arn.clone())
                .unwrap_or_default(),
        )
        .interact_text()
        .map_err(|e| AzureLoginError::ConfigError(e.to_string()))?;

    // Prompt for default duration hours
    let default_duration: String = Input::new()
        .with_prompt("Default Session Duration Hours (0-12)")
        .default(
            existing_config
                .as_ref()
                .and_then(|c| c.azure_default_duration_hours.clone())
                .unwrap_or_else(|| "1".to_string()),
        )
        .validate_with(|input: &String| -> std::result::Result<(), &str> {
            match input.parse::<u32>() {
                Ok(hours) if hours <= 12 => Ok(()),
                _ => Err("Duration must be between 0 and 12 hours"),
            }
        })
        .interact_text()
        .map_err(|e| AzureLoginError::ConfigError(e.to_string()))?;

    // Prompt for remember me
    let remember_me = Confirm::new()
        .with_prompt("Stay logged in (remember me)?")
        .default(
            existing_config
                .as_ref()
                .and_then(|c| c.azure_default_remember_me.as_ref())
                .and_then(|s| s.parse::<bool>().ok())
                .unwrap_or(false),
        )
        .interact()
        .map_err(|e| AzureLoginError::ConfigError(e.to_string()))?;

    // Prompt for AWS region (optional)
    let region: String = Input::new()
        .with_prompt("AWS Region (optional)")
        .allow_empty(true)
        .default(
            existing_config
                .as_ref()
                .and_then(|c| c.region.clone())
                .unwrap_or_default(),
        )
        .interact_text()
        .map_err(|e| AzureLoginError::ConfigError(e.to_string()))?;

    // Build configuration
    let config = ProfileConfig {
        azure_tenant_id: tenant_id,
        azure_app_id_uri: app_id_uri,
        azure_default_username: if default_username.is_empty() {
            None
        } else {
            Some(default_username)
        },
        azure_default_password: existing_config.and_then(|c| c.azure_default_password),
        azure_default_role_arn: if default_role_arn.is_empty() {
            None
        } else {
            Some(default_role_arn)
        },
        azure_default_duration_hours: Some(default_duration),
        azure_default_remember_me: Some(remember_me.to_string()),
        region: if region.is_empty() { None } else { Some(region) },
    };

    // Save configuration
    aws_config.set_profile_config(profile_name, &config)?;

    Ok(())
}
