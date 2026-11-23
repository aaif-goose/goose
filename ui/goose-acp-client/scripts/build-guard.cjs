/**
 * Shared by `pnpm run build` here and by ui/desktop/scripts/build-goose-acp-client.js, which keeps a
 * freshness stamp for this package's dist. Two things keep that stamp honest:
 *
 *  - one builder at a time (an on-disk lock), so a stamp can never be written while another build is
 *    still writing dist;
 *  - every build removes the stamp before it writes anything, so a build that bypasses the desktop
 *    script (`build`, `prepack`) cannot leave an old stamp vouching for new output.
 */
const fs = require("fs");
const path = require("path");

const cacheDir = path.join(__dirname, "..", "node_modules", ".cache");
const stampPath = path.join(cacheDir, "desktop-build-stamp.json");
const lockDir = path.join(cacheDir, "build.lock");
const staleMutex = path.join(cacheDir, "build.lock.stale");
const ownerFile = path.join(lockDir, "owner");
// Set by the desktop script, which already holds the lock when it runs `build` as a child.
const HELD_ENV = "GOOSE_ACP_CLIENT_BUILD_LOCK_HELD";

const WAIT_MS = 10 * 60 * 1000;
const POLL_MS = 200;

function sleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function ownerAlive() {
  let pid = Number.NaN;
  try {
    pid = Number.parseInt(fs.readFileSync(ownerFile, "utf8"), 10);
  } catch {
    // Missing: the owner has made the directory but not yet written its pid.
  }
  if (!Number.isInteger(pid)) {
    // Also covers a pid file caught half-written (empty). Only call the lock abandoned once old.
    try {
      return Date.now() - fs.statSync(lockDir).mtimeMs < 30_000;
    } catch {
      return false;
    }
  }
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code === "EPERM";
  }
}

// Remove a lock whose owner looks dead. Several waiters can see the same dead owner at once, and
// removing the lock in place would let a slow one delete the lock a faster one has just taken. So
// stale removal is itself serialized: while the stale lock is still in place nobody else can mkdir
// it, and only the holder of this mutex touches it, so the owner re-checked under the mutex is the
// owner that gets removed.
function removeStale() {
  try {
    fs.mkdirSync(staleMutex);
  } catch (error) {
    if (error.code !== "EEXIST") throw error;
    // A remover that died mid-removal leaves the mutex behind; the work under it takes microseconds.
    try {
      if (Date.now() - fs.statSync(staleMutex).mtimeMs > 10_000) {
        fs.rmSync(staleMutex, { recursive: true, force: true });
      }
    } catch {
      // Gone already.
    }
    return;
  }
  try {
    // The lock must still exist: if it was already removed and retaken, "no owner" would read as dead.
    if (fs.existsSync(lockDir) && !ownerAlive()) {
      fs.rmSync(lockDir, { recursive: true, force: true });
    }
  } finally {
    fs.rmSync(staleMutex, { recursive: true, force: true });
  }
}

function release() {
  fs.rmSync(lockDir, { recursive: true, force: true });
}

/** Take the build lock and release it when this process exits. No-op if the parent holds it. */
function acquireLock() {
  if (process.env[HELD_ENV] === "1") return;
  fs.mkdirSync(cacheDir, { recursive: true });
  const deadline = Date.now() + WAIT_MS;
  for (let announced = false; ;) {
    try {
      fs.mkdirSync(lockDir);
      break;
    } catch (error) {
      if (error.code !== "EEXIST") throw error;
    }
    if (!ownerAlive()) {
      removeStale();
      continue;
    }
    if (Date.now() > deadline) {
      throw new Error(
        `Timed out waiting for another goose ACP client build (lock: ${lockDir})`,
      );
    }
    if (!announced) {
      console.log("Waiting for another goose ACP client build to finish ...");
      announced = true;
    }
    sleep(POLL_MS);
  }
  fs.writeFileSync(ownerFile, String(process.pid));
  process.on("exit", release);
  for (const signal of ["SIGINT", "SIGTERM"])
    process.on(signal, () => process.exit(1));
}

function dropStamp() {
  fs.rmSync(stampPath, { force: true });
}

module.exports = { acquireLock, dropStamp, stampPath, HELD_ENV };
