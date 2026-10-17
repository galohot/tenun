import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MAX_URL, reportText, reportUrl, scrub, systemOf } from '../src/lib/report.ts';

test('the error keeps its words and loses paths, links, addresses and values', () => {
  const e = scrub(
    `Cannot read C:\\Users\\Ani\\Sales 2026.xlsx: sheet 'Gaji Rahasia' has a value "Rp 12.500.000" at /home/ani/data/q3.csv, see https://intra.example/x?id=4 or mail ani@example.co.id; also \\\\server\\share\\f.csv and ~/x.csv`,
  );
  for (const leak of ['Users', 'Ani', 'Gaji', 'Rahasia', '12.500.000', '/home', 'q3.csv', 'intra.example', 'ani@', 'server', 'share', 'x.csv']) {
    assert.ok(!e.includes(leak), `${leak} leaked: ${e}`);
  }
  assert.match(e, /^Cannot read \[path\]/);
  assert.match(e, /has a value "…" at \[path\]/);
});

test('no planted path or value reaches the address', () => {
  const error = scrub('Column "omzet_cabang_7" not found in /srv/rahasia/data.csv');
  const { url } = reportUrl({ what: 'The chart is empty', version: '0.7.0', system: 'Linux, desktop app', error });
  assert.ok(!url.includes('omzet') && !url.includes('rahasia') && !url.includes('srv'));
  assert.ok(url.startsWith('https://github.com/galohot/tenun/issues/new?template=problem.yml'));
});

test('a long text is cut so the address fits', () => {
  const r = reportUrl({ what: 'x '.repeat(10000), version: '0.7.0', system: 'Windows, desktop app', error: '' });
  assert.ok(r.cut);
  assert.ok(r.url.length <= MAX_URL);
  assert.ok(!reportUrl({ what: 'short', version: '0.7.0', system: 'Windows, desktop app', error: '' }).cut);
});

test('the system and the copied text', () => {
  assert.equal(systemOf('Mozilla/5.0 (Windows NT 10.0; Win64; x64)', true), 'Windows, desktop app');
  assert.equal(systemOf('Mozilla/5.0 (X11; Linux x86_64)', false), 'Linux, browser');
  assert.equal(systemOf('Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)', true), 'macOS, desktop app');
  assert.match(reportText({ what: '', version: '0.7.0', system: 'Linux, browser', error: '' }), /\(not written\)[\s\S]*Last error: \(none\)/);
});
