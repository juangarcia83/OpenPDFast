// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Downloads the real-world benchmark PDFs listed in bench/corpus/sources.json
// into bench/corpus/real/ and verifies their SHA-256. Files are never
// committed: only their URL, license and hash are (AGENTS.md §11).
//
// Usage: node bench/tools/fetch-corpus.mjs

import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const corpus = join(dirname(fileURLToPath(import.meta.url)), "..", "corpus");
const outDir = join(corpus, "real");
const sources = JSON.parse(await readFile(join(corpus, "sources.json"), "utf8"));
await mkdir(outDir, { recursive: true });

const sha256 = (buf) => createHash("sha256").update(buf).digest("hex");
let failed = 0;
for (const src of sources) {
  const path = join(outDir, src.file);
  const existing = await readFile(path).catch(() => null);
  if (existing && sha256(existing) === src.sha256) {
    console.log(`ok       ${src.file}`);
    continue;
  }
  process.stdout.write(`download ${src.file} (${(src.bytes / 1e6).toFixed(1)} MB)… `);
  const res = await fetch(src.url, { redirect: "follow" });
  if (!res.ok) {
    console.log(`HTTP ${res.status}`);
    failed++;
    continue;
  }
  const buf = Buffer.from(await res.arrayBuffer());
  const hash = sha256(buf);
  if (hash !== src.sha256) {
    console.log(`hash mismatch (got ${hash}); the upstream file changed, update sources.json`);
    failed++;
    continue;
  }
  await writeFile(path, buf);
  console.log("ok");
}
process.exit(failed ? 1 : 0);
