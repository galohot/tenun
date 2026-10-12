// The Markdown subset of text tiles (build 05, and build 10's blocks, page links and live numbers): what it understands,
// what it never lets through, and that writing a tree back gives the same text.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { liveNumbers, parseMarkdown, safeUrl, toMarkdown } from '../src/lib/markdown.ts';

const text = (nodes) => nodes.map((n) => (n.t === 'text' ? n.text : n.t === 'br' ? '\n' : n.t === 'a' ? n.text : text(n.children))).join('');

test('headings, paragraphs and lists', () => {
  const b = parseMarkdown('# Sales\n\n## This month\nLine one\nLine two\n\n- first\n- second\n\nDone');
  assert.deepEqual(b.map((x) => x.t), ['h1', 'h2', 'p', 'ul', 'p']);
  assert.equal(text(b[0].children), 'Sales');
  assert.equal(text(b[2].children), 'Line one\nLine two');
  assert.deepEqual(b[3].items.map(text), ['first', 'second']);
});

test('bold, italic and links', () => {
  const [p] = parseMarkdown('A **bold** and *italic* word, see [the docs](https://example.com/a?b=1).');
  const kinds = p.children.map((n) => n.t);
  assert.deepEqual(kinds, ['text', 'b', 'text', 'i', 'text', 'a', 'text']);
  const link = p.children.find((n) => n.t === 'a');
  assert.equal(link.href, 'https://example.com/a?b=1');
  assert.equal(link.text, 'the docs');
  // Bold may hold italic.
  const [q] = parseMarkdown('**very *good* news**');
  assert.equal(q.children[0].t, 'b');
  assert.deepEqual(q.children[0].children.map((n) => n.t), ['text', 'i', 'text']);
});

test('hostile input stays plain text', () => {
  const evil = [
    '<script>alert(1)</script>',
    '<img src=x onerror=alert(1)>',
    '[click](javascript:alert(1))',
    '[click](JaVaScRiPt:alert(1))',
    '[click](data:text/html;base64,PHNjcmlwdD4=)',
    '[click](vbscript:msgbox(1))',
    '[click](//evil.example/x)',
    '[click](ftp://example.com/file)',
    '[click](https://exa mple.com)',
    '[a](https://example.com/"onmouseover="alert(1))',
    '# <b>x</b>',
    '- <iframe src=//evil></iframe>',
  ].join('\n\n');
  const walk = (nodes, f) => nodes.forEach((n) => (f(n), n.children && walk(n.children, f)));
  const blocks = parseMarkdown(evil);
  let links = 0;
  let raw = '';
  for (const b of blocks) {
    const lists = b.t === 'ul' || b.t === 'ol' ? b.items : b.t === 'hr' ? [] : [b.children];
    for (const nodes of lists) {
      walk(nodes, (n) => {
        if (n.t === 'a') {
          links++;
          assert.match(n.href, /^https?:\/\//);
          assert.ok(!/["'<>\s]/.test(n.href), n.href);
        }
        if (n.t === 'text') raw += n.text;
      });
    }
  }
  // Nothing became an HTML tag: the markup is still there as characters, for the tile to draw as text.
  assert.ok(raw.includes('<script>alert(1)</script>'));
  assert.ok(raw.includes('[click](javascript:alert(1))'));
  assert.ok(raw.includes('<img src=x onerror=alert(1)>'));
  // Only the quote-carrying address could pass as a link, and it was cut to a safe one or refused.
  assert.ok(links <= 1, `links: ${links}`);
  // No node type outside the known set.
  const known = new Set(['text', 'b', 'i', 'a', 'page', 'num', 'br']);
  for (const b of blocks) {
    assert.ok(['h1', 'h2', 'h3', 'p', 'ul', 'ol', 'quote', 'callout', 'hr'].includes(b.t));
    for (const nodes of b.t === 'ul' || b.t === 'ol' ? b.items : b.t === 'hr' ? [] : [b.children]) walk(nodes, (n) => assert.ok(known.has(n.t), n.t));
  }
});

test('which addresses may be linked', () => {
  assert.equal(safeUrl('https://example.com'), 'https://example.com/');
  assert.equal(safeUrl('http://example.com/a'), 'http://example.com/a');
  for (const bad of ['javascript:alert(1)', 'data:text/html,x', 'file:///etc/passwd', '//example.com', 'https://', 'https://a b', 'mailto:a@b.c', '']) {
    assert.equal(safeUrl(bad), null, bad);
  }
});

test('unknown markup is text, and an empty note has no blocks', () => {
  const [p] = parseMarkdown('#### four hashes\n1) numbered\n{{measure:}} {{metric:m1}} {{measure:m1|total}}');
  assert.equal(p.t, 'p');
  assert.equal(text(p.children), '#### four hashes\n1) numbered\n{{measure:}} {{metric:m1}} {{measure:m1|total}}');
  assert.deepEqual(parseMarkdown(''), []);
  assert.deepEqual(parseMarkdown('  \n\n  '), []);
  // A star that does not open anything stays a star.
  assert.equal(text(parseMarkdown('2 * 3 * 4')[0].children), '2 * 3 * 4');
});

test('the blocks of build 10: a third heading, numbered lists, quotes, callouts and a line across', () => {
  const b = parseMarkdown('### Small\n\n3. three\n4. four\n- dot\n\n> said\n> twice\n\n> [!warn]\n> Mind the gap\n\n> [!good] Done\n\n---\nafter');
  assert.deepEqual(b.map((x) => x.t), ['h3', 'ol', 'ul', 'quote', 'callout', 'callout', 'hr', 'p']);
  assert.equal(b[1].start, 3);
  assert.deepEqual(b[1].items.map(text), ['three', 'four']);
  assert.equal(text(b[3].children), 'said\ntwice');
  assert.deepEqual([b[4].kind, text(b[4].children)], ['warn', 'Mind the gap']);
  assert.deepEqual([b[5].kind, text(b[5].children)], ['good', 'Done']);
  // A callout word in any case; an unknown one is a quote.
  assert.equal(parseMarkdown('> [!NOTE] x')[0].kind, 'note');
  assert.equal(parseMarkdown('> [!tip] x')[0].t, 'quote');
});

test('page links and live numbers', () => {
  const [p] = parseMarkdown('See [Sales](page:d-sales_2) for {{measure:m3}} ({{measure:m3|compare}} in {{measure:m3|period}}).');
  assert.deepEqual(p.children.map((n) => n.t), ['text', 'page', 'text', 'num', 'text', 'num', 'text', 'num', 'text']);
  assert.deepEqual(p.children[1], { t: 'page', page: 'd-sales_2', text: 'Sales' });
  assert.deepEqual(p.children.filter((n) => n.t === 'num').map((n) => n.part), ['value', 'compare', 'period']);
  // A page id with anything else in it is text; a live number inside bold counts.
  assert.equal(parseMarkdown('[x](page:a/b)')[0].children[0].t, 'text');
  assert.deepEqual(liveNumbers('**{{measure:m1}}** and {{measure:m1}} and {{measure:m2|period}}\n- {{measure:m1|compare}}'), [
    { id: 'm1', part: 'value' },
    { id: 'm2', part: 'period' },
    { id: 'm1', part: 'compare' },
  ]);
});

test('a backslash writes a mark as itself', () => {
  const [p] = parseMarkdown('2\\*3\\*4 and \\[not](https://example.com) and \\{{measure:m1}}');
  assert.equal(p.children.length, 1);
  assert.equal(text(p.children), '2*3*4 and [not](https://example.com) and {{measure:m1}}');
  assert.equal(parseMarkdown('\\# not a heading')[0].t, 'p');
  assert.equal(parseMarkdown('1\\. not a list')[0].t, 'p');
  assert.equal(text(parseMarkdown('1\\. not a list')[0].children), '1. not a list');
});

test('nesting stops at four levels', () => {
  const deep = '**a *b **c *d **e** d* c** b* a**';
  const depth = (nodes) => Math.max(0, ...nodes.map((n) => (n.children ? 1 + depth(n.children) : 0)));
  assert.ok(depth(parseMarkdown(deep)[0].children) <= 4);
});

// Thirty texts that are written the way the writer writes them: reading and writing back gives each one again.
const CORPUS = [
  '# Sales',
  '## This month',
  '### Notes',
  'One line.',
  'Two\nlines.',
  'A **bold** and *italic* word.',
  '**Bold with *italic* inside**',
  'A [link](https://example.com/a?b=1) here.',
  'A [page](page:d1) link.',
  'Revenue is {{measure:m1}}.',
  'Change {{measure:m1|compare}} in {{measure:m1|period}}.',
  '- one\n- two\n- three',
  '1. one\n2. two',
  '7. seven\n8. eight',
  '> A quote',
  '> Two\n> lines',
  '> [!note]\n> Read this.',
  '> [!good]\n> All good.',
  '> [!warn]\n> Careful **now**.',
  '---',
  '# Title\n\nA paragraph.\n\n- a\n- b\n\n---\n\n> [!note]\n> End.',
  '2 \\* 3 = 6',
  '\\# not a heading',
  '\\- not a list',
  '1\\. not a list',
  '\\> not a quote',
  'Braces \\{\\{ stay',
  'A \\[bracket\\]',
  'Back\\\\slash',
  '> [!note]\n> {{measure:m2}} so far',
];

test('thirty texts read and write back to themselves, and every tree writes back to itself', () => {
  assert.equal(CORPUS.length, 30);
  for (const t of CORPUS) {
    assert.equal(toMarkdown(parseMarkdown(t)), t, t);
    assert.deepEqual(parseMarkdown(toMarkdown(parseMarkdown(t))), parseMarkdown(t), t);
  }
  // A tree made by hand, with text that looks like marks, writes so it reads back the same.
  const tree = [{ t: 'p', children: [{ t: 'text', text: '# 2*3 [x](y) {{measure:m1}}' }] }, { t: 'ul', items: [[{ t: 'text', text: '> quoted?' }]] }];
  assert.deepEqual(parseMarkdown(toMarkdown(tree)), tree);
});
