// Prints the line coverage of the extension's own sources (src/, without its
// tests) from the last `npm test -- --coverage` run, and fails below a floor,
// so a change that adds untested code fails CI:
//
//   node scripts/check-coverage.mjs 70
//
// Computed here rather than read off c8's total because the e2e tests load the
// esbuild bundle, whose source map also covers the libraries it carries.
// A floor, not a target: raise it as coverage rises.
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const floor = Number(process.argv[2]);
if (!Number.isFinite(floor)) {
    console.error('usage: check-coverage.mjs <minimum line coverage in percent>');
    process.exit(2);
}
const sources = fileURLToPath(new URL('../src/', import.meta.url));
const summary = JSON.parse(readFileSync(new URL('../coverage/coverage-summary.json', import.meta.url), 'utf8'));
const own = Object.entries(summary).filter(([file]) => file.startsWith(sources) && !file.startsWith(`${sources}test/`));
let covered = 0;
let total = 0;
for (const [file, { lines }] of own) {
    console.log(`${String(lines.pct).padStart(6)}%  ${file.slice(sources.length)}`);
    covered += lines.covered;
    total += lines.total;
}
const percent = total === 0 ? 0 : Math.round((covered / total) * 10_000) / 100;
console.log(`Line coverage of src/: ${percent}% (floor ${floor}%)`);
if (percent < floor) {
    console.error(`Line coverage ${percent}% is below the floor of ${floor}%.`);
    process.exit(1);
}
