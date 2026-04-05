use crate::aws::saml::AwsRole;
use crate::config::credentials::set_profile_credentials;
use crate::config::ProfileCredentials;
use crate::error::{AzureLoginError, Result};
use aws_config::BehaviorVersion;
use aws_sdk_sts::Client as StsClient;
use chrono::{DateTime, Utc};

pub async fn assume_role_with_saml(
    profile_name: &str,
    role: &AwsRole,
    saml_assertion: &str,
    duration_hours: u32,
    region: Option<&str>,
    no_verify_ssl: bool,
) -> Result<()> {
    tracing::debug!(
        "Assuming role '{}' for profile '{}'",
        role.role_arn,
        profile_name
    );

    // Configure AWS SDK
    let mut config_loader = aws_config::defaults(BehaviorVersion::latest());

    if let Some(region_str) = region {
        config_loader = config_loader.region(aws_config::Region::new(region_str.to_string()));
    }

    // Handle proxy
    if let Ok(proxy) = std::env::var("https_proxy") {
        tracing::debug!("Using proxy: {}", proxy);
    }

    // TODO: Handle SSL verification flag
    if no_verify_ssl {
        tracing::warn!("SSL verification disabled");
    }

    let config = config_loader.load().await;
    let sts_client = StsClient::new(&config);

    // Call AssumeRoleWithSAML
    let duration_seconds = (duration_hours * 3600) as i32;

    let response = sts_client
        .assume_role_with_saml()
        .principal_arn(&role.principal_arn)
        .role_arn(&role.role_arn)
        .saml_assertion(saml_assertion)
        .duration_seconds(duration_seconds)
        .send()
        .await
        .map_err(|e| AzureLoginError::StsError(e.to_string()))?;

    // Extract credentials
    let credentials = response
        .credentials()
        .ok_or_else(|| AzureLoginError::StsError("No credentials returned from STS".to_string()))?;

    let access_key_id = credentials.access_key_id.clone();
    let secret_access_key = credentials.secret_access_key.clone();
    let session_token = credentials.session_token.clone();
    let expiration = &credentials.expiration;

    // Convert AWS DateTime to chrono DateTime
    use aws_smithy_types::date_time::Format;
    let expiration_str = expiration
        .fmt(Format::DateTime)
        .map_err(|e| AzureLoginError::StsError(format!("Failed to format expiration: {}", e)))?;
    let expiration_dt: DateTime<Utc> = DateTime::parse_from_rfc3339(&expiration_str)
        .map_err(|e| AzureLoginError::StsError(format!("Failed to parse expiration: {}", e)))?
        .into();

    // Create credentials
    let profile_credentials = ProfileCredentials::new(
        access_key_id,
        secret_access_key,
        session_token,
        expiration_dt,
    );

    // Save credentials
    set_profile_credentials(profile_name, &profile_credentials)?;

    tracing::debug!(
        "Successfully obtained credentials for profile '{}', expires at {}",
        profile_name,
        profile_credentials.aws_expiration
    );

    Ok(())
}
