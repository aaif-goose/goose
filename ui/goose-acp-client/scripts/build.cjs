#!/usr/bin/env node
// Every writer of this package's dist goes through here: `build` (generate + compile) and
// `build:ts` (compile only). Each takes the build lock and removes the desktop freshness stamp
// before writing anything. See build-guard.cjs.
//
// Usage: node scripts/build.cjs <generate|tsc>...
const { spawnSync } = require("child_process");
const { acquireLock, dropStamp, HELD_ENV } = require("./build-guard.cjs");

// Windows needs a shell to resolve .cmd shims; the commands and arguments here are fixed.
const steps = {
  generate: ["npm", ["run", "generate"]],
  tsc: ["tsc", []],
};
const requested = process.argv.slice(2);
if (!requested.length || requested.some((step) => !steps[step])) {
  console.error(`Usage: build.cjs <${Object.keys(steps).join("|")}>...`);
  process.exit(2);
}

acquireLock();
dropStamp();
// Anything this process starts that also goes through here already has the lock.
process.env[HELD_ENV] = "1";

for (const step of requested) {
  const [command, args] = steps[step];
  const result = spawnSync(command, args, {
    stdio: "inherit",
    shell: process.platform === "win32",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
