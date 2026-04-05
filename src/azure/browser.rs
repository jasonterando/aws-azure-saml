use crate::cli::{Cli, LoginMode};
use crate::config::Paths;
use crate::error::{AzureLoginError, Result};
use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use std::time::Duration;

const AZURE_AD_SSO: &str = "autologon.microsoftazuread-sso.com";

#[derive(Debug, Clone)]
pub struct BrowserOptions {
    pub mode: LoginMode,
    pub no_sandbox: bool,
    pub enable_chrome_network_service: bool,
    pub enable_chrome_seamless_sso: bool,
    pub no_disable_extensions: bool,
    pub disable_gpu: bool,
}

impl From<&Cli> for BrowserOptions {
    fn from(cli: &Cli) -> Self {
        BrowserOptions {
            mode: cli.mode,
            no_sandbox: cli.no_sandbox,
            enable_chrome_network_service: cli.enable_chrome_network_service,
            enable_chrome_seamless_sso: cli.enable_chrome_seamless_sso,
            no_disable_extensions: cli.no_disable_extensions,
            disable_gpu: cli.disable_gpu,
        }
    }
}

pub async fn launch_browser(options: &BrowserOptions) -> Result<Browser> {
    let paths = Paths::new();
    let headless = options.mode == LoginMode::Cli;

    // Clean up chromiumoxide's default temp directory if it exists
    // This prevents singleton lock issues from previous runs
    let chromiumoxide_temp = std::path::Path::new("/tmp/chromiumoxide-runner");
    if chromiumoxide_temp.exists() {
        if let Err(e) = std::fs::remove_dir_all(chromiumoxide_temp) {
            tracing::warn!("Failed to clean up chromiumoxide temp directory: {}", e);
        }
    }

    let mut args = Vec::new();

    // Window size for non-headless mode
    if !headless {
        const WIDTH: u32 = 425;
        const HEIGHT: u32 = 550;
        args.push(format!("--window-size={},{}", WIDTH, HEIGHT));
    }

    // Sandbox
    if options.no_sandbox {
        args.push("--no-sandbox".to_string());
    }

    // Chrome network service
    if options.enable_chrome_network_service {
        args.push("--enable-features=NetworkService".to_string());
    }

    // Azure SSO
    if options.enable_chrome_seamless_sso {
        args.push(format!("--auth-server-whitelist={}", AZURE_AD_SSO));
        args.push(format!(
            "--auth-negotiate-delegate-whitelist={}",
            AZURE_AD_SSO
        ));
    }

    // GPU
    if options.disable_gpu {
        args.push("--disable-gpu".to_string());
    }

    // Proxy
    if let Ok(proxy) = std::env::var("https_proxy") {
        args.push(format!("--proxy-server={}", proxy));
        tracing::debug!("Using proxy: {}", proxy);
    }

    // User data directory - use a unique directory to avoid lock conflicts
    let user_data_dir = paths.chromium.to_string_lossy().to_string();
    args.push(format!("--user-data-dir={}", user_data_dir));

    // Prevent singleton lock issues
    args.push("--no-first-run".to_string());
    args.push("--no-default-browser-check".to_string());
    args.push("--disable-default-apps".to_string());

    // Disable extensions unless explicitly enabled
    if !options.no_disable_extensions {
        args.push("--disable-extensions".to_string());
    }

    // Accept language
    args.push("--lang=en-US".to_string());

    tracing::debug!("Launching browser with args: {:?}", args);

    // Build browser config
    let mut builder = BrowserConfig::builder();

    // Show browser window for GUI and Debug modes (with_head() means "with head" i.e., NOT headless)
    if !headless {
        builder = builder.with_head();
    }

    builder = builder
        .args(args.iter().map(|s| s.as_str()))
        .request_timeout(Duration::from_secs(60));

    let (browser, mut handler) = Browser::launch(builder.build().map_err(|e| {
        AzureLoginError::BrowserError(format!("Failed to build browser config: {}", e))
    })?)
    .await
    .map_err(|e| AzureLoginError::BrowserError(format!("Failed to launch browser: {}", e)))?;

    // Spawn handler task
    tokio::spawn(async move {
        loop {
            if handler.next().await.is_none() {
                break;
            }
        }
    });

    tracing::debug!("Browser launched successfully");

    Ok(browser)
}
