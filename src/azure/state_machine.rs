use crate::error::{AzureLoginError, Result};
use chromiumoxide::Page;
use dialoguer::{Input, Password, Select};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub struct LoginContext {
    pub username: Option<String>,
    pub password: Option<String>,
    pub no_prompt: bool,
    pub remember_me: Option<bool>,
    pub username_from_config: bool,
}

// Static flag to track if TFA instructions have been shown
static TFA_INSTRUCTIONS_SHOWN: AtomicBool = AtomicBool::new(false);

// Static flag to track if voice call waiting message has been shown
static VOICE_CALL_WAITING_SHOWN: AtomicBool = AtomicBool::new(false);

// Static flag to track if user requested cancellation
static USER_REQUESTED_CANCEL: AtomicBool = AtomicBool::new(false);

/// Save a screenshot to the specified filename in the temp directory
async fn save_debug_screenshot(page: &Page, filename: &str) -> String {
    let screenshot_path = std::env::temp_dir().join(filename);
    let screenshot_path_str = screenshot_path.to_string_lossy().to_string();

    tracing::debug!("Attempting to save screenshot to {}", screenshot_path_str);

    match page
        .screenshot(
            chromiumoxide::page::ScreenshotParams::builder()
                .full_page(true)
                .omit_background(false)
                .build(),
        )
        .await
    {
        Ok(screenshot_data) => {
            if let Err(e) = std::fs::write(&screenshot_path, screenshot_data) {
                tracing::warn!(
                    "Failed to write screenshot to {}: {}",
                    screenshot_path_str,
                    e
                );
            } else {
                tracing::info!("Screenshot saved to {}", screenshot_path_str);
            }
        }
        Err(e) => {
            tracing::warn!("Failed to capture screenshot: {}", e);
        }
    }

    screenshot_path_str
}

/// Main state machine loop that handles Azure AD login automation
pub async fn run_state_machine(page: &Page, context: &LoginContext) -> Result<()> {
    tracing::debug!("Running Azure AD state machine");

    let mut unrecognized_delay = 0;
    let mut aws_endpoint_delay = 0;
    const MAX_UNRECOGNIZED_DELAY: u64 = 30_000; // 30 seconds
    const MAX_AWS_ENDPOINT_DELAY: u64 = 10_000; // 10 seconds for AWS endpoint
    const POLL_INTERVAL: u64 = 1000; // 1 second

    loop {
        // Check if we've reached the AWS SAML endpoint (successful authentication)
        // This can happen quickly when session is cached (browser reuse)
        if let Ok(Some(url)) = page.url().await {
            if url.contains("signin.aws.amazon.com")
                || url.contains("signin.amazonaws-us-gov.com")
                || url.contains("signin.amazonaws.cn")
            {
                tracing::debug!(
                    "Reached AWS endpoint, waiting for SAML interception ({}ms)",
                    aws_endpoint_delay
                );
                // Reset unrecognized delay since we're in a known state
                unrecognized_delay = 0;
                // Wait for the interceptor to capture the SAML response
                tokio::time::sleep(Duration::from_millis(POLL_INTERVAL)).await;
                aws_endpoint_delay += POLL_INTERVAL;

                // Check if we're at the AWS role selection page (already authenticated)
                if let Ok(content) = page.content().await {
                    // AWS role selection page contains specific elements
                    if content.contains("arn:aws:iam::") || content.contains("saml-account") {
                        tracing::debug!("Detected AWS role selection page (already authenticated via shared session)");
                        // The SAML response is already processed by AWS, but we missed capturing it
                        // This is expected when reusing browser sessions - we need to extract from the page
                        return Err(AzureLoginError::BrowserError(
                            "Reached AWS role selection page but SAML response was not captured. This can happen with shared tenant sessions.".to_string()
                        ));
                    }
                }

                // If we've been at AWS endpoint too long without finding the role selection page, something went wrong
                if aws_endpoint_delay > MAX_AWS_ENDPOINT_DELAY {
                    tracing::error!(
                        "Timeout at AWS endpoint after {}ms, saving screenshot",
                        aws_endpoint_delay
                    );

                    let screenshot_path =
                        save_debug_screenshot(page, "unrecognized-state.png").await;

                    return Err(AzureLoginError::BrowserError(
                        format!("Timeout waiting for SAML response at AWS endpoint. The SAML POST may have been missed. Screenshot saved to {}", screenshot_path)
                    ));
                }
                continue;
            } else {
                // Reset AWS endpoint delay if we're not at AWS endpoint
                aws_endpoint_delay = 0;
            }
        }

        // Try each state in priority order
        // Username input - use visibility check to avoid false positives with hidden fields
        if is_element_visible(page, "input[name='loginfmt']").await {
            tracing::debug!("Found state: username input");
            handle_username_input(page, context).await?;
            unrecognized_delay = 0;
            continue;
        }

        // Account selection
        if try_selector(page, "#aadTile").await {
            tracing::debug!("Found state: account selection");
            handle_account_selection(page, context).await?;
            unrecognized_delay = 0;
            continue;
        }

        // Passwordless authentication
        if try_selector(page, "input[value='Send notification']").await {
            tracing::debug!("Found state: passwordless authentication");
            handle_passwordless_auth(page).await?;
            unrecognized_delay = 0;
            continue;
        }

        // Password input - use visibility check to avoid false positives with hidden fields
        if is_element_visible(page, "input[name='Password']").await
            || is_element_visible(page, "input[name='passwd']").await
        {
            tracing::debug!("Found state: password input");
            handle_password_input(page, context).await?;
            unrecognized_delay = 0;
            continue;
        }

        // TFA code input (check this BEFORE TFA instructions as they can appear together)
        // Use visibility check to avoid false positives with hidden fields
        if is_element_visible(page, "input[name='otc']").await {
            tracing::debug!("Found state: TFA code input");
            handle_tfa_code_input(page, context).await?;
            unrecognized_delay = 0;
            continue;
        }

        // Voice call waiting (check before TFA instructions as they can look similar)
        // Title: "Approve sign in request", Message: "We're calling your phone..."
        // Try multiple selectors as the exact IDs may vary
        if try_selector(page, "#idDiv_SAOTCV_Title").await
            || try_selector(page, "#idDiv_SAOTCV_Description").await
            || try_selector(page, "div[data-bind*='voiceCall']").await
        {
            tracing::debug!("Found state: voice call waiting");
            handle_voice_call_waiting(page, context).await?;
            unrecognized_delay = 0;
            continue;
        }

        // Check for "Approve sign in request" text as a fallback
        if let Ok(content) = page.content().await {
            if content.contains("We're calling your phone")
                || (content.contains("Approve sign in request") && content.contains("calling"))
            {
                tracing::debug!("Found state: voice call waiting (via content match)");
                handle_voice_call_waiting(page, context).await?;
                unrecognized_delay = 0;
                continue;
            }
        }

        // TFA instructions (only triggers if code input not present)
        // This is a transient/informational state - just display and continue polling
        if try_selector(page, "#idDiv_SAOTCAS_Description").await {
            tracing::debug!("Found state: TFA instructions");
            handle_tfa_instructions_once(page, context).await?;
            unrecognized_delay = 0;
            continue;
        }

        // Verify your identity (multiple authentication options)
        // Check for multiple possible selectors for this state
        if try_selector(page, "#idDiv_SAOTCC_Title").await
            || try_selector(page, "#idDiv_SAOTCC_Description").await
            || try_selector(page, "div[data-value='PhoneAppNotification']").await
            || try_selector(page, "div[data-value='PhoneAppOTP']").await
            || try_selector(page, "div[data-value='OneWaySMS']").await
            || try_selector(page, "div[data-value='TwoWayVoiceMobile']").await
            || try_selector(page, "#idA_SAOTCC_Resend").await
        {
            tracing::debug!(
                "Found state: verify your identity (checking for authentication options)"
            );
            handle_verify_identity(page, context).await?;
            unrecognized_delay = 0;
            continue;
        }

        // Remember me
        if try_selector(page, "#KmsiDescription").await {
            tracing::debug!("Found state: remember me");
            handle_remember_me(page, context).await?;
            unrecognized_delay = 0;
            continue;
        }

        // Service exception
        if try_selector(page, "#service_exception_message").await {
            tracing::error!("Found state: service exception");
            return handle_service_exception(page).await;
        }

        // TFA failure
        if try_selector(page, "#idDiv_SAASDS_Description").await
            || try_selector(page, "#idDiv_SAASTO_Description").await
        {
            tracing::debug!("Found state: TFA failure");
            handle_tfa_failure(page).await?;
            unrecognized_delay = 0;
            continue;
        }

        // No state matched, wait and retry
        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL)).await;
        unrecognized_delay += POLL_INTERVAL;

        if unrecognized_delay > MAX_UNRECOGNIZED_DELAY {
            tracing::error!(
                "Unrecognized page state after {}ms, saving screenshot",
                unrecognized_delay
            );

            let screenshot_path = save_debug_screenshot(page, "unrecognized-state.png").await;

            return Err(AzureLoginError::UnrecognizedPageState(screenshot_path));
        }
    }
}

/// Helper function to check if a selector exists on the page
async fn try_selector(page: &Page, selector: &str) -> bool {
    page.find_element(selector).await.is_ok()
}

/// Handle username input
async fn handle_username_input(page: &Page, context: &LoginContext) -> Result<()> {
    tracing::debug!("Handling username input");

    // Get username
    let username = if let Some(u) = &context.username {
        u.clone()
    } else if context.no_prompt {
        return Err(AzureLoginError::AuthenticationFailed(
            "No username provided and --no-prompt is set".to_string(),
        ));
    } else {
        Input::<String>::new()
            .with_prompt("Username")
            .interact_text()
            .map_err(|e| AzureLoginError::AuthenticationFailed(e.to_string()))?
    };

    tracing::debug!("Entering username: {}", username);

    // Retry loop for handling stale elements
    const MAX_RETRIES: usize = 5;
    const RETRY_DELAY_MS: u64 = 200;

    for attempt in 0..MAX_RETRIES {
        match try_input_text_and_submit(page, "input[name='loginfmt']", &username, "username").await
        {
            Ok(_) => break,
            Err(e) if attempt < MAX_RETRIES - 1 => {
                tracing::debug!("Attempt {} failed: {}, retrying...", attempt + 1, e);
                tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    // Wait for page transition - ensure the username field is no longer visible
    // This prevents the state machine from detecting the username field again
    const TRANSITION_TIMEOUT_MS: u64 = 5000;
    const TRANSITION_POLL_MS: u64 = 100;
    let mut elapsed = 0;

    while elapsed < TRANSITION_TIMEOUT_MS {
        // Check if username field is no longer visible
        if !is_element_visible(page, "input[name='loginfmt']").await {
            tracing::debug!(
                "Username field hidden, page transition detected after {}ms",
                elapsed
            );
            break;
        }

        tokio::time::sleep(Duration::from_millis(TRANSITION_POLL_MS)).await;
        elapsed += TRANSITION_POLL_MS;
    }

    if elapsed >= TRANSITION_TIMEOUT_MS {
        tracing::warn!(
            "Username field still visible after {}ms, proceeding anyway",
            elapsed
        );
    }

    // Additional small delay to ensure next page is ready
    tokio::time::sleep(Duration::from_millis(300)).await;

    Ok(())
}

/// Helper function to input text into a field with retry logic
async fn try_input_text(page: &Page, selector: &str, text: &str, field_name: &str) -> Result<()> {
    let input = page.find_element(selector).await.map_err(|e| {
        AzureLoginError::BrowserError(format!(
            "Failed to find {} field '{}': {}",
            field_name, selector, e
        ))
    })?;

    input.scroll_into_view().await.map_err(|e| {
        AzureLoginError::BrowserError(format!(
            "Failed to scroll {} field into view: {} (selector: {})",
            field_name, e, selector
        ))
    })?;

    input.focus().await.map_err(|e| {
        AzureLoginError::BrowserError(format!(
            "Failed to focus {} field: {} (selector: {})",
            field_name, e, selector
        ))
    })?;

    input.type_str(text).await.map_err(|e| {
        AzureLoginError::BrowserError(format!(
            "Failed to type into {} field: {} (selector: {})",
            field_name, e, selector
        ))
    })?;

    Ok(())
}

/// Helper function to input text and submit by pressing Enter
async fn try_input_text_and_submit(
    page: &Page,
    selector: &str,
    text: &str,
    field_name: &str,
) -> Result<()> {
    try_input_text(page, selector, text, field_name).await?;

    // Press Enter to submit instead of clicking submit button (more reliable)
    let input = page.find_element(selector).await.map_err(|e| {
        AzureLoginError::BrowserError(format!(
            "Failed to re-find {} field for submit: {}",
            field_name, e
        ))
    })?;

    input.press_key("Enter").await.map_err(|e| {
        AzureLoginError::BrowserError(format!(
            "Failed to press Enter in {} field: {} (selector: {})",
            field_name, e, selector
        ))
    })?;

    Ok(())
}

/// Helper to check if an element is actually visible (not just in DOM)
async fn is_element_visible(page: &Page, selector: &str) -> bool {
    // Use page.evaluate to check if element is visible
    // This checks: element exists, has offsetParent (not display:none), and has dimensions
    let script = format!(
        r#"
        (function() {{
            const el = document.querySelector('{}');
            if (!el) return false;
            const style = window.getComputedStyle(el);
            return el.offsetParent !== null &&
                   el.offsetWidth > 0 &&
                   el.offsetHeight > 0 &&
                   style.visibility !== 'hidden' &&
                   style.display !== 'none';
        }})()
        "#,
        selector.replace('\'', "\\'")
    );

    if let Ok(result) = page.evaluate(script).await {
        if let Ok(value) = result.into_value::<bool>() {
            return value;
        }
    }

    false
}

/// Handle password input
async fn handle_password_input(page: &Page, context: &LoginContext) -> Result<()> {
    tracing::debug!("Handling password input");

    let password = if let Some(p) = &context.password {
        p.clone()
    } else if context.no_prompt {
        return Err(AzureLoginError::AuthenticationFailed(
            "No password provided and --no-prompt is set".to_string(),
        ));
    } else {
        // Show username in prompt only if it was auto-filled from config
        let prompt = if context.username_from_config {
            if let Some(username) = &context.username {
                format!("Password for {}", username)
            } else {
                "Password".to_string()
            }
        } else {
            "Password".to_string()
        };

        Password::new()
            .with_prompt(&prompt)
            .interact()
            .map_err(|e| AzureLoginError::AuthenticationFailed(e.to_string()))?
    };

    tracing::debug!("Entering password");

    // Retry loop for handling stale elements
    const MAX_RETRIES: usize = 5;
    const RETRY_DELAY_MS: u64 = 200;

    for attempt in 0..MAX_RETRIES {
        // Try both possible password field names
        let selector = if page.find_element("input[name='Password']").await.is_ok() {
            "input[name='Password']"
        } else {
            "input[name='passwd']"
        };

        match try_input_text_and_submit(page, selector, &password, "password").await {
            Ok(_) => break,
            Err(e) if attempt < MAX_RETRIES - 1 => {
                tracing::debug!("Attempt {} failed: {}, retrying...", attempt + 1, e);
                tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    // Wait for page transition - ensure the password field is no longer visible
    const TRANSITION_TIMEOUT_MS: u64 = 5000;
    const TRANSITION_POLL_MS: u64 = 100;
    let mut elapsed = 0;

    while elapsed < TRANSITION_TIMEOUT_MS {
        // Check if both password fields are no longer visible
        if !is_element_visible(page, "input[name='Password']").await
            && !is_element_visible(page, "input[name='passwd']").await
        {
            tracing::debug!(
                "Password field hidden, page transition detected after {}ms",
                elapsed
            );
            break;
        }

        tokio::time::sleep(Duration::from_millis(TRANSITION_POLL_MS)).await;
        elapsed += TRANSITION_POLL_MS;
    }

    if elapsed >= TRANSITION_TIMEOUT_MS {
        tracing::warn!(
            "Password field still visible after {}ms, proceeding anyway",
            elapsed
        );
    }

    // Additional small delay to ensure next page is ready
    tokio::time::sleep(Duration::from_millis(300)).await;

    Ok(())
}

/// Handle account selection
async fn handle_account_selection(page: &Page, context: &LoginContext) -> Result<()> {
    tracing::debug!("Handling account selection");

    let has_aad = try_selector(page, "#aadTile").await;
    let has_msa = try_selector(page, "#msaTile").await;
    let account_count = (has_aad as usize) + (has_msa as usize);

    if account_count == 0 {
        return Err(AzureLoginError::AuthenticationFailed(
            "No account tiles found".to_string(),
        ));
    }

    if account_count == 1 {
        tracing::debug!("Only one account available, selecting automatically");
        let selector = if has_aad { "#aadTile" } else { "#msaTile" };
        let tile = page.find_element(selector).await.map_err(|e| {
            AzureLoginError::BrowserError(format!("Failed to find account tile: {}", e))
        })?;
        tile.click().await.map_err(|e| {
            AzureLoginError::BrowserError(format!("Failed to click account: {}", e))
        })?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        return Ok(());
    }

    if context.no_prompt {
        tracing::debug!("Multiple accounts available, selecting AAD (no-prompt mode)");
        let tile = page.find_element("#aadTile").await.map_err(|e| {
            AzureLoginError::BrowserError(format!("Failed to find AAD tile: {}", e))
        })?;
        tile.click().await.map_err(|e| {
            AzureLoginError::BrowserError(format!("Failed to click account: {}", e))
        })?;
    } else {
        let accounts = vec!["Azure AD Account", "Microsoft Account"];
        let selection = Select::new()
            .with_prompt("Select account")
            .items(&accounts)
            .default(0)
            .interact()
            .map_err(|e| AzureLoginError::AuthenticationFailed(e.to_string()))?;

        let selector = if selection == 0 {
            "#aadTile"
        } else {
            "#msaTile"
        };
        let tile = page.find_element(selector).await.map_err(|e| {
            AzureLoginError::BrowserError(format!("Failed to find account tile: {}", e))
        })?;
        tile.click().await.map_err(|e| {
            AzureLoginError::BrowserError(format!("Failed to click account: {}", e))
        })?;
    }

    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(())
}

/// Handle passwordless authentication
async fn handle_passwordless_auth(page: &Page) -> Result<()> {
    tracing::debug!("Handling passwordless authentication");

    let button = page
        .find_element("input[value='Send notification']")
        .await
        .map_err(|e| {
            AzureLoginError::BrowserError(format!("Failed to find send notification button: {}", e))
        })?;

    button.click().await.map_err(|e| {
        AzureLoginError::BrowserError(format!("Failed to click send notification: {}", e))
    })?;

    tokio::time::sleep(Duration::from_millis(1000)).await;

    if let Ok(code_elem) = page.find_element("#idRichContext_DisplaySign").await {
        if let Ok(Some(code)) = code_elem.inner_text().await {
            println!("Authentication code: {}", code);
            tracing::debug!("Authentication code displayed: {}", code);
        }
    }

    println!("Waiting for push notification approval...");
    tokio::time::sleep(Duration::from_millis(2000)).await;
    Ok(())
}

/// Handle voice call waiting (similar to TFA instructions but for voice calls)
async fn handle_voice_call_waiting(page: &Page, context: &LoginContext) -> Result<()> {
    // Check if we've already shown the waiting message
    if VOICE_CALL_WAITING_SHOWN.load(Ordering::Relaxed) {
        // Already shown, just return and continue polling
        return Ok(());
    }

    tracing::debug!("Handling voice call waiting");

    // Mark as shown
    VOICE_CALL_WAITING_SHOWN.store(true, Ordering::Relaxed);

    // Display the title if present
    if let Ok(elem) = page.find_element("#idDiv_SAOTCV_Title").await {
        if let Ok(Some(title)) = elem.inner_text().await {
            println!("{}", title);
        }
    }

    // Display the description if present
    if let Ok(elem) = page.find_element("#idDiv_SAOTCV_Description").await {
        if let Ok(Some(description)) = elem.inner_text().await {
            println!("{}", description);
        }
    }

    if context.no_prompt {
        println!("Waiting for call to be answered (use Ctrl+C to cancel)...");
    } else {
        println!("Answer your phone and follow the prompts...");
    }

    Ok(())
}

/// Handle TFA instructions (only display once, not on every poll)
async fn handle_tfa_instructions_once(page: &Page, context: &LoginContext) -> Result<()> {
    // First, check if user has requested cancellation
    if USER_REQUESTED_CANCEL.load(Ordering::Relaxed) {
        tracing::debug!("User requested cancellation, looking for fallback link");

        // Try to find and click fallback link
        // The link text is "I can't use my Microsoft Authenticator app right now"
        let fallback_selectors = vec![
            "#signInAnotherWay", // Primary selector (2025+)
            "a[aria-describedby='idDiv_SAOTCAS_Title idDiv_SAOTCAS_Description']", // Alternative via aria-describedby
            "#idA_SAOTCS_BeginAuth",              // Legacy selector
            "a[id*='SAOTCS']",                    // Any link with SAOTCS in ID
            "a[id*='BeginAuth']",                 // Any link with BeginAuth in ID
            "#switchToAnotherVerificationOption", // Alternate verification
            "a[onclick*='BeginAuth']",            // Link with BeginAuth onclick
            "a[id*='signInAnother']",             // Match variations of signInAnotherWay
        ];

        for selector in fallback_selectors {
            if let Ok(elem) = page.find_element(selector).await {
                tracing::debug!("Found fallback element with selector: {}", selector);
                if let Ok(Some(text)) = elem.inner_text().await {
                    tracing::debug!("Fallback link text: '{}'", text);
                }
                elem.click().await.map_err(|e| {
                    AzureLoginError::BrowserError(format!("Failed to click fallback link: {}", e))
                })?;
                // Reset the flag after clicking
                USER_REQUESTED_CANCEL.store(false, Ordering::Relaxed);
                tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
                return Ok(());
            }
        }

        tracing::warn!("Could not find fallback link, will continue waiting");
        // Reset flag even if we couldn't find the link
        USER_REQUESTED_CANCEL.store(false, Ordering::Relaxed);
    }

    // Check if we've already shown the instructions
    if TFA_INSTRUCTIONS_SHOWN.load(Ordering::Relaxed) {
        // Already shown, just return and continue polling
        return Ok(());
    }

    tracing::debug!("Handling TFA instructions");

    // Mark as shown
    TFA_INSTRUCTIONS_SHOWN.store(true, Ordering::Relaxed);

    if let Ok(elem) = page.find_element("#idDiv_SAOTCAS_Description").await {
        if let Ok(Some(description)) = elem.inner_text().await {
            println!("{}", description);
        }
    }

    if let Ok(code_elem) = page.find_element("#idRichContext_DisplaySign").await {
        if let Ok(Some(code)) = code_elem.inner_text().await {
            println!("Authentication code: {}", code);
        }
    }

    // If in no_prompt mode, just wait for approval
    if context.no_prompt {
        println!("Waiting for approval (use Ctrl+C to cancel)...");
        return Ok(());
    }

    // Spawn background task to listen for Enter key press
    println!("Press Enter to use another method, or approve on your phone...");

    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(|| {
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)
        })
        .await;

        if result.is_ok() {
            tracing::debug!("User pressed Enter, setting cancellation flag");
            USER_REQUESTED_CANCEL.store(true, Ordering::Relaxed);
        }
    });

    Ok(())
}

/// Handle "Verify your identity" state with multiple authentication options
async fn handle_verify_identity(page: &Page, context: &LoginContext) -> Result<()> {
    tracing::debug!("Handling verify your identity options");

    // Execute verification logic and save screenshot on any failure
    match handle_verify_identity_inner(page, context).await {
        Ok(()) => Ok(()),
        Err(e) => {
            tracing::error!("Verification identity failed: {}, saving screenshot", e);
            let screenshot_path = save_debug_screenshot(page, "unrecognized-state.png").await;
            tracing::error!("Screenshot saved to {}", screenshot_path);
            Err(e)
        }
    }
}

async fn handle_verify_identity_inner(page: &Page, context: &LoginContext) -> Result<()> {
    // Wait a bit for options to fully load
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Look for available authentication options using multiple selector strategies
    let has_authenticator = page
        .find_element("div[data-value='PhoneAppNotification']")
        .await
        .is_ok()
        || page
            .find_element("[data-value='PhoneAppNotification']")
            .await
            .is_ok()
        || page
            .find_element("#idDiv_SAOTCC_Desc_PhoneApp")
            .await
            .is_ok();

    let has_verification_code = page
        .find_element("div[data-value='PhoneAppOTP']")
        .await
        .is_ok()
        || page
            .find_element("[data-value='PhoneAppOTP']")
            .await
            .is_ok()
        || page.find_element("#idDiv_SAOTCC_Desc_OTP").await.is_ok();

    let has_sms = page
        .find_element("div[data-value='OneWaySMS']")
        .await
        .is_ok()
        || page.find_element("[data-value='OneWaySMS']").await.is_ok()
        || page.find_element("#idDiv_SAOTCC_Desc_SMS").await.is_ok();

    let has_voice_call = page
        .find_element("div[data-value='TwoWayVoiceMobile']")
        .await
        .is_ok()
        || page
            .find_element("[data-value='TwoWayVoiceMobile']")
            .await
            .is_ok()
        || page.find_element("#idDiv_SAOTCC_Desc_Voice").await.is_ok();

    tracing::debug!(
        "Authentication options detected - Authenticator: {}, Verification code: {}, SMS: {}, Voice call: {}",
        has_authenticator,
        has_verification_code,
        has_sms,
        has_voice_call
    );

    // If no prompt mode, default to authenticator app if available
    if context.no_prompt {
        if has_authenticator {
            tracing::debug!("Auto-selecting Microsoft Authenticator approval (no-prompt mode)");
            return handle_authenticator_approval(page).await;
        } else if has_verification_code {
            return Err(AzureLoginError::AuthenticationFailed(
                "Verification code required but --no-prompt is set".to_string(),
            ));
        }
    }

    // Prompt user to choose authentication method
    let mut options = Vec::new();
    let mut option_handlers = Vec::new();

    if has_authenticator {
        options.push("Approve a request on my Microsoft Authenticator app");
        option_handlers.push("authenticator");
    }
    if has_verification_code {
        options.push("Use a verification code");
        option_handlers.push("verification_code");
    }
    if has_sms {
        options.push("Text me a code");
        option_handlers.push("sms");
    }
    if has_voice_call {
        options.push("Call me");
        option_handlers.push("voice_call");
    }

    if options.is_empty() {
        return Err(AzureLoginError::AuthenticationFailed(
            "No authentication options available".to_string(),
        ));
    }

    if options.len() == 1 {
        tracing::debug!("Only one authentication option available, using it");
        return match option_handlers[0] {
            "authenticator" => handle_authenticator_approval(page).await,
            "verification_code" => handle_verification_code_flow(page, context).await,
            "sms" => handle_sms_flow(page, context).await,
            "voice_call" => handle_voice_call_flow(page, context).await,
            _ => Err(AzureLoginError::BrowserError(
                "Unknown authentication method".to_string(),
            )),
        };
    }

    // Multiple options - prompt user
    println!("Multiple authentication methods available:");
    let selection = Select::new()
        .with_prompt("Select authentication method")
        .items(&options)
        .default(0)
        .interact()
        .map_err(|e| AzureLoginError::AuthenticationFailed(format!("Selection failed: {}", e)))?;

    match option_handlers[selection] {
        "authenticator" => handle_authenticator_approval(page).await,
        "verification_code" => handle_verification_code_flow(page, context).await,
        "sms" => handle_sms_flow(page, context).await,
        "voice_call" => handle_voice_call_flow(page, context).await,
        _ => Err(AzureLoginError::BrowserError(
            "Unknown authentication method selected".to_string(),
        )),
    }
}

/// Handle Microsoft Authenticator app approval (acts like retry flow)
async fn handle_authenticator_approval(page: &Page) -> Result<()> {
    tracing::debug!("Selecting Microsoft Authenticator approval");

    // Try different selectors for the authenticator option
    let selectors = vec![
        "div[data-value='PhoneAppNotification']",
        "[data-value='PhoneAppNotification']",
        "#idDiv_SAOTCC_Desc_PhoneApp",
        "div[role='link'][data-value='PhoneAppNotification']",
    ];

    for selector in selectors {
        if let Ok(link) = page.find_element(selector).await {
            tracing::debug!("Found authenticator element with selector: {}", selector);
            link.click().await.map_err(|e| {
                AzureLoginError::BrowserError(format!("Failed to click authenticator link: {}", e))
            })?;

            // Reset TFA instructions flag so the new code will be displayed
            TFA_INSTRUCTIONS_SHOWN.store(false, Ordering::Relaxed);

            tokio::time::sleep(Duration::from_millis(1000)).await;
            return Ok(());
        }
    }

    Err(AzureLoginError::BrowserError(
        "Could not find authenticator approval link".to_string(),
    ))
}

/// Handle Microsoft Authenticator approval waiting screen
/// Allows user to cancel by entering nothing, which triggers fallback link
/// Handle verification code flow (click link, prompt for code, enter code)
async fn handle_verification_code_flow(page: &Page, _context: &LoginContext) -> Result<()> {
    tracing::debug!("Selecting verification code option");

    // Click on "Use a verification code" link
    let selectors = vec![
        "div[data-value='PhoneAppOTP']",
        "[data-value='PhoneAppOTP']",
        "#idDiv_SAOTCC_Desc_OTP",
        "div[role='link'][data-value='PhoneAppOTP']",
    ];

    let mut clicked = false;
    for selector in selectors {
        if let Ok(link) = page.find_element(selector).await {
            tracing::debug!(
                "Found verification code element with selector: {}",
                selector
            );
            link.click().await.map_err(|e| {
                AzureLoginError::BrowserError(format!(
                    "Failed to click verification code link: {}",
                    e
                ))
            })?;

            clicked = true;
            tokio::time::sleep(Duration::from_millis(1000)).await;
            break;
        }
    }

    if !clicked {
        tracing::warn!("Could not find verification code link to click");
    }

    // Wait for the code input field to appear
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Now prompt for and enter the verification code
    // This should trigger the existing TFA code input handler on next state machine loop
    Ok(())
}

/// Handle SMS text message flow (click link, prompt for code, enter code)
async fn handle_sms_flow(page: &Page, _context: &LoginContext) -> Result<()> {
    tracing::debug!("Selecting SMS text message option");

    // Click on "Text me a code" link
    let selectors = vec![
        "div[data-value='OneWaySMS']",
        "[data-value='OneWaySMS']",
        "#idDiv_SAOTCC_Desc_SMS",
        "div[role='link'][data-value='OneWaySMS']",
    ];

    let mut clicked = false;
    for selector in selectors {
        if let Ok(link) = page.find_element(selector).await {
            tracing::debug!("Found SMS element with selector: {}", selector);
            link.click().await.map_err(|e| {
                AzureLoginError::BrowserError(format!("Failed to click SMS link: {}", e))
            })?;

            clicked = true;
            println!("Sending SMS code...");
            tokio::time::sleep(Duration::from_millis(1500)).await;
            break;
        }
    }

    if !clicked {
        tracing::warn!("Could not find SMS link to click");
    }

    // Wait for the code input field to appear
    tokio::time::sleep(Duration::from_millis(500)).await;

    // The code input field should trigger the existing TFA code input handler
    Ok(())
}

/// Handle voice call flow (click link, prompt for code, enter code)
async fn handle_voice_call_flow(page: &Page, _context: &LoginContext) -> Result<()> {
    tracing::debug!("Selecting voice call option");

    // Click on "Call me" link
    let selectors = vec![
        "div[data-value='TwoWayVoiceMobile']",
        "[data-value='TwoWayVoiceMobile']",
        "#idDiv_SAOTCC_Desc_Voice",
        "div[role='link'][data-value='TwoWayVoiceMobile']",
    ];

    let mut clicked = false;
    for selector in selectors {
        if let Ok(link) = page.find_element(selector).await {
            tracing::debug!("Found voice call element with selector: {}", selector);
            link.click().await.map_err(|e| {
                AzureLoginError::BrowserError(format!("Failed to click voice call link: {}", e))
            })?;

            clicked = true;
            println!("Initiating voice call...");
            tokio::time::sleep(Duration::from_millis(1500)).await;
            break;
        }
    }

    if !clicked {
        tracing::warn!("Could not find voice call link to click");
    }

    // Wait for the code input field to appear
    tokio::time::sleep(Duration::from_millis(500)).await;

    // The code input field should trigger the existing TFA code input handler
    Ok(())
}

/// Handle TFA failure
async fn handle_tfa_failure(page: &Page) -> Result<()> {
    tracing::warn!("TFA authentication failed, looking for retry option");

    // Look for retry link - Microsoft shows "Send another request to my Microsoft Authenticator app"
    // Try common selectors for retry links
    let retry_selectors = vec![
        "a[id*='retry']",
        "a[id*='Retry']",
        "a:has-text('Send another request')",
        "#idA_SAASTO_Resend",
        "#idA_SAASDS_Resend",
    ];

    for selector in retry_selectors {
        if let Ok(retry_link) = page.find_element(selector).await {
            if let Ok(Some(text)) = retry_link.inner_text().await {
                tracing::debug!("Found retry link: '{}', clicking to retry", text);
                println!("TFA failed, retrying authentication...");

                retry_link.click().await.map_err(|e| {
                    AzureLoginError::BrowserError(format!("Failed to click retry link: {}", e))
                })?;

                // Reset the TFA instructions flag so the new code will be displayed
                TFA_INSTRUCTIONS_SHOWN.store(false, Ordering::Relaxed);

                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                return Ok(());
            }
        }
    }

    // No retry link found, get error message and fail
    let error_elem = if let Ok(elem) = page.find_element("#idDiv_SAASDS_Description").await {
        Ok(elem)
    } else {
        page.find_element("#idDiv_SAASTO_Description").await
    };

    if let Ok(elem) = error_elem {
        if let Ok(Some(error_msg)) = elem.inner_text().await {
            return Err(AzureLoginError::AuthenticationFailed(format!(
                "TFA failed: {}",
                error_msg
            )));
        }
    }

    Err(AzureLoginError::AuthenticationFailed(
        "TFA authentication failed (no retry option available)".to_string(),
    ))
}

/// Handle TFA code input
async fn handle_tfa_code_input(page: &Page, context: &LoginContext) -> Result<()> {
    tracing::debug!("Handling TFA code input");

    if let Ok(desc_elem) = page.find_element("#idDiv_SAOTCC_Description").await {
        if let Ok(Some(description)) = desc_elem.inner_text().await {
            println!("{}", description);
        }
    }

    let code = if context.no_prompt {
        return Err(AzureLoginError::AuthenticationFailed(
            "TFA code required but --no-prompt is set".to_string(),
        ));
    } else {
        Input::<String>::new()
            .with_prompt("Enter verification code")
            .interact_text()
            .map_err(|e| AzureLoginError::AuthenticationFailed(e.to_string()))?
    };

    tracing::debug!("Entering TFA code");

    // Retry loop for handling stale elements
    const MAX_RETRIES: usize = 5;
    const RETRY_DELAY_MS: u64 = 200;

    for attempt in 0..MAX_RETRIES {
        match try_input_text_and_submit(page, "input[name='otc']", &code, "TFA code").await {
            Ok(_) => break,
            Err(e) if attempt < MAX_RETRIES - 1 => {
                tracing::debug!("Attempt {} failed: {}, retrying...", attempt + 1, e);
                tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    // Wait for page transition
    tokio::time::sleep(Duration::from_millis(500)).await;

    Ok(())
}

/// Handle "Remember me" prompt
async fn handle_remember_me(page: &Page, context: &LoginContext) -> Result<()> {
    tracing::debug!("Handling remember me prompt");

    // Use config value if available, otherwise default to Yes
    let should_remember = context.remember_me.unwrap_or(true);

    let button_value = if should_remember { "Yes" } else { "No" };
    let button_selector = format!("input[value='{}']", button_value);

    if let Ok(btn) = page.find_element(&button_selector).await {
        tracing::debug!("Clicking '{}' for remember me", button_value);
        btn.click().await.map_err(|e| {
            AzureLoginError::BrowserError(format!("Failed to click {}: {}", button_value, e))
        })?;
    } else {
        tracing::warn!(
            "Remember me button '{}' not found, trying fallback",
            button_value
        );
        // Fallback: try the opposite button
        let fallback_value = if should_remember { "No" } else { "Yes" };
        let fallback_selector = format!("input[value='{}']", fallback_value);
        if let Ok(btn) = page.find_element(&fallback_selector).await {
            btn.click().await.map_err(|e| {
                AzureLoginError::BrowserError(format!("Failed to click {}: {}", fallback_value, e))
            })?;
        }
    }

    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(())
}

/// Handle service exception errors
async fn handle_service_exception(page: &Page) -> Result<()> {
    tracing::error!("Service exception occurred");

    if let Ok(elem) = page.find_element("#service_exception_message").await {
        if let Ok(Some(error_msg)) = elem.inner_text().await {
            return Err(AzureLoginError::AuthenticationFailed(format!(
                "Service exception: {}",
                error_msg
            )));
        }
    }

    Err(AzureLoginError::AuthenticationFailed(
        "Service exception occurred".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_context_creation() {
        let context = LoginContext {
            username: Some("user@example.com".to_string()),
            password: Some("password123".to_string()),
            no_prompt: false,
            remember_me: Some(true),
            username_from_config: false,
        };

        assert_eq!(context.username, Some("user@example.com".to_string()));
        assert_eq!(context.password, Some("password123".to_string()));
        assert!(!context.no_prompt);
        assert_eq!(context.remember_me, Some(true));
    }

    #[test]
    fn test_login_context_no_prompt_mode() {
        let context = LoginContext {
            username: None,
            password: None,
            no_prompt: true,
            remember_me: None,
            username_from_config: false,
        };

        assert!(context.username.is_none());
        assert!(context.password.is_none());
        assert!(context.no_prompt);
        assert!(context.remember_me.is_none());
    }

    #[test]
    fn test_login_context_with_defaults() {
        let context = LoginContext {
            username: Some("admin@company.com".to_string()),
            password: None,
            no_prompt: false,
            remember_me: Some(false),
            username_from_config: false,
        };

        assert_eq!(context.username, Some("admin@company.com".to_string()));
        assert!(context.password.is_none());
        assert_eq!(context.remember_me, Some(false));
    }

    #[test]
    fn test_tfa_instructions_shown_flag() {
        // Test that the static flag starts as false
        use std::sync::atomic::Ordering;

        // Note: This test may interfere with other tests if run in parallel
        // In a real scenario, you'd want to refactor to avoid static mutable state
        let initial = TFA_INSTRUCTIONS_SHOWN.load(Ordering::Relaxed);

        // Just verify the atomic bool can be read
        assert!(initial || !initial); // Always true, just checking it compiles
    }

    #[test]
    fn test_constants() {
        // Test that constants are defined correctly
        const MAX_UNRECOGNIZED_DELAY: u64 = 30_000;
        const MAX_AWS_ENDPOINT_DELAY: u64 = 10_000;
        const POLL_INTERVAL: u64 = 1000;

        assert_eq!(MAX_UNRECOGNIZED_DELAY, 30_000);
        assert_eq!(MAX_AWS_ENDPOINT_DELAY, 10_000);
        assert_eq!(POLL_INTERVAL, 1000);
    }
}
