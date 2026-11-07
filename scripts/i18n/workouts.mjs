#!/usr/bin/env node
// Builds one translated workout library from the Hungarian source.
//
//   node scripts/i18n/workouts.mjs es           # writes crates/ride-model/assets/workouts.es.json
//   node scripts/i18n/workouts.mjs es --check   # reports coverage, writes nothing
//
// The translation lives in `crates/ride-model/assets/workouts.es.map.json`: a flat
// `{ "hungarian": "spanish" }` map, because the same sentence ("Bemelegítés") appears as a step
// name hundreds of times and is translated once.
//
// A missing entry fails the run. A half-translated library must never be written: the app would
// then show two languages in one workout.
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const assets = join(root, "crates", "ride-model", "assets");
const code = process.argv[2];
const check = process.argv.includes("--check");
if (!code || !/^[a-z]{2,3}$/.test(code)) {
  console.error("usage: node scripts/i18n/workouts.mjs <code> [--check]");
  process.exit(2);
}

const source = JSON.parse(readFileSync(join(assets, "workouts.json"), "utf8"));
const mapPath = join(assets, `workouts.${code}.map.json`);
const map = existsSync(mapPath) ? JSON.parse(readFileSync(mapPath, "utf8")) : {};

/** Every user-visible string of one workout, in document order. */
function texts(workout) {
  const out = [workout.name, workout.goal, workout.description];
  for (const step of workout.steps) {
    out.push(step.name);
    if (step.note) out.push(step.note);
    for (const message of step.messages ?? []) out.push(message.text);
  }
  return out;
}

const all = new Set();
for (const workout of source.workouts) for (const text of texts(workout)) all.add(text);

const missing = [...all].filter((text) => typeof map[text] !== "string" || !map[text].trim());
const unused = Object.keys(map).filter((text) => !all.has(text));

if (missing.length) {
  console.error(`${code}: ${all.size - missing.length}/${all.size} strings translated, ${missing.length} missing`);
  if (check) for (const text of missing.slice(0, 40)) console.error(`  · ${text}`);
  if (!check) process.exitCode = 1;
} else {
  console.log(`${code}: ${all.size}/${all.size} strings translated`);
}
if (unused.length) {
  console.warn(`  ${unused.length} map entries match no string (stale?):`);
  for (const text of unused.slice(0, 10)) console.warn(`  · ${text}`);
}
if (check || missing.length) process.exit(process.exitCode ?? 0);

const translated = {
  schema_version: source.schema_version,
  workouts: source.workouts.map((workout) => ({
    ...workout,
    name: map[workout.name],
    goal: map[workout.goal],
    description: map[workout.description],
    steps: workout.steps.map((step) => ({
      ...step,
      name: map[step.name],
      ...(step.note ? { note: map[step.note] } : {}),
      ...(step.messages
        ? { messages: step.messages.map((message) => ({ ...message, text: map[message.text] })) }
        : {}),
    })),
  })),
};

const dest = join(assets, `workouts.${code}.json`);
writeFileSync(dest, `${JSON.stringify(translated, null, 1)}\n`, "utf8");
console.log(`wrote ${dest}`);
