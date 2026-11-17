import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// Every Tauri config file in src-tauri/ must pass the installed CLI's own schema.
//
// Why this exists: the release overlay (tauri.updater-prod.conf.json) once carried a
// `"//"` comment key. The release build merges that overlay only when the updater
// signing secret is present, so per-push CI never read it, and the first release dry run
// with the secret (2026-11-17) failed with "Additional properties are not allowed ('//'
// was unexpected)". This test reads each file on every pull request instead.
//
// It checks unknown keys only, which is the failure that happened. It walks the schema's
// `properties`, following `$ref`, and wherever the schema says
// `additionalProperties: false` it requires every key to be a declared property. Where a
// schema branches (`anyOf`, `oneOf`, `allOf`) it follows the branch only if exactly one
// branch declares object properties, and otherwise stops, so it cannot report a false
// failure. Value types are not checked; `pnpm tauri build` does that for the base config.

type Schema = {
  $ref?: string;
  properties?: Record<string, Schema>;
  additionalProperties?: boolean | Schema;
  anyOf?: Schema[];
  oneOf?: Schema[];
  allOf?: Schema[];
  definitions?: Record<string, Schema>;
};

const SCHEMA: Schema = JSON.parse(
  readFileSync(new URL("../../node_modules/@tauri-apps/cli/config.schema.json", import.meta.url), "utf8"),
);

const TAURI_DIR = new URL("../../src-tauri/", import.meta.url);
const CONFIG_FILES = readdirSync(TAURI_DIR).filter((f) => /^tauri(\..+)?\.conf\.json$/.test(f));

function resolve(node: Schema): Schema {
  let n = node;
  for (let i = 0; n.$ref && i < 20; i++) {
    const name = n.$ref.replace("#/definitions/", "");
    const target = SCHEMA.definitions?.[name];
    if (!target) return {};
    n = target;
  }
  const branches = n.anyOf ?? n.oneOf ?? n.allOf;
  if (!n.properties && branches) {
    const objectBranches = branches.map(resolve).filter((b) => b.properties);
    return objectBranches.length === 1 ? objectBranches[0] : {};
  }
  return n;
}

function unknownKeys(value: unknown, node: Schema, path: string): string[] {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return [];
  const schema = resolve(node);
  if (!schema.properties) return [];
  const found: string[] = [];
  for (const [key, child] of Object.entries(value)) {
    const childSchema = schema.properties[key];
    if (childSchema) {
      found.push(...unknownKeys(child, childSchema, `${path}.${key}`));
    } else if (schema.additionalProperties === false) {
      found.push(`${path}.${key}`);
    }
  }
  return found;
}

describe("Tauri config files", () => {
  it("finds the base config and both updater overlays", () => {
    // Guards the scan itself: an empty file list would make the next test pass vacuously.
    expect(CONFIG_FILES).toEqual(
      expect.arrayContaining(["tauri.conf.json", "tauri.updater-prod.conf.json", "tauri.updater-e2e.conf.json"]),
    );
  });

  it("the schema walk catches an unknown key", () => {
    // Guards the walker: a `"//"` key at the root and inside `bundle` must both be found.
    expect(unknownKeys({ "//": "note", bundle: { "//": "note", active: true } }, SCHEMA, "$")).toEqual([
      "$.//",
      "$.bundle.//",
    ]);
  });

  it.each(CONFIG_FILES)("%s has no key the Tauri schema rejects", (file) => {
    const config = JSON.parse(readFileSync(new URL(file, TAURI_DIR), "utf8"));
    expect(unknownKeys(config, SCHEMA, "$")).toEqual([]);
  });
});
