// Unusual values (v5 build 09): the words a marked value gets, in the reader's number format.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { isHigh, unusualCount, unusualWords } from '../src/lib/unusual.ts';

const short = (n) => `${Math.round(n / 1e6)}M`;

test('the words say how far, which way, and against what', () => {
  assert.equal(
    unusualWords({ at: 9, spreads: 4.3, median: 267_170_000, around: 6 }, 'month', short),
    'About 4 times the usual spread above the 6 months around it (median 267M).',
  );
  assert.equal(
    unusualWords({ at: 16, spreads: -3.9, median: 359_800_000, around: 6 }, 'quarter', short),
    'About 4 times the usual spread below the 6 quarters around it (median 360M).',
  );
  // Periods that are all the same: no spread to count in.
  assert.equal(unusualWords({ at: 6, spreads: -1e308, median: 10e6, around: 6 }, null, short), 'Different from the 6 periods around it, which are all 10M.');
  assert.equal(isHigh({ at: 0, spreads: 2.6, median: 1, around: 6 }), true);
  assert.equal(unusualCount(1), '1 unusual value');
  assert.equal(unusualCount(3), '3 unusual values');
});
