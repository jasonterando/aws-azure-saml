use thiserror::Error;

#[derive(Error, Debug)]
pub enum AzureLoginError {
    #[error("Profile '{0}' not found. Run with --configure to set up a new profile.")]
    ProfileNotFound(String),

    #[error("Profile '{0}' is missing required Azure configuration. Run with --configure.")]
    MissingAzureConfig(String),

    #[error("Browser automation failed: {0}")]
    BrowserError(String),

    #[error("Azure authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("No roles found in SAML response. Verify your Azure AD app configuration.")]
    NoRolesFound,

    #[error("SAML response parsing failed: {0}")]
    SamlParsingError(String),

    #[error("AWS STS error: {0}")]
    StsError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("INI parsing error: {0}")]
    IniError(String),

    #[error("Unrecognized page state. Screenshot saved to: {0}")]
    UnrecognizedPageState(String),
}

pub type Result<T> = std::result::Result<T, AzureLoginError>;
