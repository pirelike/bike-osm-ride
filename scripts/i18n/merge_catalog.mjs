#!/usr/bin/env node
// Zips the translated catalog chunks into `crates/ride-i18n/locales/<code>.json`.
//
//   node scripts/i18n/merge_catalog.mjs en            # report coverage, write the catalog
//   node scripts/i18n/merge_catalog.mjs en --check    # report only
//
// The chunks live in `target/c-<code>/part<NN>.json`, each a flat `{ "key": "text" }` object.
// They are authored against the Hungarian source, so the merge is a plain union: a key that
// appears in two chunks with different text is a hard error, and the coverage report names every
// source key that no chunk translated.
//
// `i18n.test.*` keys are the crate's own fixtures: they are not part of the source text set and
// are carried over from the existing catalog unchanged.
import { readFileSync, writeFileSync, existsSync, readdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const locales = join(root, "crates", "ride-i18n", "locales");
const code = process.argv[2];
const check = process.argv.includes("--check");
if (!code || !/^[a-z]{2,3}(-[A-Za-z]{2,4})?$/.test(code)) {
  console.error("usage: node scripts/i18n/merge_catalog.mjs <code> [--check]");
  process.exit(2);
}

const source = JSON.parse(readFileSync(join(locales, "hu.json"), "utf8"));
const dir = join(root, "target", `c-${code}`);

/** `{ key: text }` of every chunk, in file-name order. */
const parts = existsSync(dir)
  ? readdirSync(dir)
      .filter((name) => /^part\d+\.json$/.test(name))
      .sort()
      .map((name) => ({ name, part: JSON.parse(readFileSync(join(dir, name), "utf8")) }))
  : [];

const merged = new Map();
for (const { name, part } of parts) {
  for (const [key, value] of Object.entries(part)) {
    if (typeof value !== "string" || !value.trim()) {
      console.error(`${name}: ${key} is empty`);
      process.exit(1);
    }
    const seen = merged.get(key);
    if (seen !== undefined && seen !== value) {
      console.error(`${name}: ${key} translated twice, differently:\n  ${seen}\n  ${value}`);
      process.exit(1);
    }
    merged.set(key, value);
  }
}

const sourceKeys = Object.keys(source).filter((key) => !key.startsWith("i18n.test."));
/** Plural forms collapse onto their bare key: `items.one` satisfies `items`. */
const bare = (key) => key.replace(/\.(one|other)$/, "");
const translated = new Set([...merged.keys()].map(bare));
const missing = sourceKeys.filter((key) => !translated.has(bare(key)));
const sourceSet = new Set(sourceKeys.map(bare));
const invented = [...translated].filter((key) => !sourceSet.has(key));
if (invented.length) {
  console.error(`${code}: ${invented.length} key(s) the source does not know:`);
  for (const key of invented.slice(0, 20)) console.error(`  · ${key}`);
  process.exit(1);
}

console.log(
  `${code}: ${sourceKeys.length - missing.length}/${sourceKeys.length} keys translated` +
    `${parts.length ? ` (${parts.length} chunk${parts.length === 1 ? "" : "s"})` : ""}`,
);
if (missing.length) {
  // Group by prefix so the report is actionable rather than a wall of keys.
  const byPrefix = new Map();
  for (const key of missing) {
    const prefix = key.split(".")[0];
    byPrefix.set(prefix, (byPrefix.get(prefix) ?? 0) + 1);
  }
  console.log(
    `  missing: ${[...byPrefix].map(([prefix, n]) => `${prefix} ${n}`).join(", ")}`,
  );
  for (const key of missing.slice(0, 10)) console.log(`  · ${key}  =  ${source[key]}`);
}

if (check || missing.length) process.exit(missing.length ? 1 : 0);

// Fixtures are not translated text; keep whatever the shipped catalog already had.
const existing = existsSync(join(locales, `${code}.json`))
  ? JSON.parse(readFileSync(join(locales, `${code}.json`), "utf8"))
  : {};
for (const [key, value] of Object.entries(existing)) {
  if (key.startsWith("i18n.test.")) merged.set(key, value);
}

const sorted = {};
for (const key of [...merged.keys()].sort()) sorted[key] = merged.get(key);
const dest = join(locales, `${code}.json`);
writeFileSync(dest, `${JSON.stringify(sorted, null, 2)}\n`, "utf8");
console.log(`wrote ${dest}`);
