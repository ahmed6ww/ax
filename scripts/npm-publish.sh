#!/usr/bin/env bash
# Publish one npm package directory, skipping it if that exact version is
# already on the registry.
#
#   DIST=latest ./scripts/npm-publish.sh ./npm/axur
#
# Two things this exists to prevent, both of which bit the 0.1.0 release:
#
#   1. `npm publish npm/axur` is not a path. npm reads `owner/repo` as a
#      GitHub shorthand and tries to clone github.com/npm/axur. Only a path
#      starting with ./ or ending in / is unambiguous, so this script
#      normalises whatever it is given.
#
#   2. A release that fails partway leaves some packages published. Re-running
#      then dies on the first one with EPUBLISHCONFLICT, so the packages that
#      never made it cannot be retried. Already-published versions are skipped
#      instead, which makes a rerun the obvious fix rather than a dead end.

set -euo pipefail

dir="${1:?usage: npm-publish.sh <package-dir>}"
dist="${DIST:-latest}"

# Force an unambiguous path so npm cannot read it as a package spec.
case "$dir" in
  /* | ./* | ../*) ;;
  *) dir="./$dir" ;;
esac

manifest="${dir%/}/package.json"
[ -f "$manifest" ] || { echo "No package.json in $dir" >&2; exit 1; }

name=$(node -p "require('$manifest').name")
version=$(node -p "require('$manifest').version")

if npm view "$name@$version" version >/dev/null 2>&1; then
  echo "= $name@$version already published, skipping"
  exit 0
fi

echo "+ $name@$version"
npm publish "$dir" --access public --provenance --tag "$dist"
