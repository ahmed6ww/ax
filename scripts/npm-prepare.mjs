#!/usr/bin/env node
// Prepare the npm packages for publishing.
//
//   node scripts/npm-prepare.mjs <version> <artifacts-dir>
//
// Stamps <version> into every package.json (including the optionalDependencies
// range, which must match exactly or npm resolves an older platform package
// against a newer shim), copies each release binary into its platform package,
// and copies the README into the shim so the npm page is not blank.
//
// Run by the release workflow after the build matrix, and runnable by hand to
// inspect what would be published.

import { chmodSync, copyFileSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const [, , rawVersion, rawArtifacts] = process.argv;
if (!rawVersion || !rawArtifacts) {
  console.error("usage: node scripts/npm-prepare.mjs <version> <artifacts-dir>");
  process.exit(1);
}

// Accept either `1.2.3` or the `v1.2.3` git tag.
const version = rawVersion.replace(/^v/, "");
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error(`Not a semver version: ${version}`);
  process.exit(1);
}

const artifacts = resolve(rawArtifacts);

/** Platform package directory to the release asset it carries. */
const PLATFORMS = {
  "darwin-arm64": { asset: "axur-macos-arm64", binary: "axur" },
  "darwin-x64": { asset: "axur-macos-x64", binary: "axur" },
  "linux-arm64": { asset: "axur-linux-arm64", binary: "axur" },
  "linux-x64": { asset: "axur-linux-x64", binary: "axur" },
  "win32-x64": { asset: "axur-windows-x64.exe", binary: "axur.exe" },
};

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

// ---- platform packages ----------------------------------------------------
for (const [dir, { asset, binary }] of Object.entries(PLATFORMS)) {
  const pkgDir = join(root, "npm", "platforms", dir);
  const source = join(artifacts, asset);

  if (!existsSync(source)) {
    console.error(`Missing release artifact: ${source}`);
    console.error("Every platform must publish, or the shim resolves to nothing on that OS.");
    process.exit(1);
  }

  const destination = join(pkgDir, "bin", binary);
  copyFileSync(source, destination);
  // The artifact loses its executable bit through upload-artifact.
  if (binary !== "axur.exe") chmodSync(destination, 0o755);

  const manifestPath = join(pkgDir, "package.json");
  const manifest = readJson(manifestPath);
  manifest.version = version;
  writeJson(manifestPath, manifest);

  console.log(`@axur/${dir}  <-  ${asset}`);
}

// ---- the shim -------------------------------------------------------------
const shimPath = join(root, "npm", "axur", "package.json");
const shim = readJson(shimPath);
shim.version = version;
for (const name of Object.keys(shim.optionalDependencies)) {
  // An exact pin, not a range: the shim and its binary are one artifact.
  shim.optionalDependencies[name] = version;
}
writeJson(shimPath, shim);

copyFileSync(join(root, "README.md"), join(root, "npm", "axur", "README.md"));

console.log(`axur  ${version}`);
