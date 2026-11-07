#!/usr/bin/env node
// Zips the ordered Hungarian worklist with the translated arrays and writes the flat map
// `crates/ride-model/assets/workouts.<code>.map.json`.
//
//   node scripts/i18n/merge_workouts.mjs es
//
// Worklists live in `target/w-es/chunkNN.txt` (one string per line, `### <id>` markers) and the
// translations in `target/w-es/chunkNN.es.json` (a JSON array in exactly the same order). The
// array form keeps the translation output about half the size of a `{hu: es}` object and makes a
// missing or extra entry a hard error instead of a silent shift.
import { readFileSync, writeFileSync, existsSync, readdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const code = process.argv[2] ?? "es";
const dir = join(root, "target", `w-${code}`);
if (!existsSync(dir)) {
  console.error(`no worklists in ${dir}`);
  process.exit(2);
}

const map = {};
const files = readdirSync(dir)
  .filter((name) => /^chunk\d+\.txt$/.test(name))
  .sort();
let done = 0;
for (const name of files) {
  const hu = readFileSync(join(dir, name), "utf8")
    .split("\n")
    .filter((line) => line.trim() && !line.startsWith("###"));
  const esPath = join(dir, name.replace(/\.txt$/, `.${code}.json`));
  if (!existsSync(esPath)) {
    console.log(`${name}: not translated yet (${hu.length} strings)`);
    continue;
  }
  const es = JSON.parse(readFileSync(esPath, "utf8"));
  if (es.length !== hu.length) {
    console.error(`${name}: ${es.length} translations for ${hu.length} strings`);
    process.exit(1);
  }
  hu.forEach((text, i) => {
    const target = es[i];
    if (typeof target !== "string" || !target.trim()) {
      console.error(`${name}: empty translation at #${i + 1}: ${text}`);
      process.exit(1);
    }
    if (map[text] !== undefined && map[text] !== target) {
      console.error(`${name}: "${text}" translated twice, differently`);
      process.exit(1);
    }
    map[text] = target;
  });
  done += hu.length;
}

const dest = join(root, "crates", "ride-model", "assets", `workouts.${code}.map.json`);
writeFileSync(dest, `${JSON.stringify(map, null, 1)}\n`, "utf8");
console.log(`${done} strings in ${dest}`);
