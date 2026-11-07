#!/usr/bin/env node
// Writes the translation worklists of the workout library: `target/w-<code>/chunkNN.txt`, one
// Hungarian string per line, `### <workout id>` markers between workouts.
//
//   node scripts/i18n/workout_worklist.mjs en [--chars 11000]
//
// The translator writes `chunkNN.<code>.json` next to each list (a JSON array in exactly the same
// order, no markers), and `merge_workouts.mjs <code>` turns the chunks into the flat map.
// A string that already exists in the map is skipped, so a rerun after an edit only lists the new
// text. Lines cannot contain a newline (the format is line based); the script stops if one does.
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const assets = join(root, "crates", "ride-model", "assets");
const code = process.argv[2];
const budget = Number(process.argv[process.argv.indexOf("--chars") + 1]) || 11000;
if (!code || !/^[a-z]{2,3}$/.test(code)) {
  console.error("usage: node scripts/i18n/workout_worklist.mjs <code> [--chars 11000]");
  process.exit(2);
}

const source = JSON.parse(readFileSync(join(assets, "workouts.json"), "utf8"));
const mapPath = join(assets, `workouts.${code}.map.json`);
const done = existsSync(mapPath) ? JSON.parse(readFileSync(mapPath, "utf8")) : {};

/** Every user-visible string of one workout, in document order (same order as workouts.mjs). */
function texts(workout) {
  const out = [workout.name, workout.goal, workout.description];
  for (const step of workout.steps) {
    out.push(step.name);
    if (step.note) out.push(step.note);
    for (const message of step.messages ?? []) out.push(message.text);
  }
  return out;
}

const dir = join(root, "target", `w-${code}`);
mkdirSync(dir, { recursive: true });

let chunk = [];
let size = 0;
let index = 0;
let listed = 0;
/** The map is global, so a string is translated once however many workouts repeat it. */
const seen = new Set();
const flush = () => {
  if (!chunk.length) return;
  index += 1;
  const name = `chunk${String(index).padStart(2, "0")}.txt`;
  writeFileSync(join(dir, name), `${chunk.join("\n")}\n`, "utf8");
  console.log(`${name}: ${chunk.length} strings, ${size} chars`);
  chunk = [];
  size = 0;
};

for (const workout of source.workouts) {
  const wanted = [...new Set(texts(workout))].filter(
    (text) => !done[text]?.trim() && !seen.has(text),
  );
  if (!wanted.length) continue;
  for (const text of wanted) {
    if (text.includes("\n")) {
      console.error(`a workout string contains a newline, unsupported: ${JSON.stringify(text)}`);
      process.exit(1);
    }
    seen.add(text);
  }
  chunk.push(`### ${workout.id}`);
  for (const text of wanted) {
    chunk.push(text);
    size += text.length;
    listed += 1;
  }
  if (size >= budget) flush();
}
flush();

const total = Object.values(done).filter((value) => typeof value === "string" && value.trim());
console.log(
  `${code}: ${listed} strings to translate, ${total.length} already in the map` +
    (index ? `, ${index} chunk(s) in ${dir}` : ""),
);
