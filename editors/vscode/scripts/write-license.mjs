// Writes LICENSE for packaging: vsce recognizes only a file of that name,
// while rinx is dual-licensed in the repository root's LICENSE-MIT and
// LICENSE-APACHE. Generated rather than copied into git, so the texts have
// one home.
import { readFileSync, writeFileSync } from 'node:fs';

const root = (name) => readFileSync(new URL(`../../../${name}`, import.meta.url), 'utf8');

writeFileSync(
    new URL('../LICENSE', import.meta.url),
    [
        'rinx is licensed under either of the following licenses, at your option.',
        '',
        '=== MIT License ===',
        '',
        root('LICENSE-MIT').trimEnd(),
        '',
        '=== Apache License, Version 2.0 ===',
        '',
        root('LICENSE-APACHE').trimEnd(),
        '',
    ].join('\n'),
);
