#!/usr/bin/env node
// Single source of truth for the RepoSync version bump.
//
// RepoSync carries its version in four files that nothing otherwise keeps in
// sync. This script is the only sanctioned way to change the version: it rewrites
// all four atomically so a release can never ship a bundle whose installer version
// disagrees with the crate version.
//
//   1. Cargo.toml            [workspace.package] version   (inherited by reposync-core)
//   2. src-tauri/Cargo.toml  [package] version             (the desktop binary crate)
//   3. package.json          top-level "version"           (frontend package)
//   4. src-tauri/tauri.conf.json  "version"                (stamps the MSI/NSIS/.app/.dmg)
//
// Usage:  node scripts/bump-version.mjs <semver>     e.g. node scripts/bump-version.mjs 0.9.0

import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const newVersion = process.argv[2];
if (!newVersion || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(newVersion)) {
  console.error('Usage: node scripts/bump-version.mjs <semver>   (e.g. 0.9.0 or 0.9.0-beta.1)');
  process.exit(1);
}

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

// Replace the first `version = "..."` line that sits inside the named [section].
// Section-aware so we never touch the version pins in [workspace.dependencies].
function bumpTomlSection(relPath, section) {
  const path = join(root, relPath);
  const lines = readFileSync(path, 'utf8').split(/\r?\n/);
  let inSection = false;
  let done = false;
  for (let i = 0; i < lines.length; i++) {
    const header = lines[i].match(/^\s*\[([^\]]+)\]\s*$/);
    if (header) { inSection = header[1] === section; continue; }
    if (inSection && !done && /^\s*version\s*=\s*"/.test(lines[i])) {
      lines[i] = lines[i].replace(/version\s*=\s*"[^"]*"/, `version = "${newVersion}"`);
      done = true;
    }
  }
  if (!done) throw new Error(`No version line found in [${section}] of ${relPath}`);
  writeFileSync(path, lines.join('\n'));
  console.log(`  ${relPath} [${section}] -> ${newVersion}`);
}

// Replace the first top-level "version": "..." in a JSON file (preserves formatting).
function bumpJsonVersion(relPath) {
  const path = join(root, relPath);
  const text = readFileSync(path, 'utf8');
  const re = /("version"\s*:\s*)"[^"]*"/;
  if (!re.test(text)) throw new Error(`No "version" key found in ${relPath}`);
  writeFileSync(path, text.replace(re, `$1"${newVersion}"`));
  console.log(`  ${relPath} version -> ${newVersion}`);
}

console.log(`Bumping RepoSync to ${newVersion}:`);
bumpTomlSection('Cargo.toml', 'workspace.package');
bumpTomlSection('src-tauri/Cargo.toml', 'package');
bumpJsonVersion('package.json');
bumpJsonVersion('src-tauri/tauri.conf.json');
console.log('Done. Review: git diff -- Cargo.toml src-tauri/Cargo.toml package.json src-tauri/tauri.conf.json');
console.log('Then run `cargo check` and `pnpm install` so the lockfiles pick up the new version.');
