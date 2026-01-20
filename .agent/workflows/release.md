---
description: How to release a new version of AX
---

# Release Workflow for AX

This workflow guides you through releasing a new version of AX to GitHub and making it available via the install script.

## Prerequisites

- Ensure all changes are committed and pushed to `main`
- You have push access to the repository
- All tests pass locally

## Steps

### 1. Update version in Cargo.toml

Edit `Cargo.toml` and update the version number:
```toml
[package]
version = "1.5.0"  # Update this
```

### 2. Commit the version change

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to v1.5.0"
git push origin main
```

### 3. Create and push the git tag

// turbo
```bash
git tag v1.5.0
```

// turbo
```bash
git push origin v1.5.0
```

### 4. Monitor the GitHub Actions workflow

The release workflow will automatically:
- Build binaries for all platforms (Linux x64/ARM64, macOS x64/ARM64, Windows x64)
- Create a GitHub release
- Upload all binaries to the release

Check the progress at: https://github.com/ahmed6ww/ax/actions

### 5. Verify the release

Once the workflow completes:
- Visit https://github.com/ahmed6ww/ax/releases
- Verify v1.5.0 is published with all binaries
- Test the install script:

```bash
curl -fsSL https://raw.githubusercontent.com/ahmed6ww/ax/main/install.sh | bash
```

## Quick Release Script

Alternatively, use the automated release script:

```bash
./scripts/release.sh 1.5.0
```

This will handle steps 1-3 automatically.

## Troubleshooting

### Release workflow fails
- Check GitHub Actions logs
- Ensure all targets can be built
- Verify GITHUB_TOKEN permissions

### Install script downloads old version
- Ensure the tag was pushed: `git ls-remote --tags origin`
- Check GitHub API: `curl -s https://api.github.com/repos/ahmed6ww/ax/releases/latest`
- Clear any CDN cache if using one

### Binary not found
- Verify artifact names match those in install.sh
- Check the release assets on GitHub
