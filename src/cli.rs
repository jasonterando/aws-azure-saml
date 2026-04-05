use clap::Parser;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginMode {
    Cli,
    Gui,
    Debug,
}

impl std::str::FromStr for LoginMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cli" => Ok(LoginMode::Cli),
            "gui" => Ok(LoginMode::Gui),
            "debug" => Ok(LoginMode::Debug),
            _ => Err(format!(
                "Invalid mode: '{}'. Valid modes are: cli, gui, debug",
                s
            )),
        }
    }
}

impl std::fmt::Display for LoginMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoginMode::Cli => write!(f, "cli"),
            LoginMode::Gui => write!(f, "gui"),
            LoginMode::Debug => write!(f, "debug"),
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "aws-azure-saml")]
#[command(about = "AWS CLI authentication using Azure Active Directory SAML SSO", long_about = None)]
#[command(version)]
pub struct Cli {
    /// The name of the profile to log in with (or configure)
    #[arg(short, long, env = "AWS_PROFILE", default_value = "default")]
    pub profile: String,

    /// Run for all configured profiles
    #[arg(short = 'a', long)]
    pub all_profiles: bool,

    /// Force a credential refresh, even if they are still valid
    #[arg(short = 'f', long)]
    pub force_refresh: bool,

    /// Configure the profile
    #[arg(short = 'c', long)]
    pub configure: bool,

    /// Login mode: 'cli' (headless, default), 'gui' (visible browser), or 'debug' (visible with automation)
    #[arg(short = 'm', long, default_value = "cli", value_parser = clap::value_parser!(LoginMode))]
    pub mode: LoginMode,

    /// Disable the Chromium sandbox (usually necessary on Linux)
    #[arg(long)]
    pub no_sandbox: bool,

    /// Do not prompt for input and accept the default choice
    #[arg(long)]
    pub no_prompt: bool,

    /// Enable Chromium's Network Service (needed when login provider redirects with 3XX)
    #[arg(long)]
    pub enable_chrome_network_service: bool,

    /// Disable SSL Peer Verification for connections to AWS (no effect if behind proxy)
    #[arg(long)]
    pub no_verify_ssl: bool,

    /// Enable Chromium's pass-through authentication with Azure AD Seamless SSO
    #[arg(long)]
    pub enable_chrome_seamless_sso: bool,

    /// Don't pass the --disable-extensions flag to Chromium
    #[arg(long)]
    pub no_disable_extensions: bool,

    /// Tell Chromium to pass the --disable-gpu flag
    #[arg(long)]
    pub disable_gpu: bool,
}

impl Cli {
    pub fn validate(&self) -> Result<(), String> {
        // Validate that --all-profiles is not used with --profile
        if self.all_profiles && self.profile != "default" && std::env::var("AWS_PROFILE").is_err() {
            return Err("Cannot specify both --all-profiles and --profile".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_cli(profile: &str, all_profiles: bool) -> Cli {
        Cli {
            profile: profile.to_string(),
            all_profiles,
            force_refresh: false,
            configure: false,
            mode: LoginMode::Cli,
            no_sandbox: false,
            no_prompt: false,
            enable_chrome_network_service: false,
            no_verify_ssl: false,
            enable_chrome_seamless_sso: false,
            no_disable_extensions: false,
            disable_gpu: false,
        }
    }

    #[test]
    fn test_login_mode_from_str() {
        assert_eq!("cli".parse::<LoginMode>().unwrap(), LoginMode::Cli);
        assert_eq!("gui".parse::<LoginMode>().unwrap(), LoginMode::Gui);
        assert_eq!("debug".parse::<LoginMode>().unwrap(), LoginMode::Debug);
        assert_eq!("CLI".parse::<LoginMode>().unwrap(), LoginMode::Cli);
        assert_eq!("GUI".parse::<LoginMode>().unwrap(), LoginMode::Gui);

        assert!("invalid".parse::<LoginMode>().is_err());
    }

    #[test]
    fn test_login_mode_display() {
        assert_eq!(LoginMode::Cli.to_string(), "cli");
        assert_eq!(LoginMode::Gui.to_string(), "gui");
        assert_eq!(LoginMode::Debug.to_string(), "debug");
    }

    #[test]
    fn test_cli_validate_default_profile() {
        let cli = create_test_cli("default", false);
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn test_cli_validate_all_profiles_only() {
        let cli = create_test_cli("default", true);
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn test_cli_validate_single_profile_only() {
        let cli = create_test_cli("production", false);
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn test_cli_validate_all_profiles_with_custom_profile_fails() {
        // This should fail when AWS_PROFILE env var is not set
        std::env::remove_var("AWS_PROFILE");
        let cli = create_test_cli("production", true);
        assert!(cli.validate().is_err());
        assert_eq!(
            cli.validate().unwrap_err(),
            "Cannot specify both --all-profiles and --profile"
        );
    }

    #[test]
    fn test_cli_validate_respects_aws_profile_env() {
        // When AWS_PROFILE is set, the validation should pass
        std::env::set_var("AWS_PROFILE", "test");
        let cli = create_test_cli("test", true);
        // This passes because AWS_PROFILE env var is set
        assert!(cli.validate().is_ok());
        std::env::remove_var("AWS_PROFILE");
    }
}
