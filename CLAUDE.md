# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

aws-azure-saml is a Rust port of the TypeScript aws-azure-login CLI tool. It enables AWS CLI authentication using Azure Active Directory SAML SSO via automated browser interactions. This Rust implementation provides significant performance improvements: <100ms startup vs 1-2s for Node.js, 10-15MB binary vs 200MB+ with node_modules, and 50-100MB runtime memory vs 150-300MB.

The tool maintains full backwards compatibility with the TypeScript version, using the same `~/.aws/config` and `~/.aws/credentials` formats.

## Build & Development Commands

- **Build**: `cargo build` — compile debug binary to target/debug/
- **Release build**: `cargo build --release` — optimized binary to target/release/ (opt-level=z, LTO enabled)
- **Run**: `cargo run -- [args]` — run with arguments (e.g., `cargo run -- --profile test`)
- **Test**: `cargo test` — run all tests
- **Debug run**: `RUST_LOG=debug cargo run -- --profile test --mode debug` — verbose logging with visible browser
- **Format**: `cargo fmt` — auto-format code with rustfmt
- **Lint**: `cargo clippy` — lint with clippy
- **Check**: `cargo check` — fast compilation check without building binary

## Architecture

The codebase follows a modular structure organized by concern:

### Entry Point & CLI
- **[main.rs](src/main.rs)** — Initializes tracing, parses CLI args, orchestrates main flow (configure vs login)
- **[cli.rs](src/cli.rs)** — Clap CLI definition with all flags (--profile, --mode, --all-profiles, --force-refresh, etc.)

### Configuration Management ([config/](src/config/))
- **[paths.rs](src/config/paths.rs)** — Centralized path handling for ~/.aws/config, ~/.aws/credentials, chromium data dir; respects AWS_CONFIG_FILE and AWS_SHARED_CREDENTIALS_FILE env vars
- **[profile.rs](src/config/profile.rs)** — Reads/writes AWS config INI files, manages azure_* profile settings (tenant_id, app_id_uri, default_username, default_role_arn, default_duration_hours, default_remember_me)
- **[credentials.rs](src/config/credentials.rs)** — Reads/writes AWS credentials INI files, handles credential expiration checks (11-minute refresh threshold), stores STS temporary credentials

### Azure AD Integration ([azure/](src/azure/))
- **[browser.rs](src/azure/browser.rs)** — Launches chromiumoxide browser with appropriate flags (headless/gui/debug modes, --no-sandbox, proxy support, user data directory)
- **[login.rs](src/azure/login.rs)** — Main login orchestration: generates SAML AuthnRequest, manages browser navigation, intercepts SAML response, handles role selection, stores credentials. Includes loginAll() for multi-profile support
- **[state_machine.rs](src/azure/state_machine.rs)** — Azure AD page state detection and handlers (username input, password input, account selection, MFA code, passwordless auth, remember me, error states)

### AWS Integration ([aws/](src/aws/))
- **[saml.rs](src/aws/saml.rs)** — SAML AuthnRequest generation (base64 + deflate encoding), SAML response parsing (quick-xml), extracts role ARNs and principal ARNs
- **[sts.rs](src/aws/sts.rs)** — AWS STS AssumeRoleWithSAML API calls, handles region-specific endpoints (standard, GovCloud, China), optional SSL verification flag

### Interactive Prompts ([prompts/](src/prompts/))
- **[configure.rs](src/prompts/configure.rs)** — Interactive profile configuration using dialoguer (tenant ID, app ID URI, default role, session duration, etc.)

### Error Handling
- **[error.rs](src/error.rs)** — Custom error types using thiserror (CliError, BrowserError, AwsError, ConfigError), distinguishes user-facing errors from internal errors

## Key Dependencies

- **chromiumoxide** 0.7 — Chrome DevTools Protocol-based browser automation (equivalent to Puppeteer)
- **tokio** 1.40 — async runtime with full features
- **clap** 4.5 — CLI parsing with derive macros
- **dialoguer** 0.11 — interactive prompts (equivalent to inquirer)
- **aws-sdk-sts** 1.48 — official AWS Rust SDK for STS operations
- **rust-ini** 0.21 — INI file parsing for AWS config/credentials
- **quick-xml** 0.36 / **scraper** 0.21 — SAML XML parsing
- **tracing** 0.1 — structured logging (controlled by RUST_LOG env var)
- **thiserror** / **anyhow** — error handling

## Important Implementation Details

### Credential Refresh Logic
Credentials are only refreshed if expiration time is within 11 minutes, preventing unnecessary authentication requests for both single profile and `--all-profiles` modes. Use `--force-refresh` to bypass the expiration check and re-authenticate immediately.

### Browser Automation Modes
- **cli** (default): Headless browser, fully automated
- **gui**: Visible browser window for manual interaction
- **debug**: Visible browser with automation (useful for troubleshooting state machine)

### SAML Flow
1. Generate SAML AuthnRequest with UUID and timestamp
2. Deflate + base64 encode → create Azure AD login URL
3. Navigate browser, automate login via state machine
4. Intercept network request containing SAMLResponse
5. Parse SAML XML to extract role ARNs
6. Call STS AssumeRoleWithSAML to get temporary credentials
7. Write to ~/.aws/credentials with expiration timestamp

### Multi-Profile Support
The `--all-profiles` flag reads all [profile *] sections from ~/.aws/config that contain azure_tenant_id, then logs into each using browser session reuse where appropriate. Browser sessions are grouped by both tenant_id AND username - profiles with the same tenant but different usernames will use separate browser sessions to avoid authentication conflicts.

### Signal Handling
The application implements graceful Ctrl-C handling using tokio::select! to intercept SIGINT signals. When interrupted, it gives browser processes 500ms to clean up before exiting with exit code 130 (standard for SIGINT). This prevents orphaned browser processes, especially on Windows.

### State Machine Enhancements
- **Password prompts** display the username when it was auto-filled from config (azure_default_username), helping users identify which account they're authenticating to
- **Screenshot capture** on unrecognized states saves debug screenshots to temp directory (unrecognized-state.png) for troubleshooting
- **Centralized screenshot helper** eliminates code duplication for debug screenshot capture

## Code Conventions

- Rust 2021 edition, async/await with tokio
- Use `tracing::info!`, `tracing::debug!`, `tracing::error!` for logging (not println!)
- Error types use thiserror for custom errors, anyhow for general error propagation
- Release profile uses opt-level="z" (optimize for size), LTO, strip symbols
- Module structure: public interfaces in mod.rs, implementation in separate files
- Prefer explicit error handling over unwrap/expect except in main.rs where it's acceptable

## Testing Strategy

- Integration tests in tests/ directory validate full login flows
- Unit tests within modules test individual components (SAML generation, INI parsing, etc.)
- Manual testing required for browser automation due to Azure AD dependencies
- Cross-platform testing needed (Linux, macOS, Windows) due to browser launch differences

## Compatibility with TypeScript Version

Maintains 100% config file compatibility:
- Same ~/.aws/config profile format with azure_* keys
- Same ~/.aws/credentials format with aws_expiration field
- Same CLI flags and behavior
- Can be used interchangeably with TypeScript version

## Known Limitations

- Chromium must be installed separately (not bundled like Puppeteer)
- Linux often requires --no-sandbox flag due to sandboxing restrictions
- State machine may need updates if Microsoft changes Azure AD UI
