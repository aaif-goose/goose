#!/usr/bin/env node
// `pnpm run build` for this package: generate, then compile, under the build lock and with the
// desktop freshness stamp removed first. See build-guard.cjs.
const { spawnSync } = require("child_process");
const { acquireLock, dropStamp } = require("./build-guard.cjs");

acquireLock();
dropStamp();

for (const script of ["generate", "build:ts"]) {
  // Windows needs a shell to resolve npm.cmd; the command and arguments are fixed.
  const result = spawnSync("npm", ["run", script], {
    stdio: "inherit",
    shell: process.platform === "win32",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
