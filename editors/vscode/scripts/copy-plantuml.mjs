// Copies PlantUML's JavaScript build (`@plantuml/core`) into dist/plantuml/,
// where the preview's webview loads it from to draw diagrams (ADR-042). Only
// what drawing needs is copied: the engine, Graphviz, and the themes and icons
// it looks up as globals — not the 1.8 MB of emoji, nor the demo pages — and
// the licence they are distributed under.
import { copyFileSync, mkdirSync } from 'node:fs';

const FILES = ['plantuml.js', 'viz-global.js', 'themes.js', 'openiconic.js', 'LICENSE'];

const source = new URL('../node_modules/@plantuml/core/', import.meta.url);
const target = new URL('../dist/plantuml/', import.meta.url);
mkdirSync(target, { recursive: true });
for (const file of FILES) copyFileSync(new URL(file, source), new URL(file, target));
