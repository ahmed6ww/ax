#!/usr/bin/env node
"use strict";

// axur is a Rust binary. npm resolves exactly one of the platform packages
// below through optionalDependencies — it filters them by the `os` and `cpu`
// fields, so a machine downloads only its own binary — and this shim hands
// control over to it.
//
// The alternative, a postinstall script that downloads from GitHub releases,
// is avoided deliberately: it breaks under `--ignore-scripts`, offline
// installs, and corporate proxies, and it puts an unverified download in the
// install path.

const { spawnSync } = require("node:child_process");

/** `${platform} ${arch}` as Node reports it, to the package that carries it. */
const PACKAGES = {
  "darwin arm64": "@axur/darwin-arm64",
  "darwin x64": "@axur/darwin-x64",
  "linux arm64": "@axur/linux-arm64",
  "linux x64": "@axur/linux-x64",
  "win32 x64": "@axur/win32-x64",
};

function fail(lines) {
  for (const line of lines) console.error(line);
  process.exit(1);
}

const key = `${process.platform} ${process.arch}`;
const pkg = PACKAGES[key];

if (!pkg) {
  fail([
    `axur: no prebuilt binary for ${key}.`,
    "",
    "Build it from source instead:",
    "  cargo install axur",
  ]);
}

const binary = process.platform === "win32" ? "axur.exe" : "axur";

let executable;
try {
  executable = require.resolve(`${pkg}/bin/${binary}`);
} catch {
  fail([
    `axur: ${pkg} is not installed.`,
    "",
    "The binary ships as an optional dependency, so installing with",
    "--no-optional or --omit=optional skips it. Reinstall with optional",
    "dependencies enabled, or add it directly:",
    "",
    `  npm install ${pkg}`,
  ]);
}

const result = spawnSync(executable, process.argv.slice(2), {
  stdio: "inherit",
});

if (result.error) {
  fail([`axur: could not run ${executable}`, result.error.message]);
}

// A process killed by a signal reports a null status. Exiting 0 there would
// tell a CI job that a killed sync succeeded.
process.exit(result.status === null ? 1 : result.status);
