// Writes ui/src/lib/currencies.json: every ISO 4217 currency with the symbol Tenun writes for it.
// The app (TypeScript) and the engine (Rust) both read this one file, so they write a currency the same way.
// Rule: the English symbol ("A$", "£", "¥"); where English has only the code, the narrow symbol ("RM", "₱"),
// unless other currencies share it ("$", "kr"), then the code stays. Run: node scripts/gen-currencies.mjs
import { writeFileSync } from 'node:fs';

const codes = Intl.supportedValuesOf('currency');
const sym = (code, display) =>
  new Intl.NumberFormat('en-US', { style: 'currency', currency: code, currencyDisplay: display })
    .formatToParts(1).find((p) => p.type === 'currency').value;
const narrow = Object.fromEntries(codes.map((c) => [c, sym(c, 'narrowSymbol')]));
const count = {};
for (const s of Object.values(narrow)) count[s] = (count[s] ?? 0) + 1;
const out = {};
for (const c of codes) {
  let s = sym(c, 'symbol');
  if (s === c && narrow[c] !== c && count[narrow[c]] === 1) s = narrow[c];
  out[c] = s;
}
const path = new URL('../ui/src/lib/currencies.json', import.meta.url);
writeFileSync(path, JSON.stringify(out, null, 0).replace(/,"/g, ',\n"').replace(/^\{/, '{\n') .replace(/\}$/, '\n}') + '\n');
console.log(codes.length, 'currencies');
