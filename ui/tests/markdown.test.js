// The Markdown subset of text tiles (build 05): what it understands, and what it never lets through.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseMarkdown, safeUrl } from '../src/lib/markdown.ts';

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
    const lists = b.t === 'ul' ? b.items : [b.children];
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
  const known = new Set(['text', 'b', 'i', 'a', 'br']);
  for (const b of blocks) {
    assert.ok(['h1', 'h2', 'p', 'ul'].includes(b.t));
    for (const nodes of b.t === 'ul' ? b.items : [b.children]) walk(nodes, (n) => assert.ok(known.has(n.t), n.t));
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
  const [p] = parseMarkdown('### three hashes\n1. numbered');
  assert.equal(p.t, 'p');
  assert.equal(text(p.children), '### three hashes\n1. numbered');
  assert.deepEqual(parseMarkdown(''), []);
  assert.deepEqual(parseMarkdown('  \n\n  '), []);
  // A star that does not open anything stays a star.
  assert.equal(text(parseMarkdown('2 * 3 * 4')[0].children), '2 * 3 * 4');
});
