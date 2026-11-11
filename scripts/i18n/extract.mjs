#!/usr/bin/env node
// Extraction and mechanical application of the Hungarian UI strings.
//
// The source language is Hungarian, so a string literal that contains a Hungarian character is a
// candidate. The tool gives every distinct text a key (`<crate>.<module>.<slug>`), writes
// `hu.json` / `_meta.json`, and with `--apply` replaces the literal with `t("key")` (or
// `t("key").to_string()` where the surrounding prop wants a `String`).
//
// `format!` templates are *reported*, never rewritten: they need a human to name the arguments and
// turn positional `{}` into `{name}` (see PROMPT.md).
//
// Usage:
//   node scripts/i18n/extract.mjs                 # scan, write hu.json + _meta.json
//   node scripts/i18n/extract.mjs --crate ride-ui-hud
//   node scripts/i18n/extract.mjs --crate ride-ui-hud --apply
//   node scripts/i18n/extract.mjs --list          # print every candidate with its context
//   node scripts/i18n/extract.mjs --prune         # drop keys no Rust source looks up any more
//
// Run from the repository root. The tests and the `examples/` previews are not UI: they are
// skipped, as is `vendor/`.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const CRATES = path.join(ROOT, "crates");
const LOCALES = path.join(CRATES, "ride-i18n", "locales");

const HUNGARIAN = /[áéíóöőúüűÁÉÍÓÖŐÚÜŰ]/;
// Hungarian UI text that carries no accented letter. Curated from a scan of every literal in a
// UI position; the accented heuristic alone would leave these Hungarian.
const EXTRA = new Set([
  "Adatok",
  "Fokozat",
  "Grafikonok",
  "Nehezebb fokozat",
  "Nehezebb fokozat (→)",
  "Rendben",
  "Statisztika",
  "Szint · teljes",
  "A Firefox ablaka nem jelent meg.",
  "A Firefox indul…",
  "Illessz be egy bikerouter-linket.",
  "a HTTP-kliens nem indult el",
  "nincs Bluetooth-adapter",
  "Aerob alap",
  "Aerob",
  "Anaerob",
  "VO₂max",
  "Alacsony kadencia, hegymenet.",
  "Egyenletes",
  "Hegymenet",
  "Intervallum",
  "Jelleg",
  "Munka",
  "Sprint",
  "Sweet spot",
  "Szabad",
  "Szint",
  "Teszt",
  "Vegyes",
  "Verseny",
  "Versenydinamika",
  "A megosztott link",
  "A projekt licence",
  "Alaphelyzet",
  "Automatikus",
  "Blokkok",
  "Csatlakoztatva",
  "Elrejt",
  "Energia",
  "Gyenge jel",
  "Legjobb (PR)",
  "Licenc",
  "Lista",
  "Max. pulzus",
  "Megnyitva",
  "Mind",
  "Mutat",
  "Nincs jel",
  "Nincs kapcsolat",
  "Nyelv",
  "Okos trainer – FTMS",
  "Pulzus",
  "Sikertelen",
  "Szegmensek",
  "Szellem",
  "Teljes",
  "Nyiss meg egy projektet a",
  "Azonnali",
  "Befejezve",
  "Blokk",
  "Energia (kcal)",
  "Fajlagos",
  "Hossz",
  "Kadencia",
  "Leadott",
  "Megtett",
  "Megtett szint",
  "Menetadatok",
  "Munka (kJ)",
  "Nincs szenzoradat",
  "Rajt",
  "A panel tartalma",
  "Ablak",
  "Nincs mit kirajzolni",
  "Szegmens",
  "A szakasz adatai",
  "Grafikon",
  "Havi",
  "Heti",
  "Legjobb (W)",
  "Mikor",
  "Mit mutasson a grafikon",
  "Oldal",
  "Szakasz",
  "Vissza",
  "Google hiba",
  "Itt nincs Street View",
  "Nincs kapcsolat a Google-lel",
  "Nincs megadva Google Maps API-kulcs.",
]);
const STRING_PROPS = new Set([
  "label",
  "hint",
  "title",
  "aria_label",
  "note",
  "unit",
  "shortcut",
  "placeholder",
  "description",
  "message",
  "tooltip",
  "caption",
  "text",
]);

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const value = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? args[i + 1] : fallback;
};

const onlyCrate = value("--crate", null);
const apply = flag("--apply");
const list = flag("--list");

if (flag("--self-test")) {
  const fixture = '#[cfg(test)] mod tests { let json = r#"}"#; /* } /* { */ } */ let ch = \'}\'; let label = "Kettő"; }\nfn real() { let label = "Éles"; }';
  const labels = scanStrings(stripTestBlocks(fixture)).map(s => s.text);
  if (JSON.stringify(labels) !== JSON.stringify(["Éles"])) throw new Error("Test fixtures escaped the test-block mask");
  const external = '#[cfg(test)] mod tests;\nfn real() { let label = "Éles"; }';
  if (!stripTestBlocks(external).includes('let label = "Éles"')) throw new Error("An external test module hid production code");
  console.log("Passed extractor test-block regressions");
  process.exit(0);
}

// ───────────────────────────── pruning ─────────────────────────────

// A key whose literal no longer appears in any Rust source (the text changed, the code went) is
// dead: it can never be looked up, yet every translator keeps translating it. `--prune` removes
// it from every catalog and from `_meta.json` (plural keys by their head, `items` for
// `items.one` / `items.other`; the `i18n.test.*` fixtures stay). The catalog test
// `every_source_key_is_used` fails while one is left.
if (flag("--prune")) {
  const sources = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        if (entry.name !== "target") walk(full);
      } else if (entry.name.endsWith(".rs")) {
        sources.push(fs.readFileSync(full, "utf8"));
      }
    }
  };
  walk(CRATES);
  const text = sources.join("\n");
  const head = (key) => key.replace(/\.(one|other)$/, "");
  const used = (key) => key.startsWith("i18n.test.") || text.includes(`"${head(key)}"`);
  let removed = new Set();
  for (const file of fs.readdirSync(LOCALES).filter((f) => f.endsWith(".json")).sort()) {
    const full = path.join(LOCALES, file);
    const catalog = readJson(full);
    let changed = false;
    for (const key of Object.keys(catalog)) {
      if (key === "_doc" || used(key)) continue;
      delete catalog[key];
      removed.add(key);
      changed = true;
    }
    if (changed) writeJson(full, catalog);
  }
  removed = [...removed].sort();
  console.log(`${removed.length} dead key(s) removed${removed.length ? ":\n  " + removed.join("\n  ") : ""}`);
  process.exit(0);
}

// ───────────────────────────── Rust scanning ─────────────────────────────

/** UTF-16 offsets of `#[cfg(test)]` items removed before scanning. */
function testBlocks(src) {
  // Braces in Rust strings, char literals and comments do not close an item.
  // In particular, CSS/JSON test fixtures often contain unbalanced literal braces.
  // Offsets below are UTF-16, as are slice()/scanStrings(); keep code units intact.
  const masked = src.split("");
  for (const { start, end } of scanStrings(src)) {
    for (let i = start; i < end; i++) if (masked[i] !== "\n") masked[i] = " ";
  }
  let code = masked.join("").replace(/'(?:\\.|[^'\\])'/g, (s) => " ".repeat(s.length));
  const clean = code.split("");
  for (let i = 0; i < code.length;) {
    let end = i;
    if (code.startsWith("//", i)) {
      end = code.indexOf("\n", i);
      if (end < 0) end = code.length;
    } else if (code.startsWith("/*", i)) {
      let depth = 1;
      end = i + 2;
      while (end < code.length && depth) {
        if (code.startsWith("/*", end)) { depth++; end += 2; }
        else if (code.startsWith("*/", end)) { depth--; end += 2; }
        else end++;
      }
    }
    if (end > i) {
      for (; i < end; i++) if (clean[i] !== "\n") clean[i] = " ";
    } else i++;
  }
  code = clean.join("");
  const blocks = [];
  const re = /#\[cfg\(test\)\]/g;
  let m;
  while ((m = re.exec(code))) {
    let i = m.index + m[0].length;
    // Skip attributes and visibility until the item keyword.
    while (i < src.length && /\s/.test(src[i])) i++;
    while (src.startsWith("#[", i)) {
      const close = src.indexOf("]", i);
      if (close < 0) break;
      i = close + 1;
      while (i < src.length && /\s/.test(src[i])) i++;
    }
    const rest = src.slice(i, i + 80);
    if (!/^(pub(\([^)]*\))?\s+)?(mod|impl|fn)\b/.test(rest)) continue;
    const externalModule = rest.match(/^(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*;/);
    if (externalModule) {
      blocks.push([m.index, i + externalModule[0].length]);
      re.lastIndex = i + externalModule[0].length;
      continue;
    }
    const brace = code.indexOf("{", i);
    if (brace < 0) continue;
    let depth = 1;
    let j = brace + 1;
    while (j < src.length && depth > 0) {
      if (code[j] === "{") depth++;
      else if (code[j] === "}") depth--;
      j++;
    }
    blocks.push([m.index, j]);
    re.lastIndex = j;
  }
  return blocks;
}

function stripTestBlocks(src) {
  const blocks = testBlocks(src);
  if (!blocks.length) return src;
  let out = "";
  let last = 0;
  for (const [start, end] of blocks) {
    out += src.slice(last, start);
    // Keep the newlines so line numbers stay valid.
    out += src.slice(start, end).replace(/[^\n]/g, " ");
    last = end;
  }
  out += src.slice(last);
  return out;
}

function unescape(text) {
  return text.replace(/\\(u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)/g, (whole, esc) => {
    if (esc.startsWith("u{")) return String.fromCodePoint(parseInt(esc.slice(2, -1), 16));
    if (esc.startsWith("x")) return String.fromCharCode(parseInt(esc.slice(1), 16));
    switch (esc) {
      case "n": return "\n";
      case "t": return "\t";
      case "r": return "\r";
      case "0": return "\0";
      case "\\": return "\\";
      case '"': return '"';
      case "'": return "'";
      default: return esc;
    }
  });
}

/** Every string literal with its source span. Raw strings and escapes included. */
function scanStrings(src) {
  const out = [];
  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src[i];
    if (c === "/" && src[i + 1] === "/") {
      const nl = src.indexOf("\n", i);
      i = nl < 0 ? n : nl;
      continue;
    }
    if (c === "/" && src[i + 1] === "*") {
      const close = src.indexOf("*/", i + 2);
      i = close < 0 ? n : close + 2;
      continue;
    }
    // raw string: r"…", r#"…"#, br#"…"#
    const rawStart = c === "r" ? i + 1 : c === "b" && src[i + 1] === "r" ? i + 2 : -1;
    if (rawStart >= 0) {
      let j = rawStart;
      let hashes = 0;
      while (src[j] === "#") { hashes++; j++; }
      if (src[j] === '"') {
        const close = '"' + "#".repeat(hashes);
        const end = src.indexOf(close, j + 1);
        if (end >= 0) {
          out.push({
            start: i,
            end: end + close.length,
            text: src.slice(j + 1, end),
            raw: true,
          });
          i = end + close.length;
          continue;
        }
      }
    }
    // byte/normal string: b"…" / "…"
    const quote = c === "b" && src[i + 1] === '"' ? i + 1 : c === '"' ? i : -1;
    if (quote >= 0) {
      let j = quote + 1;
      let text = "";
      while (j < n) {
        if (src[j] === "\\") {
          text += src[j] + (src[j + 1] ?? "");
          j += 2;
          continue;
        }
        if (src[j] === '"') {
          out.push({ start: i, end: j + 1, text: unescape(text), raw: false });
          i = j + 1;
          break;
        }
        text += src[j];
        j++;
      }
      if (j >= n) break;
      continue;
    }
    // char literal or lifetime
    if (c === "'") {
      let j = i + 1;
      if (src[j] === "\\") j += 2;
      else j += 1;
      if (src[j] === "'") {
        i = j + 1;
        continue;
      }
      i++;
      continue;
    }
    i++;
  }
  return out;
}

function lineOf(src, offset) {
  return src.slice(0, offset).split("\n").length;
}

function lineText(src, offset) {
  const start = src.lastIndexOf("\n", offset) + 1;
  const end = src.indexOf("\n", offset);
  return src.slice(start, end < 0 ? src.length : end).trim();
}

function functionOf(src, offset) {
  const prefix = src.slice(0, offset);
  const re = /\bfn\s+([a-z_][a-z0-9_]*)/g;
  let name = null;
  let m;
  while ((m = re.exec(prefix))) name = m[1];
  return name;
}

// ───────────────────────────── keys ─────────────────────────────

function slugify(text) {
  const slug = text
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 52);
  return slug || "text";
}

function namespaceOf(rel) {
  const parts = rel.split(/[\\/]/);
  const crate = parts[0].replace(/^ride-/, "").replace(/-/g, "_");
  const mods = parts.slice(2, -1).map((m) => m.replace(/\.rs$/, ""));
  const file = parts[parts.length - 1].replace(/\.rs$/, "");
  if (file !== "lib" && file !== "mod" && file !== "main") mods.push(file);
  return [crate, ...mods].join(".");
}

// ───────────────────────────── walk ─────────────────────────────

function* rustFiles(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    const rel = path.relative(CRATES, full).split(path.sep).join("/");
    if (entry.isDirectory()) {
      if (entry.name === "examples" || entry.name === "vendor" || entry.name === "tests") continue;
      yield* rustFiles(full);
      continue;
    }
    if (!entry.name.endsWith(".rs")) continue;
    // Only the Cargo build script at the crate root is generated code; `src/build.rs` is an
    // ordinary module (route building) whose messages a rider sees.
    if (entry.name === "build.rs" && rel === `${rel.split("/")[0]}/build.rs`) continue;
    if (/test/i.test(entry.name)) continue;
    yield { full, rel };
  }
}

const byText = new Map(); // text -> { key, occurrences }
const keyTaken = new Set();

for (const { full, rel } of rustFiles(CRATES)) {
  if (onlyCrate && !rel.startsWith(onlyCrate + "/")) continue;
  const crate = rel.split("/")[0];
  if (crate === "ride-i18n") continue;
  const source = fs.readFileSync(full, "utf8");
  const src = stripTestBlocks(source);
  const literals = scanStrings(src);
  for (const literal of literals) {
    if (!HUNGARIAN.test(literal.text) && !EXTRA.has(literal.text.trim())) continue;
    // Skip CSS/HTML blobs: they are not UI labels (the map's inline SVG labels, the Street View
    // mock's fake page). The words inside such a page are built from `t()` separately.
    const trimmed = literal.text.trim();
    if (!trimmed) continue;
    if (/<[a-z]+[\s>]/i.test(trimmed)) continue;
    const before = src.slice(Math.max(0, literal.start - 300), literal.start);
    const after = src.slice(literal.end, literal.end + 20);
    const kind = /\bformat!\s*\(\s*$/.test(before)
      ? "format"
      : /#\[error\(\s*$/.test(before)
        ? "attribute"
        : /^\s*\.to_string\(\)/.test(after)
          ? "to_string"
          : "plain";
    const line = lineOf(src, literal.start);
    const ctx = `${rel}:${line} ${functionOf(src, literal.start) ?? "?"} — ${lineText(src, literal.start)}`;

    const existing = byText.get(literal.text);
    if (existing) {
      existing.occurrences.push({
        file: rel,
        line,
        start: literal.start,
        end: literal.end,
        kind,
        ctx,
        prop: propOf(src, literal.start),
      });
      continue;
    }
    const ns = namespaceOf(rel);
    let base = `${ns}.${slugify(literal.text)}`;
    let key = base;
    let n = 2;
    while (keyTaken.has(key)) key = `${base}_${n++}`;
    keyTaken.add(key);
    byText.set(literal.text, {
      key,
      text: literal.text,
      occurrences: [
        {
          file: rel,
          line,
          start: literal.start,
          end: literal.end,
          kind,
          ctx,
          prop: propOf(src, literal.start),
        },
      ],
    });
  }
}

function propOf(src, start) {
  const prefix = src.slice(Math.max(0, start - 80), start);
  const m = /([a-z_][a-z0-9_]*)\s*:\s*$/.exec(prefix);
  return m ? m[1] : null;
}

const entries = [...byText.values()].sort((a, b) => a.key.localeCompare(b.key));

// ───────────────────────────── report ─────────────────────────────

if (list) {
  for (const entry of entries) {
    for (const occ of entry.occurrences) {
      console.log(`${entry.key}\t${occ.kind}\t${occ.ctx}`);
    }
  }
  console.log(`\n${entries.length} keys, ${entries.reduce((n, e) => n + e.occurrences.length, 0)} occurrences`);
  process.exit(0);
}

const formatEntries = entries.filter((e) => e.occurrences.some((o) => o.kind === "format"));
const plainEntries = entries.filter((e) => !e.occurrences.some((o) => o.kind === "format"));

// ───────────────────────────── catalogs ─────────────────────────────

function readJson(file) {
  return fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, "utf8")) : {};
}

function writeJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify(value, null, 2) + "\n", "utf8");
}

const huPath = path.join(LOCALES, "hu.json");
const metaPath = path.join(LOCALES, "_meta.json");
const hu = readJson(huPath);
const meta = readJson(metaPath);
delete meta._doc;

for (const entry of entries) {
  hu[entry.key] = entry.text;
  const previous = meta[entry.key] ?? {};
  const contexts = new Set(previous.ctx ?? []);
  for (const occ of entry.occurrences) contexts.add(occ.ctx);
  const params = new Set(previous.params ?? []);
  for (const m of entry.text.matchAll(/\{([a-zA-Z_][a-zA-Z0-9_]*)\}/g)) params.add(m[1]);
  meta[entry.key] = {
    ctx: [...contexts].slice(0, 8),
    params: [...params],
    ...(previous.tight ? { tight: true } : {}),
    ...(previous.note ? { note: previous.note } : {}),
  };
}

const sortObject = (obj) =>
  Object.fromEntries(Object.keys(obj).sort().map((k) => [k, obj[k]]));

writeJson(huPath, sortObject(hu));
writeJson(metaPath, { _doc: readJson(metaPath)._doc ?? "Translator context: ctx (where the string is used), params (placeholders), tight (must not grow much), note.", ...sortObject(meta) });

console.log(
  `${entries.length} keys (${plainEntries.length} plain, ${formatEntries.length} with format!), ` +
    `${entries.reduce((n, e) => n + e.occurrences.length, 0)} occurrences`
);

// ───────────────────────────── apply ─────────────────────────────

if (!apply) process.exit(0);

const touched = new Map();
for (const entry of entries) {
  for (const occ of entry.occurrences) {
    if (occ.kind === "format" || occ.kind === "attribute") continue;
    const list = touched.get(occ.file) ?? [];
    list.push({ ...occ, key: entry.key });
    touched.set(occ.file, list);
  }
}

for (const [file, occurrences] of touched) {
  const full = path.join(CRATES, file);
  let src = fs.readFileSync(full, "utf8");
  const original = src;
  // Replace from the end so earlier offsets stay valid.
  occurrences.sort((a, b) => b.start - a.start);
  for (const occ of occurrences) {
    const base = `t(${JSON.stringify(occ.key)})`;
    const needsString =
      occ.kind !== "to_string" && occ.prop && STRING_PROPS.has(occ.prop);
    // A bare expression in an rsx child position must be wrapped: `div { "text" }` becomes
    // `div { {t("key")} }`. Function arguments and array elements stay unwrapped.
    const before = original.slice(0, occ.start).trimEnd().slice(-1);
    const after = original.slice(occ.end).trimStart()[0];
    const textNode =
      (before === "," || before === "{") && (after === "}" || after === ",");
    const expr = needsString ? `${base}.to_string()` : base;
    const replacement = textNode ? `{${expr}}` : expr;
    src = src.slice(0, occ.start) + replacement + src.slice(occ.end);
  }
  if (!/^use ride_i18n::\{?t\b/m.test(src) && !/^use ride_i18n::t;/m.test(src)) {
    src = addUse(src);
  }
  fs.writeFileSync(full, src, "utf8");
}

function addUse(src) {
  const lines = src.split("\n");
  let lastUse = -1;
  for (let i = 0; i < Math.min(lines.length, 160); i++) {
    if (/^use .*;\s*$/.test(lines[i])) lastUse = i;
  }
  const insertAt = lastUse >= 0 ? lastUse + 1 : firstItemLine(lines);
  lines.splice(insertAt, 0, "use ride_i18n::t;");
  return lines.join("\n");
}

function firstItemLine(lines) {
  let i = 0;
  while (i < lines.length && (/^\/\//.test(lines[i]) || lines[i].trim() === "")) i++;
  return i;
}

console.log(`applied to ${touched.size} files`);
if (formatEntries.length) {
  console.log(`\nformat! templates left for hand work (${formatEntries.length}):`);
  for (const entry of formatEntries) console.log(`  ${entry.key}\t${entry.text}`);
}
