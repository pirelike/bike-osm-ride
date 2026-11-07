#!/usr/bin/env node
// Writes the translation worklist of the UI catalog, grouped by the screen (source file) the
// strings appear on: `target/c-<code>/partNN.src.json`, plus `BRIEF.md` with the glossary and the
// house rules.
//
//   node scripts/i18n/catalog_worklist.mjs zh [--size 130]
//
// Each part is a JSON array of `{ key, hu, en, params, file, ctx }`. The translator writes
// `target/c-<code>/partNN.json` next to it: a flat `{ "key": "translation" }` object with exactly
// the same keys, and `merge_catalog.mjs <code>` turns the parts into `locales/<code>.json`.
//
// The English catalog is the pivot: it is complete, and a translation starts from it while the
// Hungarian source stays visible for disambiguation. `ctx` is the first source location from
// `_meta.json` (the line the string was extracted from); it is context, not text to translate.
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname, basename } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const locales = join(root, "crates", "ride-i18n", "locales");
const code = process.argv[2];
const sizeArg = process.argv.indexOf("--size");
const size = sizeArg > 0 ? Number(process.argv[sizeArg + 1]) : 130;
if (!code || !/^[a-z]{2,3}$/.test(code)) {
  console.error("usage: node scripts/i18n/catalog_worklist.mjs <code> [--size 130]");
  process.exit(2);
}

const hu = JSON.parse(readFileSync(join(locales, "hu.json"), "utf8"));
const en = JSON.parse(readFileSync(join(locales, "en.json"), "utf8"));
const meta = JSON.parse(readFileSync(join(locales, "_meta.json"), "utf8"));

/** The screen a key belongs to: the source file of its first context. */
function sourceFile(key) {
  const ctx = meta[key]?.ctx?.[0] ?? "";
  const match = ctx.match(/^([^:]+):/);
  return match ? match[1] : "(ismeretlen)";
}

/** One compact line of context: the location plus the expression the string sits in. */
function context(key) {
  const ctx = meta[key]?.ctx?.[0] ?? "";
  const trimmed = ctx.replace(/\s+/g, " ").trim();
  return trimmed.length > 220 ? `${trimmed.slice(0, 217)}…` : trimmed;
}

const keys = Object.keys(hu).filter((key) => !key.startsWith("i18n.test."));
const byFile = new Map();
for (const key of keys) {
  const file = sourceFile(key);
  if (!byFile.has(file)) byFile.set(file, []);
  byFile.get(file).push(key);
}

// Screen order: the shell first, then the HUD, the route/profile panels, the dialogs, the review
// page, then the logic and error text. Unknown files go last, alphabetically.
const ORDER = [
  "ride-app/src/shell.rs",
  "ride-app/src/dock.rs",
  "ride-app/src/hooks.rs",
  "ride-app/src/state.rs",
  "ride-app/src/wake.rs",
  "ride-app/src/controller.rs",
  "ride-app/src/controller/review.rs",
  "ride-app/src/controller/backup.rs",
  "ride-app/src/android_picker.rs",
  "ride-app/src/updater.rs",
  "ride-app/src/guard.rs",
  "ride-ui-hud/src/lib.rs",
  "ride-ui-hud/src/dash.rs",
  "ride-ui-map/src/lib.rs",
  "ride-ui-profile/src/lib.rs",
  "ride-ui-profile/src/workout.rs",
  "ride-ui-profile/src/series.rs",
  "ride-ui-profile/src/swipe.rs",
  "ride-ui-dialogs/src/welcome.rs",
  "ride-ui-dialogs/src/settings.rs",
  "ride-ui-dialogs/src/devices.rs",
  "ride-ui-dialogs/src/workouts.rs",
  "ride-ui-dialogs/src/summary.rs",
  "ride-ui-dialogs/src/about.rs",
  "ride-ui-dialogs/src/helpers.rs",
  "ride-ui-dialogs/src/virtual_rider.rs",
  "ride-ui-review/src/lib.rs",
  "ride-ui-review/src/power.rs",
  "ride-ui-review/src/chart.rs",
];
const rank = (file) => {
  const i = ORDER.indexOf(file);
  return i < 0 ? ORDER.length : i;
};
const files = [...byFile.keys()].sort(
  (a, b) => rank(a) - rank(b) || a.localeCompare(b),
);

const dir = join(root, "target", `c-${code}`);
mkdirSync(dir, { recursive: true });

let part = [];
let partSize = 0;
let index = 0;
const parts = [];
const flush = () => {
  if (!part.length) return;
  index += 1;
  const name = `part${String(index).padStart(2, "0")}`;
  const rows = part.map((key) => ({
    key,
    hu: hu[key],
    en: en[key] ?? "",
    params: meta[key]?.params ?? [],
    file: sourceFile(key),
    ctx: context(key),
  }));
  writeFileSync(join(dir, `${name}.src.json`), `${JSON.stringify(rows, null, 2)}\n`, "utf8");
  parts.push({ name, keys: part.length, files: [...new Set(rows.map((row) => row.file))] });
  console.log(`${name}: ${part.length} keys (${parts.at(-1).files.join(", ")})`);
  part = [];
  partSize = 0;
};

for (const file of files) {
  const group = byFile.get(file).sort();
  // Keep a screen in one part where possible; a large screen may span parts.
  if (part.length && partSize + group.length > size) flush();
  for (const key of group) {
    part.push(key);
    partSize += 1;
    if (partSize >= size) flush();
  }
}
flush();

writeFileSync(
  join(dir, "manifest.json"),
  `${JSON.stringify({ code, source: "hu", pivot: "en", parts }, null, 2)}\n`,
  "utf8",
);
console.log(`${code}: ${keys.length} keys in ${parts.length} part(s) under ${dir}`);
