use crate::config::Paths;
use crate::error::{AzureLoginError, Result};
use chrono::{DateTime, Duration, Utc};
use ini::Ini;
use serde::{Deserialize, Serialize};
use std::fs;

// Autorefresh credential time limit: 11 minutes
const REFRESH_LIMIT_MINUTES: i64 = 11;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileCredentials {
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub aws_session_token: String,
    pub aws_expiration: String,
}

impl ProfileCredentials {
    pub fn new(
        access_key_id: String,
        secret_access_key: String,
        session_token: String,
        expiration: DateTime<Utc>,
    ) -> Self {
        ProfileCredentials {
            aws_access_key_id: access_key_id,
            aws_secret_access_key: secret_access_key,
            aws_session_token: session_token,
            aws_expiration: expiration.to_rfc3339(),
        }
    }
}

/// Set credentials for a profile
pub fn set_profile_credentials(profile_name: &str, credentials: &ProfileCredentials) -> Result<()> {
    let paths = Paths::new();
    tracing::debug!("Setting credentials for profile '{}'", profile_name);

    let mut ini = if paths.credentials.exists() {
        Ini::load_from_file(&paths.credentials)
            .map_err(|e| AzureLoginError::IniError(e.to_string()))?
    } else {
        Ini::new()
    };

    // Credentials use profile name directly (no "profile " prefix)
    let mut section = ini.with_section(Some(profile_name));
    section.set("aws_access_key_id", &credentials.aws_access_key_id);
    section.set("aws_secret_access_key", &credentials.aws_secret_access_key);
    section.set("aws_session_token", &credentials.aws_session_token);
    section.set("aws_expiration", &credentials.aws_expiration);

    // Create AWS directory if it doesn't exist
    if !paths.aws_dir.exists() {
        fs::create_dir_all(&paths.aws_dir)?;
    }

    ini.write_to_file(&paths.credentials)
        .map_err(|e| AzureLoginError::IniError(e.to_string()))?;
    Ok(())
}

/// Check if profile credentials are about to expire (within 11 minutes)
pub fn is_profile_about_to_expire(profile_name: &str) -> Result<bool> {
    let paths = Paths::new();
    tracing::debug!("Checking expiration for profile '{}'", profile_name);

    if !paths.credentials.exists() {
        tracing::debug!("Credentials file not found, treating as expired");
        return Ok(true);
    }

    let ini = Ini::load_from_file(&paths.credentials)
        .map_err(|e| AzureLoginError::IniError(e.to_string()))?;

    let section = match ini.section(Some(profile_name)) {
        Some(s) => s,
        None => {
            tracing::debug!(
                "Profile '{}' not found in credentials, treating as expired",
                profile_name
            );
            return Ok(true);
        }
    };

    let expiration_str = match section.get("aws_expiration") {
        Some(exp) => exp,
        None => {
            tracing::debug!(
                "No expiration found for profile '{}', treating as expired",
                profile_name
            );
            return Ok(true);
        }
    };

    // Parse expiration date
    let expiration_date = match DateTime::parse_from_rfc3339(expiration_str) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => {
            tracing::warn!(
                "Could not parse expiration date '{}', treating as expired",
                expiration_str
            );
            return Ok(true);
        }
    };

    let now = Utc::now();
    let time_remaining = expiration_date - now;
    let refresh_limit = Duration::minutes(REFRESH_LIMIT_MINUTES);

    tracing::debug!(
        "Remaining time till credential expiration: {}s, refresh due if time lower than: {}s",
        time_remaining.num_seconds(),
        refresh_limit.num_seconds()
    );

    Ok(time_remaining < refresh_limit)
}
