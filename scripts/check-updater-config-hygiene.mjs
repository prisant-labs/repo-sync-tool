// scripts/check-updater-config-hygiene.mjs
//
// E-18 (auto-update and distribution) production config-hygiene gate.
//
// Asserts the committed production src-tauri/tauri.conf.json contains NONE of the
// test-only updater markers - they belong only in the E2E overlay
// (src-tauri/tauri.updater-e2e.conf.json), which is inert unless explicitly passed
// via --config and never merged into the production config:
//
//   * dangerousInsecureTransportProtocol - the plain-http transport opt-in. In
//     production, Tauri v2 enforces TLS on updater endpoints; this flag must never
//     ship. Without it a stray http updater endpoint cannot serve updates anyway.
//   * the disposable test pubkey sentinel - a throwaway key that must never be
//     trusted by a shipped build.
//
// This is the deterministic pre-tag release gate documented in the runbook (G3). It
// mirrors the in-suite Rust test (src-tauri/src/updates.rs
// production_tauri_conf_has_no_test_only_updater_markers); run either. A `http://localhost`
// endpoint is deliberately NOT a marker: build.devUrl is legitimately localhost, and
// dangerousInsecureTransportProtocol already gates insecure updater transport.
//
// Usage: node scripts/check-updater-config-hygiene.mjs
// Exit 0 = clean; exit 1 = a forbidden marker is present (blocks the tag).

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const configPath = join(here, "..", "src-tauri", "tauri.conf.json");

const FORBIDDEN_PRODUCTION_UPDATER_MARKERS = [
  "dangerousInsecureTransportProtocol",
  "DISPOSABLE_TEST_UPDATER_PUBKEY_E2E_ONLY",
];

const text = readFileSync(configPath, "utf8");
const found = FORBIDDEN_PRODUCTION_UPDATER_MARKERS.filter((marker) => text.includes(marker));

if (found.length > 0) {
  console.error(
    `FAIL: production src-tauri/tauri.conf.json contains test-only updater marker(s): ${found.join(", ")}.\n` +
      "These belong only in src-tauri/tauri.updater-e2e.conf.json (the test overlay). Remove them before tagging.",
  );
  process.exit(1);
}

console.log("OK: production tauri.conf.json contains no test-only updater markers.");
