#!/bin/bash
# AX Release Script
# Usage: ./scripts/release.sh <version>
# Example: ./scripts/release.sh 1.5.0

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if version is provided
if [ -z "$1" ]; then
  echo -e "${RED}Error: Version number required${NC}"
  echo "Usage: $0 <version>"
  echo "Example: $0 1.5.0"
  exit 1
fi

VERSION="$1"
TAG="v$VERSION"

echo -e "${CYAN}"
echo "╔═══════════════════════════════════════╗"
echo "║     AX Release Automation             ║"
echo "╚═══════════════════════════════════════╝"
echo -e "${NC}"

# Verify we're on main branch
CURRENT_BRANCH=$(git branch --show-current)
if [ "$CURRENT_BRANCH" != "main" ]; then
  echo -e "${YELLOW}Warning: You're on branch '$CURRENT_BRANCH', not 'main'${NC}"
  read -p "Continue anyway? (y/N) " -n 1 -r
  echo
  if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    exit 1
  fi
fi

# Check for uncommitted changes
if ! git diff-index --quiet HEAD --; then
  echo -e "${RED}Error: You have uncommitted changes${NC}"
  echo "Please commit or stash your changes first"
  exit 1
fi

echo -e "${CYAN}→ Updating version to $VERSION${NC}"

# Update Cargo.toml
sed -i "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml

# Update Cargo.lock
cargo check --quiet

echo -e "${GREEN}✓ Version updated in Cargo.toml${NC}"

# Commit version change
echo -e "${CYAN}→ Committing version change${NC}"
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to $TAG"

echo -e "${GREEN}✓ Version change committed${NC}"

# Push to main
echo -e "${CYAN}→ Pushing to main${NC}"
git push origin "$CURRENT_BRANCH"

echo -e "${GREEN}✓ Pushed to main${NC}"

# Create and push tag
echo -e "${CYAN}→ Creating tag $TAG${NC}"
git tag "$TAG"

echo -e "${CYAN}→ Pushing tag $TAG${NC}"
git push origin "$TAG"

echo -e "${GREEN}✓ Tag pushed${NC}"

echo ""
echo -e "${GREEN}╔═══════════════════════════════════════╗${NC}"
echo -e "${GREEN}║   Release $TAG initiated!              ${NC}"
echo -e "${GREEN}╚═══════════════════════════════════════╝${NC}"
echo ""
echo "Next steps:"
echo "  1. Monitor GitHub Actions: https://github.com/ahmed6ww/ax/actions"
echo "  2. Once complete, verify release: https://github.com/ahmed6ww/ax/releases"
echo "  3. Test install script:"
echo "     curl -fsSL https://raw.githubusercontent.com/ahmed6ww/ax/main/install.sh | bash"
echo ""
