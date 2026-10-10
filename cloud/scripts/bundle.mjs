// Bundles the Worker for the tests (the same code wrangler ships).
import { build } from 'esbuild';

await build({
  entryPoints: ['src/index.ts'],
  bundle: true,
  format: 'esm',
  platform: 'neutral',
  mainFields: ['module', 'main'],
  outfile: '.build/worker.js',
  logLevel: 'warning',
});
