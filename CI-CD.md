# CI/CD Pipeline Documentation

This document describes the GitLab CI/CD pipeline for building and releasing `aws-azure-saml`.

## Pipeline Overview

The pipeline has three stages:

1. **Test** - Runs on all commits to `main` and on tags
2. **Build** - Compiles release binaries for multiple platforms (tags only)
3. **Release** - Creates a GitLab release with all artifacts (tags only)

## Supported Platforms

The pipeline builds binaries for:

| Platform | Architecture | Target Triple | Artifact Path |
|----------|-------------|---------------|---------------|
| Linux | x86_64 | `x86_64-unknown-linux-gnu` | `artifacts/linux-x86_64/aws-azure-saml` |
| Linux | ARM64 | `aarch64-unknown-linux-gnu` | `artifacts/linux-arm64/aws-azure-saml` |
| Windows | x86_64 | `x86_64-pc-windows-gnu` | `artifacts/windows-x86_64/aws-azure-saml.exe` |
| macOS | Intel x86_64 | `x86_64-apple-darwin` | `artifacts/macos-x86_64/aws-azure-saml` |
| macOS | Apple Silicon (ARM64) | `aarch64-apple-darwin` | `artifacts/macos-arm64/aws-azure-saml` |

### Artifact Structure

Artifacts are organized in platform-specific folders:
```
artifacts/
├── linux-x86_64/
│   └── aws-azure-saml
├── linux-arm64/
│   └── aws-azure-saml
├── windows-x86_64/
│   └── aws-azure-saml.exe
├── macos-x86_64/
│   └── aws-azure-saml
└── macos-arm64/
    └── aws-azure-saml
```

This structure keeps the binary name consistent across platforms while organizing by platform type.

## Creating a Release

### 1. Update Version

Update the version in [`Cargo.toml`](Cargo.toml):

```toml
[package]
version = "0.2.0"  # Update this
```

### 2. Commit and Tag

```bash
# Commit the version change
git add Cargo.toml
git commit -m "Bump version to 0.2.0"

# Create and push tag
git tag v0.2.0
git push origin main
git push origin v0.2.0
```

### 3. Pipeline Execution

The pipeline will automatically:
1. Run all tests
2. Build binaries for all platforms
3. Create a GitLab release with download links

## Pipeline Stages

### Test Stage

Runs on every commit to `main` and on all tags.

**Jobs:**
- `test` - Runs unit tests, integration tests, Clippy linting, and format checking

**Requirements:**
- Rust 1.75 or later
- All tests must pass
- Code must pass `cargo clippy` with no warnings
- Code must be properly formatted (`cargo fmt`)

### Build Stage

Runs only on tagged commits.

**Jobs:**
- `build:linux-x86_64` - Native build on Linux x86_64
- `build:linux-arm64` - Cross-compile for Linux ARM64
- `build:windows-x86_64` - Cross-compile for Windows using MinGW
- `build:macos-x86_64` - Build on macOS runner (Intel)
- `build:macos-arm64` - Build on macOS runner (Apple Silicon)

**Artifacts:**
- Each job produces a single executable binary
- Artifacts are stored for 1 week
- Binaries are stripped of debug symbols for smaller size

### Release Stage

Runs only on tagged commits, after all build jobs complete.

**Output:**
- Creates a GitLab Release associated with the tag
- Includes download links for all platform binaries
- Contains release notes with installation instructions

## Runner Requirements

### Linux Builds (x86_64 and ARM64)

Uses GitLab's shared Linux runners. No special configuration needed.

### Windows Build

Uses cross-compilation from Linux with MinGW-w64. No Windows runner required.

### macOS Builds

**Requires GitLab runners with macOS.**

If you don't have macOS runners:

1. **Option 1: Use GitLab.com SaaS macOS runners**
   - Available on GitLab.com Premium/Ultimate tiers
   - Tag: `macos`

2. **Option 2: Self-hosted macOS runners**
   - Install GitLab Runner on a macOS machine
   - Register with tags: `macos` for Intel, `macos,arm64` for Apple Silicon

3. **Option 3: Disable macOS builds**
   - Comment out the `build:macos-x86_64` and `build:macos-arm64` jobs in `.gitlab-ci.yml`
   - The jobs are set to `allow_failure: true` so the pipeline won't fail if macOS runners are unavailable

## Troubleshooting

### macOS Builds Fail

If macOS runners are not available, the jobs will fail but won't block the release. To disable:

```yaml
# In .gitlab-ci.yml, comment out these jobs:
# build:macos-x86_64:
#   ...
# build:macos-arm64:
#   ...
```

### Cross-Compilation Issues

If cross-compilation fails for ARM64 or Windows:

1. Check that the target is installed: `rustup target list`
2. Ensure cross-compilation dependencies are installed in the Docker image
3. Review the job logs for specific errors

### Cache Issues

If builds are slow or failing due to cache corruption:

1. Go to **CI/CD > Pipelines**
2. Click **Clear runner caches**
3. Re-run the pipeline

## Local Testing

Test the build locally before pushing:

```bash
# Run tests
cargo test

# Build for your current platform
cargo build --release

# Cross-compile for Linux ARM64 (requires cross-compilation setup)
cargo build --release --target aarch64-unknown-linux-gnu

# Cross-compile for Windows (requires MinGW)
cargo build --release --target x86_64-pc-windows-gnu
```

## Environment Variables

The pipeline uses these variables:

- `CARGO_HOME`: Cache directory for Cargo dependencies
- `RUST_BACKTRACE`: Enabled for better error messages
- `CI_COMMIT_TAG`: The git tag triggering the pipeline
- `CI_PROJECT_URL`: Used to construct artifact download URLs

## Security Considerations

### Artifact Signing

Currently, binaries are **not signed**. Consider adding:

1. **GPG signing** of release artifacts
2. **Code signing** for macOS/Windows binaries
3. **Checksum files** (SHA256) for download verification

Example checksum generation:

```yaml
script:
  - sha256sum artifacts/* > checksums.txt
artifacts:
  paths:
    - artifacts/
    - checksums.txt
```

### Supply Chain Security

The pipeline:
- ✅ Uses official Rust Docker images
- ✅ Pins Rust version (1.75)
- ✅ Runs tests before building
- ✅ Checks code with Clippy
- ⚠️  Does not verify dependencies (consider `cargo-audit`)

To add dependency auditing:

```yaml
before_script:
  - cargo install cargo-audit
  - cargo audit
```

## Future Enhancements

1. **Add cargo-audit** for dependency vulnerability scanning
2. **Generate checksums** for all artifacts
3. **Sign releases** with GPG
4. **Add benchmarks** to track performance across releases
5. **Create Docker images** in addition to binaries
6. **Publish to crates.io** automatically on release
7. **Add Windows code signing** for enterprise use
8. **Create installers** (MSI for Windows, PKG for macOS, DEB/RPM for Linux)
