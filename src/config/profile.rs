use crate::error::{AzureLoginError, Result};
use crate::config::Paths;
use ini::Ini;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub azure_tenant_id: String,
    pub azure_app_id_uri: String,
    pub azure_default_username: Option<String>,
    pub azure_default_password: Option<String>,
    pub azure_default_role_arn: Option<String>,
    pub azure_default_duration_hours: Option<String>,
    pub azure_default_remember_me: Option<String>,
    pub region: Option<String>,
}

pub struct AwsConfig {
    paths: Paths,
}

impl AwsConfig {
    pub fn new() -> Self {
        AwsConfig {
            paths: Paths::new(),
        }
    }

    /// Get the section name for a profile in the config file
    /// "default" profile stays as "default", others are "profile {name}"
    fn get_config_section_name(&self, profile_name: &str) -> String {
        if profile_name == "default" {
            "default".to_string()
        } else {
            format!("profile {}", profile_name)
        }
    }

    /// Set profile configuration values
    pub fn set_profile_config(&self, profile_name: &str, config: &ProfileConfig) -> Result<()> {
        let section_name = self.get_config_section_name(profile_name);
        tracing::debug!(
            "Setting config for profile '{}' in section '{}'",
            profile_name,
            section_name
        );

        let mut ini = if self.paths.config.exists() {
            Ini::load_from_file(&self.paths.config)
                .map_err(|e| AzureLoginError::IniError(e.to_string()))?
        } else {
            Ini::new()
        };

        // Set all config values
        let mut section = ini.with_section(Some(&section_name));
        section.set("azure_tenant_id", &config.azure_tenant_id);
        section.set("azure_app_id_uri", &config.azure_app_id_uri);

        if let Some(username) = &config.azure_default_username {
            section.set("azure_default_username", username);
        }

        if let Some(password) = &config.azure_default_password {
            section.set("azure_default_password", password);
        }

        if let Some(role_arn) = &config.azure_default_role_arn {
            section.set("azure_default_role_arn", role_arn);
        }

        if let Some(duration) = &config.azure_default_duration_hours {
            section.set("azure_default_duration_hours", duration);
        }

        if let Some(remember_me) = &config.azure_default_remember_me {
            section.set("azure_default_remember_me", remember_me);
        }

        if let Some(region) = &config.region {
            section.set("region", region);
        }

        // Create AWS directory if it doesn't exist
        if !self.paths.aws_dir.exists() {
            fs::create_dir_all(&self.paths.aws_dir)?;
        }

        ini.write_to_file(&self.paths.config)
            .map_err(|e| AzureLoginError::IniError(e.to_string()))?;
        Ok(())
    }

    /// Get profile configuration
    pub fn get_profile_config(&self, profile_name: &str) -> Result<ProfileConfig> {
        let section_name = self.get_config_section_name(profile_name);
        tracing::debug!(
            "Getting config for profile '{}' in section '{}'",
            profile_name,
            section_name
        );

        if !self.paths.config.exists() {
            return Err(AzureLoginError::ProfileNotFound(profile_name.to_string()));
        }

        let ini = Ini::load_from_file(&self.paths.config)
            .map_err(|e| AzureLoginError::IniError(e.to_string()))?;

        let section = match ini.section(Some(&section_name)) {
            Some(s) => s,
            None => return Err(AzureLoginError::ProfileNotFound(profile_name.to_string())),
        };

        // Required fields
        let azure_tenant_id = match section.get("azure_tenant_id") {
            Some(v) => v.to_string(),
            None => return Err(AzureLoginError::MissingAzureConfig(profile_name.to_string())),
        };

        let azure_app_id_uri = match section.get("azure_app_id_uri") {
            Some(v) => v.to_string(),
            None => return Err(AzureLoginError::MissingAzureConfig(profile_name.to_string())),
        };

        // Optional fields
        let azure_default_username = section.get("azure_default_username").map(|s| s.to_string());
        let azure_default_password = section.get("azure_default_password").map(|s| s.to_string());
        let azure_default_role_arn = section.get("azure_default_role_arn").map(|s| s.to_string());
        let azure_default_duration_hours = section.get("azure_default_duration_hours").map(|s| s.to_string());
        let azure_default_remember_me = section.get("azure_default_remember_me").map(|s| s.to_string());
        let region = section.get("region").map(|s| s.to_string());

        Ok(ProfileConfig {
            azure_tenant_id,
            azure_app_id_uri,
            azure_default_username,
            azure_default_password,
            azure_default_role_arn,
            azure_default_duration_hours,
            azure_default_remember_me,
            region,
        })
    }

    /// Get all profile names from config
    pub fn get_all_profile_names(&self) -> Result<Vec<String>> {
        tracing::debug!("Getting all configured profiles from config");

        if !self.paths.config.exists() {
            return Ok(Vec::new());
        }

        let ini = Ini::load_from_file(&self.paths.config)
            .map_err(|e| AzureLoginError::IniError(e.to_string()))?;

        let profiles: Vec<String> = ini
            .sections()
            .filter_map(|section_name| {
                section_name.map(|name| {
                    // Remove "profile " prefix if present
                    if name.starts_with("profile ") {
                        name.trim_start_matches("profile ").to_string()
                    } else {
                        name.to_string()
                    }
                })
            })
            .collect();

        tracing::debug!("Received profiles: {:?}", profiles);
        Ok(profiles)
    }

    /// Check if a profile has Azure configuration
    pub fn has_azure_config(&self, profile_name: &str) -> Result<bool> {
        match self.get_profile_config(profile_name) {
            Ok(_) => Ok(true),
            Err(AzureLoginError::ProfileNotFound(_)) => Ok(false),
            Err(AzureLoginError::MissingAzureConfig(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

impl Default for AwsConfig {
    fn default() -> Self {
        Self::new()
    }
}
