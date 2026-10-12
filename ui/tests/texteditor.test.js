// The block editor of a text tile (build 10): blocks from the text and back, and the edits, as pure functions.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { addLink, convert, fromBlocks, insert, insertAt, merge, move, numberToken, opensMenu, remove, split, toBlocks, toggleMark } from '../src/lib/texteditor.ts';
import { parseMarkdown, toMarkdown } from '../src/lib/markdown.ts';

const TEXTS = [
  '# Title\n\nA paragraph with **bold**, *italic* and a [link](https://example.com/).\n\n- a\n- b\n\n3. c\n4. d\n\n> quoted\n> twice\n\n> [!warn]\n> Careful {{measure:m1|compare}}\n\n---\n\n### Small\n\nSee [Sales](page:d2).',
  'Two\nlines',
  '\\# kept\n\n1\\. kept\n\n\\- kept\n\n\\> kept',
  '> a\n>\n> b',
];

test('a text becomes blocks and the blocks give the same text', () => {
  const b = toBlocks(TEXTS[0]);
  assert.deepEqual(b.map((x) => x.kind), ['h1', 'p', 'ul', 'ol', 'quote', 'warn', 'hr', 'h3', 'p']);
  assert.equal(b[2].text, 'a\nb');
  assert.equal(b[3].start, 3);
  assert.equal(b[5].text, 'Careful {{measure:m1|compare}}');
  for (const t of TEXTS) {
    assert.equal(fromBlocks(toBlocks(t)), toMarkdown(parseMarkdown(t)), t);
    assert.equal(fromBlocks(toBlocks(t)), t, t);
  }
  // Words typed in a block stay in it: a paragraph that starts like a heading is still a paragraph.
  const typed = fromBlocks([{ kind: 'p', text: '# not a heading\n1. nor a list' }, { kind: 'h2', text: 'Two\nlines' }, { kind: 'ul', text: 'x\n\ny' }, { kind: 'p', text: '' }]);
  assert.equal(typed, '\\# not a heading\n1\\. nor a list\n\n## Two lines\n\n- x\n- y');
  // The editor shows the words as typed; the backslashes come back when it saves.
  assert.deepEqual(toBlocks(typed).map((x) => [x.kind, x.text]), [['p', '# not a heading\n1. nor a list'], ['h2', 'Two lines'], ['ul', 'x\ny']]);
  assert.equal(fromBlocks(toBlocks(typed)), typed);
});

test('blocks change kind, move, split and join', () => {
  const b = [{ kind: 'p', text: 'one\ntwo' }, { kind: 'h1', text: 'Head' }, { kind: 'hr', text: '' }];
  assert.deepEqual(convert(b, 0, 'h2')[0], { kind: 'h2', text: 'one two' });
  assert.deepEqual(convert(b, 0, 'ol')[0], { kind: 'ol', text: 'one\ntwo', start: 1 });
  assert.deepEqual(convert(b, 0, 'hr')[0], { kind: 'hr', text: '' });
  assert.deepEqual(move(b, 0, 2).map((x) => x.kind), ['h1', 'hr', 'p']);
  assert.deepEqual(move(b, 0, 9).map((x) => x.kind), ['p', 'h1', 'hr']);
  assert.deepEqual(insert(b, 1, 'note')[1], { kind: 'note', text: '' });
  assert.deepEqual(remove(b, 1).map((x) => x.kind), ['p', 'hr']);
  // Enter in a heading at "He|ad": the rest goes to a paragraph under it.
  assert.deepEqual(split(b, 1, 2).slice(1, 3), [{ kind: 'h1', text: 'He' }, { kind: 'p', text: 'ad' }]);
  // Backspace at the start of the heading: it joins the paragraph above, on a new line; the caret sits where it began.
  assert.deepEqual(merge(b, 1), { blocks: [{ kind: 'p', text: 'one\ntwo\nHead' }, { kind: 'hr', text: '' }], caret: 8 });
  // Joining into a heading keeps one line; a line across above is just removed.
  assert.deepEqual(merge([{ kind: 'h1', text: 'A' }, { kind: 'p', text: 'b\nc' }], 1).blocks, [{ kind: 'h1', text: 'Ab c' }]);
  // Backspace on a line across takes it away.
  assert.deepEqual(merge(b, 2).blocks.map((x) => x.kind), ['p', 'h1']);
  assert.deepEqual(merge([{ kind: 'hr', text: '' }, { kind: 'p', text: 'x' }], 1).blocks, [{ kind: 'p', text: 'x' }]);
});

test('bold, italic, links and numbers go in around the selection, and come out again', () => {
  // "make it bold": select "it" (5 to 7).
  const on = toggleMark('make it bold', 5, 7, '**');
  assert.deepEqual(on, { text: 'make **it** bold', start: 7, end: 9 });
  assert.deepEqual(toggleMark(on.text, on.start, on.end, '**'), { text: 'make it bold', start: 5, end: 7 });
  assert.deepEqual(toggleMark('make **it** bold', 5, 11, '**'), { text: 'make it bold', start: 5, end: 7 });
  // Italic inside bold is not bold taken away.
  assert.deepEqual(toggleMark('**it**', 2, 4, '*'), { text: '***it***', start: 3, end: 5 });
  // A selection that holds bold's marks gets italic around them; it does not lose a star of the bold.
  assert.deepEqual(toggleMark('**it**', 0, 6, '*'), { text: '***it***', start: 1, end: 7 });
  assert.deepEqual(toggleMark('ab', 1, 1, '*'), { text: 'a**b', start: 2, end: 2 });
  assert.deepEqual(addLink('see docs', 4, 8, 'https://example.com', 'x'), { text: 'see [docs](https://example.com)', start: 31, end: 31 });
  assert.deepEqual(addLink('go ', 3, 3, 'page:d2', 'Sales [Q3]'), { text: 'go [Sales \\[Q3\\]](page:d2)', start: 26, end: 26 });
  assert.equal(numberToken('m1', 'value'), '{{measure:m1}}');
  assert.equal(numberToken('m1', 'compare'), '{{measure:m1|compare}}');
  assert.deepEqual(insertAt('ab', 1, 1, numberToken('m2', 'period')), { text: 'a{{measure:m2|period}}b', start: 22, end: 22 });
  assert.equal(opensMenu({ kind: 'p', text: '/' }), true);
  assert.equal(opensMenu({ kind: 'p', text: '/x' }), false);
  // What the editor writes reads back as the marks it means.
  const [p] = parseMarkdown(fromBlocks([{ kind: 'p', text: addLink('go ', 3, 3, 'page:d2', 'Sales [Q3]').text }]));
  assert.deepEqual(p.children[1], { t: 'page', page: 'd2', text: 'Sales [Q3]' });
});
