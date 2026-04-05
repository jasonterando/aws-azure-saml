# aws-azure-saml

A Rust port of [aws-azure-login](https://github.com/dtjohnson/aws-azure-login) - a CLI tool that enables AWS CLI authentication using Azure Active Directory SAML SSO.

## Features

- **Drop-in replacement**: Same CLI flags and config files as aws-azure-login
- **Cross-platform**: Linux, macOS, Windows
- **Secure**: Automated Azure AD login with MFA support
- **Single Authenticaton per Tenant**: Multiple AWS profiles using same Azure Tenant require only one authentication

## Installation

### From Pre-built Binaries

Download the latest release from [GitHub Releases](https://github.com/jasonterando/aws-azure-saml/releases).

### From Source

```bash
git clone https://github.com/dtjohnson/aws-azure-login.git
cd aws-azure-login/rust
cargo build --release
```

The binary will be available at `target/release/aws-azure-saml`.

### Using Cargo

```bash
cargo install aws-azure-saml
```

## Requirements

- **Chromium/Chrome**: Must be installed on your system for browser automation
- **AWS CLI**: For using the obtained credentials

## Quick Start

### 1. Configure a Profile

```bash
aws-azure-saml --configure --profile my-profile
```

You'll be prompted for:
- Azure Tenant ID
- Azure App ID URI (default: `https://signin.aws.amazon.com/saml`)
- Default username (optional)
- Default role ARN (optional)
- Session duration (1-12 hours)
- Remember me preference
- AWS region (optional)

### 2. Login

```bash
aws-azure-saml --profile my-profile
```

This will:
1. Open a browser (headless by default)
2. Automate the Azure AD login flow
3. Handle MFA if required
4. Obtain temporary AWS credentials
5. Store them in `~/.aws/credentials`

### 3. Use AWS CLI

```bash
aws sts get-caller-identity --profile my-profile
```

## Usage

### CLI Options

```
USAGE:
    aws-azure-saml [OPTIONS]

OPTIONS:
    -p, --profile <NAME>              Profile name (default: $AWS_PROFILE or "default")
    -a, --all-profiles                Run for all configured profiles
    -f, --force-refresh               Force refresh even if credentials are valid
    -c, --configure                   Configure a profile interactively
    -m, --mode <MODE>                 Login mode: cli (headless), gui (visible), debug
        --no-sandbox                  Disable Chromium sandbox (usually needed on Linux)
        --no-prompt                   Use defaults, don't prompt for input
        --enable-chrome-network-service  Enable Network Service for 3XX redirects
        --no-verify-ssl               Disable SSL verification for AWS connections
        --enable-chrome-seamless-sso  Enable Azure AD Seamless SSO
        --no-disable-extensions       Don't disable Chrome extensions
        --disable-gpu                 Disable GPU in Chromium
    -h, --help                        Print help
    -V, --version                     Print version
```

### Login Modes

- **cli** (default): Headless browser, fully automated
- **gui**: Visible browser window, manual login
- **debug**: Visible browser with automation (for debugging)

### Examples

```bash
# Configure a new profile
aws-azure-saml --configure --profile production

# Login with default profile (headless)
aws-azure-saml

# Login with GUI mode (for debugging or manual MFA)
aws-azure-saml --profile production --mode gui

# Login to all configured profiles
aws-azure-saml --all-profiles

# Force refresh credentials
aws-azure-saml --profile production --force-refresh

# Use proxy
https_proxy=http://proxy.example.com:8080 aws-azure-saml --profile production
```

## Configuration

Configuration is stored in `~/.aws/config`:

```ini
[profile my-profile]
azure_tenant_id = xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
azure_app_id_uri = https://signin.aws.amazon.com/saml
azure_default_username = user@example.com
azure_default_role_arn = arn:aws:iam::123456789012:role/MyRole
azure_default_duration_hours = 1
azure_default_remember_me = false
region = us-east-1
```

Credentials are stored in `~/.aws/credentials`:

```ini
[my-profile]
aws_access_key_id = AKIAIOSFODNN7EXAMPLE
aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY
aws_session_token = AQoDYXdzEJr...
aws_expiration = 2024-01-15T14:30:00.000Z
```

## Environment Variables

- `AWS_PROFILE`: Default profile name
- `AWS_CONFIG_FILE`: Override config file path
- `AWS_SHARED_CREDENTIALS_FILE`: Override credentials file path
- `https_proxy`: HTTP proxy server
- `RUST_LOG`: Logging level (e.g., `RUST_LOG=debug`)

## Comparison with TypeScript Version

| Metric | TypeScript (Node.js) | Rust |
|--------|---------------------|------|
| Binary Size | 200MB+ (with node_modules) | 10-15MB |
| Startup Time | 1-2 seconds | <100ms |
| Memory Usage | 150-300MB | 50-100MB |
| Dependencies | Node.js + npm | None (statically linked) |
| Cold Start | 2-3 seconds | <200ms |

## Troubleshooting

### Browser Fails to Launch (Linux)

Try with `--no-sandbox`:

```bash
aws-azure-saml --profile my-profile --no-sandbox
```

### MFA Not Working

Use GUI mode to see what's happening:

```bash
aws-azure-saml --profile my-profile --mode gui
```

### Credentials Not Refreshing

Force a refresh:

```bash
aws-azure-saml --profile my-profile --force-refresh
```

### Enable Debug Logging

```bash
RUST_LOG=debug aws-azure-saml --profile my-profile
```

### Proxy Issues

Ensure `https_proxy` is set:

```bash
export https_proxy=http://proxy.example.com:8080
aws-azure-saml --profile my-profile
```

## Development

### Build

```bash
cd rust
cargo build
```

### Run with Debugging

```bash
RUST_LOG=debug cargo run -- --profile test --mode debug
```

### Run Tests

```bash
cargo test
```

### Release Build

```bash
cargo build --release
```

The optimized binary will be at `target/release/aws-azure-saml`.

## Architecture

```
src/
├── main.rs              # Entry point
├── cli.rs               # CLI argument parsing
├── error.rs             # Error types
├── config/              # Configuration management
│   ├── paths.rs         # File paths
│   ├── profile.rs       # Profile config
│   └── credentials.rs   # Credential storage
├── aws/                 # AWS integration
│   ├── saml.rs          # SAML request/response
│   └── sts.rs           # STS AssumeRoleWithSAML
├── azure/               # Azure AD automation
│   ├── browser.rs       # Browser launch
│   ├── login.rs         # Login orchestration
│   └── state_machine.rs # Page state handling
└── prompts/             # Interactive prompts
    └── configure.rs     # Profile configuration
```

## License

MIT

## Credits

- Original TypeScript version: [dtjohnson/aws-azure-login](https://github.com/dtjohnson/aws-azure-login)
- Browser automation: [chromiumoxide](https://github.com/mattsse/chromiumoxide)
- AWS SDK: [aws-sdk-rust](https://github.com/awslabs/aws-sdk-rust)
