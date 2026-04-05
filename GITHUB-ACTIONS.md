# GitHub Actions CI/CD Pipeline

This document describes the GitHub Actions workflow for building and releasing `aws-azure-saml`.

## Workflow Overview

The workflow (`.github/workflows/release.yml`) has three main phases:

1. **Test** - Runs on all commits to `main`, PRs, and tags
2. **Build** - Compiles release binaries for multiple platforms (tags only)
3. **Release** - Creates a GitHub Release with ZIP archives (tags only)

## Supported Platforms

| Platform | Architecture | Runner | Artifact |
|----------|-------------|--------|----------|
| Linux | x86_64 | `ubuntu-latest` | `aws-azure-saml-linux-x86_64.zip` |
| Linux | ARM64 | `ubuntu-latest` (cross-compile) | `aws-azure-saml-linux-arm64.zip` |
| Windows | x86_64 | `ubuntu-latest` (MinGW) | `aws-azure-saml-windows-x86_64.zip` |
| macOS | Intel (x86_64) | `macos-13` | `aws-azure-saml-macos-x86_64.zip` |
| macOS | Apple Silicon (ARM64) | `macos-14` | `aws-azure-saml-macos-arm64.zip` |

## Creating a Release

### 1. Update Version

Edit `Cargo.toml`:
```toml
[package]
version = "0.2.0"  # Update this
```

### 2. Commit and Tag

```bash
# Commit version bump
git add Cargo.toml
git commit -m "Release v0.2.0"
git push origin main

# Create and push tag
git tag v0.2.0
git push origin v0.2.0
```

### 3. Workflow Execution

The workflow will automatically:
1. ✅ Run tests
2. ✅ Build binaries for all 5 platforms in parallel
3. ✅ Create ZIP archives for each platform
4. ✅ Generate SHA256 checksums
5. ✅ Create GitHub Release with all files attached

### 4. Release Output

The release will include:
- `aws-azure-saml-linux-x86_64.zip`
- `aws-azure-saml-linux-arm64.zip`
- `aws-azure-saml-windows-x86_64.zip`
- `aws-azure-saml-macos-x86_64.zip`
- `aws-azure-saml-macos-arm64.zip`
- `SHA256SUMS.txt` - checksums for verification

## Workflow Triggers

### Automatic Triggers

- **Push to `main`**: Runs tests only
- **Pull Request**: Runs tests only
- **Tag push (`v*`)**: Runs tests, builds all platforms, creates release

### Manual Trigger

You can also manually trigger the workflow from the Actions tab in GitHub.

## Build Details

### Test Job

Runs on every commit and PR:
```bash
cargo test --verbose
cargo clippy -- -D warnings
cargo fmt -- --check
```

### Build Jobs

Each platform builds in parallel:

**Linux x86_64:**
- Native compilation on Ubuntu
- Stripped binary for smaller size
- Packaged in platform folder

**Linux ARM64:**
- Cross-compilation using `gcc-aarch64-linux-gnu`
- Requires `CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER`

**Windows x86_64:**
- Cross-compilation using MinGW (`gcc-mingw-w64-x86-64`)
- No Windows runner needed!

**macOS Intel:**
- Uses `macos-13` runner (Intel hardware)
- Native compilation

**macOS Apple Silicon:**
- Uses `macos-14` runner (ARM64 hardware)
- Native compilation

### Artifact Structure

Each artifact maintains the folder structure:
```
linux-x86_64/
  aws-azure-saml

linux-arm64/
  aws-azure-saml

windows-x86_64/
  aws-azure-saml.exe

macos-x86_64/
  aws-azure-saml

macos-arm64/
  aws-azure-saml
```

## Caching

Cargo dependencies are cached per platform using `actions/cache@v4`:
- Cache key includes OS and `Cargo.lock` hash
- Significantly speeds up subsequent builds
- Cache is restored across workflow runs

## Permissions

The release job requires `contents: write` permission to:
- Create releases
- Upload release assets
- Generate release notes

This is granted automatically via `GITHUB_TOKEN`.

## Troubleshooting

### Workflow Not Triggering

**Check:**
1. Is the tag named correctly? Must start with `v` (e.g., `v0.2.0`)
2. Is the workflow file in `.github/workflows/` directory?
3. Are there any YAML syntax errors? Use a YAML validator

**Verify:**
```bash
# Check if tag was pushed
git ls-remote --tags origin

# Validate YAML syntax
yamllint .github/workflows/release.yml
```

### Build Failures

**Linux/Windows builds fail:**
- Check if cross-compilation tools are installing correctly
- Review the job logs for specific errors

**macOS builds fail:**
- `macos-13` runners might be unavailable (rare)
- `macos-14` requires GitHub Teams or Enterprise
- Consider commenting out macOS jobs if not needed

**Tests fail:**
- Fix tests before tagging
- Run tests locally: `cargo test`
- Check clippy: `cargo clippy -- -D warnings`

### Release Not Created

**Check permissions:**
- Ensure `permissions: contents: write` is set in release job
- Check if repository settings allow Actions to create releases

**Check job dependencies:**
- All build jobs must succeed before release runs
- Use `needs:` to verify dependency chain

### Artifacts Missing

**Verify uploads:**
```yaml
- uses: actions/upload-artifact@v4
  with:
    name: linux-x86_64  # Must be unique
    path: artifacts/linux-x86_64/  # Must exist
```

**Check downloads:**
```yaml
- uses: actions/download-artifact@v4
  with:
    path: artifacts/  # Downloads all artifacts here
```

## Local Testing

Test builds locally before pushing:

```bash
# Run tests
cargo test
cargo clippy -- -D warnings
cargo fmt -- --check

# Test builds for your platform
cargo build --release

# Test cross-compilation (Linux)
cargo build --release --target x86_64-pc-windows-gnu
cargo build --release --target aarch64-unknown-linux-gnu
```

## Cost Considerations

GitHub Actions provides free minutes for public repositories:
- **Linux runners**: 2,000 minutes/month (free)
- **macOS runners**: 2,000 minutes/month (free, but 10x multiplier)
- **Windows runners**: 2,000 minutes/month (free, but 2x multiplier)

For this workflow on a tag push:
- Linux builds: ~10 minutes total
- Windows build: ~8 minutes (16 effective minutes)
- macOS builds: ~12 minutes total (120 effective minutes)
- **Total per release**: ~140 effective minutes

## Comparison: GitHub Actions vs GitLab CI/CD

| Feature | GitHub Actions | GitLab CI/CD |
|---------|---------------|--------------|
| **macOS Runners** | ✅ Free (public repos) | ⚠️ Paid only |
| **Windows Cross-compile** | ✅ Easy (MinGW) | ✅ Easy (MinGW) |
| **Artifact Storage** | 500 MB (free) | 10 GB (free) |
| **Cache** | 10 GB | Unlimited |
| **Release Creation** | ✅ Built-in | ✅ Built-in |
| **Configuration** | YAML in `.github/workflows/` | YAML in root `.gitlab-ci.yml` |

## Advanced Features

### Matrix Builds

You could simplify the workflow using matrix strategy:

```yaml
strategy:
  matrix:
    include:
      - os: ubuntu-latest
        target: x86_64-unknown-linux-gnu
        platform: linux-x86_64
      - os: macos-13
        target: x86_64-apple-darwin
        platform: macos-x86_64
```

### Conditional Steps

Skip certain platforms:

```yaml
if: matrix.os != 'macos-14'  # Skip Apple Silicon
```

### Secrets

For code signing (future enhancement):

```yaml
- name: Sign binary
  env:
    MACOS_CERTIFICATE: ${{ secrets.MACOS_CERTIFICATE }}
    MACOS_PASSWORD: ${{ secrets.MACOS_PASSWORD }}
```

## Future Enhancements

1. **Code Signing**
   - Sign macOS binaries with Apple Developer certificate
   - Sign Windows binaries with Authenticode

2. **Checksums in Release Notes**
   - Include SHA256 sums directly in release description

3. **Universal macOS Binary**
   - Combine x86_64 and ARM64 into single universal binary
   - `lipo -create -output aws-azure-saml x86_64 arm64`

4. **Docker Images**
   - Build and push Docker images to GitHub Container Registry

5. **Homebrew Formula**
   - Auto-update Homebrew tap on release

6. **Cargo Publish**
   - Automatically publish to crates.io on release
