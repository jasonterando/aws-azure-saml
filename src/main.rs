mod aws;
mod azure;
mod cli;
mod config;
mod error;
mod prompts;

use clap::Parser;
use cli::Cli;
use error::AzureLoginError;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "aws_azure_saml_rs=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse CLI arguments
    let cli = Cli::parse();

    // Validate CLI arguments
    if let Err(e) = cli.validate() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    // Run the application
    let result = run(cli).await;

    // Handle errors
    match result {
        Ok(_) => {
            std::process::exit(0);
        }
        Err(AzureLoginError::ProfileNotFound(ref profile)) => {
            eprintln!("Error: Profile '{}' not found.", profile);
            eprintln!("Run with --configure to set up a new profile.");
            std::process::exit(2);
        }
        Err(AzureLoginError::MissingAzureConfig(ref profile)) => {
            eprintln!(
                "Error: Profile '{}' is missing required Azure configuration.",
                profile
            );
            eprintln!("Run with --configure to add Azure settings to the profile.");
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn run(cli: Cli) -> Result<(), AzureLoginError> {
    if cli.configure {
        // Configure profile
        prompts::configure_profile(&cli.profile)?;
        println!("Profile '{}' configured successfully.", cli.profile);
        Ok(())
    } else if cli.all_profiles {
        // Login to all profiles
        azure::login_all(&cli).await?;
        Ok(())
    } else {
        // Login to single profile
        azure::login_async(&cli.profile, &cli).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        // Test that basic CLI parsing works
        use clap::Parser;

        let args = vec!["aws-azure-saml", "--profile", "test"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.profile, "test");
        assert!(!cli.all_profiles);
        assert!(!cli.configure);
    }

    #[test]
    fn test_cli_all_profiles_flag() {
        use clap::Parser;

        let args = vec!["aws-azure-saml", "--all-profiles"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert!(cli.all_profiles);
        assert_eq!(cli.profile, "default");
    }

    #[test]
    fn test_cli_configure_flag() {
        use clap::Parser;

        let args = vec!["aws-azure-saml", "--configure", "--profile", "production"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert!(cli.configure);
        assert_eq!(cli.profile, "production");
    }

    #[test]
    fn test_cli_mode_parsing() {
        use clap::Parser;

        let args = vec!["aws-azure-saml", "--mode", "gui"];
        let cli = Cli::try_parse_from(args).unwrap();

        assert_eq!(cli.mode, cli::LoginMode::Gui);
    }

    #[test]
    fn test_cli_flags() {
        use clap::Parser;

        let args = vec![
            "aws-azure-saml",
            "--no-sandbox",
            "--force-refresh",
            "--no-prompt",
            "--disable-gpu",
        ];
        let cli = Cli::try_parse_from(args).unwrap();

        assert!(cli.no_sandbox);
        assert!(cli.force_refresh);
        assert!(cli.no_prompt);
        assert!(cli.disable_gpu);
    }
}
