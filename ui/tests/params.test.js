// Parameters and what-if (v5 build 05): the values a tile is drawn with, the steps an export holds, the control a
// parameter tile draws, and the checks of the parameters sheet.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { choicesFrom, controlOf, paramProblem, paramsOf, paramValues, snap, stepCount, stepKey, stepsOf, withParams } from '../src/lib/params.ts';

const rise = { id: 'p1', name: 'Price rise', kind: 'number', value: 0.05, min: 0, max: 0.2, step: 0.01, format: { percent: true } };
const top = { id: 'p2', name: 'Top', kind: 'number', value: 3, min: 1, max: 10, step: 1 };
const region = { id: 'p3', name: 'Region', kind: 'choice', value: 'Jawa', choices: ['Jawa', 'Bali', 'Sumatra'] };

test('a target and a line take their parameter; the tile keeps its own number when the parameter is gone', () => {
  const tile = {
    id: 't', type: 'bar', measures: [], dims: [],
    target: { value: 10, param: 'p2' },
    lines: [{ kind: 'target', value: 1, param: 'p1' }, { kind: 'average' }, { kind: 'target', value: 7, param: 'gone' }],
  };
  const drawn = withParams(tile, paramValues([rise, top]));
  assert.equal(drawn.target.value, 3);
  assert.deepEqual(drawn.lines.map((l) => l.value), [0.05, undefined, 7]);
  // A tile with nothing to take is the same object, so nothing draws again.
  const plain = { id: 'u', type: 'bar', measures: [], dims: [] };
  assert.equal(withParams(plain, paramValues([rise])), plain);
  // A choice is not a number: the target keeps its own.
  assert.equal(withParams({ ...tile, target: { value: 10, param: 'p3' } }, paramValues([region])).target.value, 10);
  assert.deepEqual(paramsOf({ ...tile, top: { n: 5, param: 'p2' } }).sort(), ['gone', 'p1', 'p2']);
});

test('values stand in for the saved ones', () => {
  assert.deepEqual(paramValues([rise, region], { p3: 'Bali' }), { p1: 0.05, p3: 'Bali' });
});

test('an export holds the steps of a parameter with at most 21 values, as the engine names them', () => {
  const steps = stepsOf(rise);
  assert.equal(steps.length, 21);
  assert.equal(steps[3], 0.03, 'no float noise');
  assert.equal(stepKey(steps[3]), '0.03');
  assert.equal(stepsOf({ ...rise, max: 0.21 }), null);
  assert.equal(stepCount({ ...rise, max: 0.21 }), 22);
  assert.deepEqual(stepsOf(region), ['Jawa', 'Bali', 'Sumatra']);
});

test('the control: a slider or a box for a number, buttons for five choices or fewer, else a dropdown', () => {
  assert.equal(controlOf(rise, undefined), 'slider');
  assert.equal(controlOf(rise, 'input'), 'input');
  assert.equal(controlOf(region, undefined), 'buttons');
  assert.equal(controlOf(region, 'dropdown'), 'dropdown');
  assert.equal(controlOf({ ...region, choices: ['a', 'b', 'c', 'd', 'e', 'f'] }, 'buttons'), 'dropdown');
  // A value is kept on the control: in range, on a step.
  assert.equal(snap(rise, 0.057), 0.06);
  assert.equal(snap(rise, -1), 0);
  assert.equal(snap(rise, 9), 0.2);
});

test('the sheet says what is wrong before the engine does', () => {
  assert.equal(paramProblem(rise, [rise]), null);
  assert.match(paramProblem({ ...rise, name: ' ' }, []), /name/);
  assert.match(paramProblem({ ...rise, id: 'p9', name: 'price RISE' }, [rise]), /Another parameter/);
  assert.match(paramProblem({ ...rise, name: 'a"b' }, []), /double quote/);
  assert.match(paramProblem({ ...rise, min: 1, max: 0 }, []), /not be below/);
  assert.match(paramProblem({ ...rise, step: 0 }, []), /above 0/);
  assert.match(paramProblem({ ...rise, step: 0.00001 }, []), /10,000/);
  assert.match(paramProblem({ ...region, choices: [] }, []), /at least one/);
  assert.deepEqual(choicesFrom('Jawa\n\n 5 \nBali\nJawa\n-1.5\n1e3'), ['Jawa', 5, 'Bali', -1.5, '1e3']);
});
