use crate::cli::Cli;
use crate::config::{AwsConfig, is_profile_about_to_expire};
use crate::error::{AzureLoginError, Result};
use crate::azure::browser::{launch_browser, BrowserOptions};
use crate::azure::state_machine::{run_state_machine, LoginContext};
use crate::aws::{create_saml_request, parse_saml_response, assume_role_with_saml, AwsRole};
use dialoguer::Select;

// AWS SAML endpoints
const AWS_SAML_ENDPOINT_STANDARD: &str = "https://signin.aws.amazon.com/saml";
const AWS_SAML_ENDPOINT_GOVCLOUD: &str = "https://signin.amazonaws-us-gov.com/saml";
const AWS_SAML_ENDPOINT_CHINA: &str = "https://signin.amazonaws.cn/saml";

pub async fn login_async(profile_name: &str, cli: &Cli) -> Result<()> {
    tracing::info!("Logging in to profile '{}'", profile_name);

    // Launch browser
    let browser_options = BrowserOptions::from(cli);
    let browser = launch_browser(&browser_options).await?;

    // Login with the browser
    login_async_with_browser(profile_name, cli, &browser).await
}

/// Login to a profile using an existing browser instance
async fn login_async_with_browser(
    profile_name: &str,
    cli: &Cli,
    browser: &chromiumoxide::Browser,
) -> Result<()> {
    tracing::debug!("Logging in to profile '{}' with existing browser", profile_name);

    // Load profile configuration
    let aws_config = AwsConfig::new();
    let profile_config = aws_config.get_profile_config(profile_name)?;

    // Determine AWS SAML endpoint
    let saml_endpoint = if profile_config.azure_app_id_uri.contains("us-gov") {
        AWS_SAML_ENDPOINT_GOVCLOUD
    } else if profile_config.azure_app_id_uri.contains(".cn") {
        AWS_SAML_ENDPOINT_CHINA
    } else {
        AWS_SAML_ENDPOINT_STANDARD
    };

    tracing::debug!("Using SAML endpoint: {}", saml_endpoint);

    // Generate SAML request
    let saml_request = create_saml_request(
        &profile_config.azure_app_id_uri,
        &profile_config.azure_tenant_id,
        saml_endpoint,
    )?;

    // Create Azure login URL
    let login_url = format!(
        "https://login.microsoftonline.com/{}/saml2?SAMLRequest={}",
        profile_config.azure_tenant_id,
        urlencoding::encode(&saml_request)
    );

    tracing::debug!("Login URL: {}", login_url);

    // Create new page (with about:blank to avoid premature navigation)
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| AzureLoginError::BrowserError(format!("Failed to create page: {}", e)))?;

    tracing::debug!("Created new page");

    // Enable network domain for request monitoring BEFORE navigating
    // This ensures we don't miss any SAML POST requests that happen quickly
    page.execute(chromiumoxide::cdp::browser_protocol::network::EnableParams::default())
        .await
        .map_err(|e| AzureLoginError::BrowserError(format!("Failed to enable network: {}", e)))?;

    tracing::debug!("Network monitoring enabled");

    // Parse remember_me config value
    let remember_me = profile_config.azure_default_remember_me.as_ref().and_then(|s| {
        match s.to_lowercase().as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        }
    });

    // Create login context for state machine
    let login_context = LoginContext {
        username: profile_config.azure_default_username.clone(),
        password: profile_config.azure_default_password.clone(),
        no_prompt: cli.no_prompt,
        remember_me,
    };

    // Set up SAML interceptor with navigation - this ensures the event listener is subscribed before navigation
    let saml_response = intercept_saml_with_navigation(&page, saml_endpoint, &login_url, &login_context).await?;

    tracing::debug!("SAML response captured");

    // Parse SAML response
    let roles = parse_saml_response(&saml_response)?;

    // Select role
    let role = select_role(&roles, &profile_config, cli.no_prompt)?;

    // Determine session duration
    let duration_hours = determine_duration(&profile_config, cli.no_prompt)?;

    // Assume role
    assume_role_with_saml(
        profile_name,
        &role,
        &saml_response,
        duration_hours,
        profile_config.region.as_deref(),
        cli.no_verify_ssl,
    )
    .await?;

    tracing::debug!("Successfully logged in to profile '{}'", profile_name);

    Ok(())
}

/// Intercept SAML response by setting up listener first, then navigating, then running state machine
async fn intercept_saml_with_navigation(
    page: &chromiumoxide::Page,
    endpoint: &str,
    login_url: &str,
    login_context: &LoginContext,
) -> Result<String> {
    use chromiumoxide::cdp::browser_protocol::network::EventRequestWillBeSent;
    use futures::StreamExt;

    tracing::debug!("Setting up SAML interception for endpoint: {}", endpoint);

    // Subscribe to network request events FIRST, before navigating
    let mut events = page
        .event_listener::<EventRequestWillBeSent>()
        .await
        .map_err(|e| AzureLoginError::BrowserError(format!("Failed to subscribe to events: {}", e)))?;

    tracing::debug!("Event listener subscribed, navigating to {}", login_url);

    // Now navigate to the login URL
    page.goto(login_url)
        .await
        .map_err(|e| AzureLoginError::BrowserError(format!("Failed to navigate: {}", e)))?;

    // Start the state machine in the background
    let mut state_machine_handle = tokio::spawn({
        let page_clone = page.clone();
        let context_clone = LoginContext {
            username: login_context.username.clone(),
            password: login_context.password.clone(),
            no_prompt: login_context.no_prompt,
            remember_me: login_context.remember_me,
        };
        async move {
            run_state_machine(&page_clone, &context_clone).await
        }
    });

    // Wait for SAML POST while state machine runs
    loop {
        tokio::select! {
            // Check if state machine completed (shouldn't happen before SAML response)
            result = &mut state_machine_handle => {
                match result {
                    Ok(Ok(())) => {
                        return Err(AzureLoginError::BrowserError(
                            "State machine completed without SAML response".to_string()
                        ));
                    }
                    Ok(Err(e)) => {
                        return Err(e);
                    }
                    Err(e) => {
                        return Err(AzureLoginError::BrowserError(
                            format!("State machine task failed: {}", e)
                        ));
                    }
                }
            }
            // Check for SAML POST
            event = events.next() => {
                if let Some(event) = event {
                    let request_url = &event.request.url;
                    let request_id = event.request_id.clone();

                    // Check if this is a POST to the AWS SAML endpoint
                    if request_url.contains(endpoint) && event.request.method == "POST" {
                        tracing::debug!("Found SAML POST request to: {}", request_url);

                        // Try to get the POST body from the request
                        use chromiumoxide::cdp::browser_protocol::network::GetRequestPostDataParams;

                        // Wait a bit for the POST data to be available
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                        if let Ok(post_data_result) = page.execute(GetRequestPostDataParams::new(request_id)).await {
                            tracing::debug!("Extracting SAML response from POST data");
                            let saml_response = parse_saml_from_post(&post_data_result.post_data)?;
                            // Cancel the state machine task as we got what we need
                            state_machine_handle.abort();
                            return Ok(saml_response);
                        } else {
                            tracing::warn!("Could not retrieve POST data for request");
                        }
                    }
                } else {
                    return Err(AzureLoginError::BrowserError(
                        "SAML response not captured - event stream ended".to_string(),
                    ));
                }
            }
        }
    }
}

/// Intercept SAML response from POST request to AWS endpoint (deprecated - use intercept_saml_with_navigation)
#[allow(dead_code)]
async fn intercept_saml_response(page: &chromiumoxide::Page, endpoint: &str) -> Result<String> {
    use chromiumoxide::cdp::browser_protocol::network::EventRequestWillBeSent;
    use futures::StreamExt;

    tracing::debug!("Starting SAML response interception for endpoint: {}", endpoint);

    // Subscribe to network request events
    let mut events = page
        .event_listener::<EventRequestWillBeSent>()
        .await
        .map_err(|e| AzureLoginError::BrowserError(format!("Failed to subscribe to events: {}", e)))?;

    while let Some(event) = events.next().await {
        let request_url = &event.request.url;
        let request_id = event.request_id.clone();

        // Check if this is a POST to the AWS SAML endpoint
        if request_url.contains(endpoint) && event.request.method == "POST" {
            tracing::debug!("Found SAML POST request to: {}", request_url);

            // Try to get the POST body from the request
            use chromiumoxide::cdp::browser_protocol::network::GetRequestPostDataParams;

            // Wait a bit for the POST data to be available
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            if let Ok(post_data_result) = page.execute(GetRequestPostDataParams::new(request_id)).await {
                tracing::debug!("Extracting SAML response from POST data");
                return parse_saml_from_post(&post_data_result.post_data);
            } else {
                tracing::warn!("Could not retrieve POST data for request");
            }
        }
    }

    Err(AzureLoginError::BrowserError(
        "SAML response not captured - event stream ended".to_string(),
    ))
}

/// Parse SAMLResponse from URL-encoded POST data
fn parse_saml_from_post(post_data: &str) -> Result<String> {
    tracing::debug!("Parsing SAML from POST data");

    for (key, value) in form_urlencoded::parse(post_data.as_bytes()) {
        if key == "SAMLResponse" {
            tracing::debug!("Successfully extracted SAMLResponse");
            return Ok(value.into_owned());
        }
    }

    Err(AzureLoginError::BrowserError(
        "SAMLResponse field not found in POST data".to_string(),
    ))
}

pub async fn login_all(cli: &Cli) -> Result<()> {
    tracing::info!("Logging in to all profiles");

    let aws_config = AwsConfig::new();
    let profiles = aws_config.get_all_profile_names()?;

    // Group profiles by Azure tenant ID to reuse browser sessions
    let mut tenant_groups: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for profile in profiles {
        // Check if profile has Azure config
        if !aws_config.has_azure_config(&profile)? {
            tracing::debug!("Skipping profile '{}' (no Azure config)", profile);
            continue;
        }

        // Check if credentials are about to expire
        if !cli.force_refresh && !is_profile_about_to_expire(&profile)? {
            println!("Skipping profile '{}' (not yet due for refresh)", profile);
            continue;
        }

        // Get tenant ID for grouping
        let profile_config = aws_config.get_profile_config(&profile)?;
        tenant_groups
            .entry(profile_config.azure_tenant_id.clone())
            .or_default()
            .push(profile);
    }

    // Process each tenant group with a shared browser session
    for (tenant_id, profiles_in_tenant) in tenant_groups {
        tracing::debug!(
            "Reusing browser session for {} profile(s) in tenant {}",
            profiles_in_tenant.len(),
            &tenant_id[..8.min(tenant_id.len())]
        );

        tracing::debug!(
            "Processing {} profile(s) for tenant '{}' with shared browser session",
            profiles_in_tenant.len(),
            tenant_id
        );

        // Launch browser once for this tenant
        let browser_options = BrowserOptions::from(cli);
        let browser = launch_browser(&browser_options).await?;

        // Login to all profiles in this tenant group
        for profile in profiles_in_tenant.iter() {
            println!("Logging in to profile '{}'...", profile);
            login_async_with_browser(profile, cli, &browser).await?;
        }

        // Browser will be dropped here, cleaning up the session
        tracing::debug!("Completed all logins for tenant '{}'", tenant_id);
    }

    Ok(())
}

fn select_role(roles: &[AwsRole], profile_config: &crate::config::ProfileConfig, no_prompt: bool) -> Result<AwsRole> {
    if roles.len() == 1 {
        return Ok(roles[0].clone());
    }

    // Check if there's a default role configured that matches one of the available roles
    if let Some(default_role_arn) = &profile_config.azure_default_role_arn {
        if let Some(role) = roles.iter().find(|r| &r.role_arn == default_role_arn) {
            tracing::debug!("Using configured default role: {}", default_role_arn);
            return Ok(role.clone());
        } else {
            tracing::warn!(
                "Configured default role '{}' not found in available roles",
                default_role_arn
            );
        }
    }

    // If no_prompt mode and no valid default, use first role
    if no_prompt {
        tracing::debug!("No default role configured, using first available role");
        return Ok(roles[0].clone());
    }

    // Prompt user to select role
    let role_names: Vec<String> = roles.iter().map(|r| r.role_arn.clone()).collect();

    let selection = Select::new()
        .with_prompt("Select a role")
        .items(&role_names)
        .default(0)
        .interact()
        .map_err(|e| AzureLoginError::AuthenticationFailed(format!("Role selection failed: {}", e)))?;

    Ok(roles[selection].clone())
}

fn determine_duration(profile_config: &crate::config::ProfileConfig, no_prompt: bool) -> Result<u32> {
    // Check if there's a valid duration configured
    if let Some(duration_str) = &profile_config.azure_default_duration_hours {
        if let Ok(hours) = duration_str.parse::<u32>() {
            if (1..=12).contains(&hours) {
                tracing::debug!("Using configured default duration: {} hours", hours);
                return Ok(hours);
            } else {
                tracing::warn!(
                    "Configured default duration '{}' is out of range (1-12 hours)",
                    hours
                );
            }
        }
    }

    // If no_prompt mode and no valid default, use 1 hour
    if no_prompt {
        tracing::debug!("No default duration configured, using 1 hour");
        return Ok(1);
    }

    // Prompt user for duration
    let duration_str: String = dialoguer::Input::new()
        .with_prompt("Session duration in hours (1-12)")
        .default("1".to_string())
        .validate_with(|input: &String| -> std::result::Result<(), &str> {
            match input.parse::<u32>() {
                Ok(hours) if (1..=12).contains(&hours) => Ok(()),
                _ => Err("Duration must be between 1 and 12 hours"),
            }
        })
        .interact_text()
        .map_err(|e| AzureLoginError::AuthenticationFailed(format!("Duration input failed: {}", e)))?;

    Ok(duration_str.parse().unwrap())
}
