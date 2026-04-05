# Release Process

Quick guide for creating a new release of `aws-azure-saml`.

## Prerequisites

- [ ] All tests passing locally
- [ ] CHANGELOG.md updated with new features/fixes
- [ ] No outstanding critical bugs
- [ ] Code reviewed and merged to `main`

## Steps

### 1. Update Version Number

Edit `Cargo.toml`:

```toml
[package]
version = "0.2.0"  # Change this
```

### 2. Update CHANGELOG

Add a new section in `CHANGELOG.md` (create if doesn't exist):

```markdown
## [0.2.0] - 2026-04-04

### Added
- New feature X
- Support for Y

### Fixed
- Bug Z
- Issue with W

### Changed
- Improved performance of A
```

### 3. Run Tests Locally

```bash
cargo test
cargo clippy -- -D warnings
cargo fmt -- --check
```

### 4. Commit Version Bump

```bash
git add Cargo.toml CHANGELOG.md
git commit -m "Release v0.2.0"
git push origin main
```

### 5. Create and Push Tag

```bash
# Create annotated tag
git tag -a v0.2.0 -m "Release version 0.2.0"

# Push tag
git push origin v0.2.0
```

### 6. Monitor Pipeline

1. Go to GitLab: **CI/CD > Pipelines**
2. Watch the pipeline for tag `v0.2.0`
3. Stages should execute in order:
   - ✅ Test (must pass)
   - ✅ Build (all platforms)
   - ✅ Release (creates GitLab release)

### 7. Verify Release

1. Go to **Deployments > Releases**
2. Verify release `v0.2.0` exists
3. Check that all binaries are downloadable:
   - Linux x86_64
   - Linux ARM64
   - Windows x86_64
   - macOS Intel
   - macOS Apple Silicon

### 8. Test Downloads

Download and test at least one binary:

```bash
# Linux example
curl -LO "https://gitlab.com/your-org/aws-azure-saml/-/releases/v0.2.0/downloads/aws-azure-saml-linux-x86_64"
chmod +x aws-azure-saml-linux-x86_64
./aws-azure-saml-linux-x86_64 --version
```

### 9. Announce Release

- [ ] Update project README if needed
- [ ] Post announcement (if applicable)
- [ ] Update documentation site (if applicable)

## Rollback

If the release has critical issues:

### Delete Tag and Release

```bash
# Delete local tag
git tag -d v0.2.0

# Delete remote tag
git push origin :refs/tags/v0.2.0

# Delete GitLab release manually in UI
```

### Fix and Re-release

```bash
# Fix the issue
git commit -m "Fix critical issue"

# Create new patch version
# Update Cargo.toml to 0.2.1
git tag -a v0.2.1 -m "Release version 0.2.1 (fixes critical issue in 0.2.0)"
git push origin main v0.2.1
```

## Version Numbering

Follow [Semantic Versioning](https://semver.org/):

- **Major** (1.0.0): Breaking changes
- **Minor** (0.1.0): New features, backward compatible
- **Patch** (0.0.1): Bug fixes, backward compatible

## Troubleshooting

### Pipeline Fails on Test Stage

```bash
# Run tests locally to debug
RUST_LOG=debug cargo test -- --nocapture

# Check specific test
cargo test test_name -- --nocapture
```

### macOS Build Fails

macOS builds require GitLab runners with macOS. If not available:

1. Builds will fail but pipeline continues (`allow_failure: true`)
2. Manually build on macOS and upload:

```bash
# On macOS
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin

# Upload manually to release
```

### Windows Build Fails

Check MinGW cross-compilation setup:

```bash
# Test locally (on Linux)
sudo apt-get install gcc-mingw-w64-x86-64
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

## Post-Release Checklist

- [ ] All platform binaries tested
- [ ] Release notes accurate
- [ ] Download links working
- [ ] Version updated in README
- [ ] CHANGELOG.md updated
- [ ] No critical issues reported

## Emergency Hotfix

For critical security/bug fixes:

```bash
# Create hotfix from main
git checkout main
git checkout -b hotfix/v0.2.1

# Make fix
git commit -m "Fix critical security issue"

# Update version to 0.2.1
git commit -m "Bump version to 0.2.1"

# Merge and tag
git checkout main
git merge hotfix/v0.2.1
git tag -a v0.2.1 -m "Hotfix: critical security issue"
git push origin main v0.2.1
```
