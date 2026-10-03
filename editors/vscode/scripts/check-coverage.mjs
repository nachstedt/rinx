// Fails when line coverage of the last `npm test -- --coverage` run is below
// a floor, so a change that adds untested code fails CI:
//
//   node scripts/check-coverage.mjs 60
//
// A floor, not a target: raise it as coverage rises.
import { readFileSync } from 'node:fs';

const floor = Number(process.argv[2]);
if (!Number.isFinite(floor)) {
    console.error('usage: check-coverage.mjs <minimum line coverage in percent>');
    process.exit(2);
}
const summary = JSON.parse(readFileSync(new URL('../coverage/coverage-summary.json', import.meta.url), 'utf8'));
const lines = summary.total.lines.pct;
console.log(`Line coverage: ${lines}% (floor ${floor}%)`);
if (lines < floor) {
    console.error(`Line coverage ${lines}% is below the floor of ${floor}%.`);
    process.exit(1);
}
