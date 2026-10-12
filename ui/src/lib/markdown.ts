// A small Markdown subset for text tiles. It returns a tree of plain data, and the tile draws that tree with text
// nodes. No HTML is ever built from the text, so nothing in it can run.
//
//   # Heading   ## Smaller   ### Smallest      a paragraph (a line break inside it stays a line break)
//   - a list item      1. a numbered item      > a quote      ---  (a line across)
//   > [!note]  > [!good]  > [!warn]   a callout: a tinted block with an icon and a word
//   **bold**   *italic*   [a link](https://example.com)   [a page](page:<dashboard id>)
//   {{measure:<id>}}  {{measure:<id>|compare}}  {{measure:<id>|period}}   a live number (build 10)
//   \*  a mark written as itself
//
// Only http and https links work, and page links to a dashboard id. Everything else, including `<script>` and
// `javascript:` links, is shown as the text it is. Pure code with no imports, so the node tests can load it.

export type NumPart = 'value' | 'compare' | 'period';

export type Inline =
  | { t: 'text'; text: string }
  | { t: 'b'; children: Inline[] }
  | { t: 'i'; children: Inline[] }
  | { t: 'a'; href: string; text: string }
  /** A link to another dashboard of the workspace. */
  | { t: 'page'; page: string; text: string }
  /** A live number: a named measure's value, its change, or its period. */
  | { t: 'num'; id: string; part: NumPart }
  | { t: 'br' };

export type CalloutKind = 'note' | 'good' | 'warn';

export type Block =
  | { t: 'h1' | 'h2' | 'h3' | 'p' | 'quote'; children: Inline[] }
  | { t: 'callout'; kind: CalloutKind; children: Inline[] }
  | { t: 'ul'; items: Inline[][] }
  | { t: 'ol'; start: number; items: Inline[][] }
  | { t: 'hr' };

/** An address a link may point to: http or https, with a host, and no spaces or quotes. */
export function safeUrl(raw: string): string | null {
  if (!/^https?:\/\/[^\s"'<>`]+$/i.test(raw)) return null;
  try {
    const u = new URL(raw);
    return (u.protocol === 'http:' || u.protocol === 'https:') && u.hostname ? u.href : null;
  } catch {
    return null;
  }
}

/** A dashboard id in a page link: letters, digits, `-` and `_`. */
const PAGE_ID = /^[A-Za-z0-9_-]{1,64}$/;
/** A named measure id in a live number. */
const MEASURE_ID = /^[A-Za-z0-9_-]{1,64}$/;

const LINK = /\[((?:\\.|[^\]\\\n])+)\]\(([^)\s]+)\)/;
const BOLD = /\*\*(?=\S)((?:\\.|[^\\])+?)(?<=\S)\*\*/;
const ITALIC = /(?<!\\)\*(?=[^\s*])((?:\\.|[^\\*])+?)(?<=[^\s*\\])\*/;
const NUM = /\{\{measure:([^|}\s]+)(?:\|(compare|period))?\}\}/;
/** A mark written as itself: a backslash before one of these. */
const ESCAPED = /\\([\\*[\]{}()#>\-.!_`|])/g;
const MAX_DEPTH = 4;

const unescape = (s: string) => s.replace(ESCAPED, '$1');

function inline(src: string, depth = 0): Inline[] {
  const out: Inline[] = [];
  let rest = src;
  const push = (text: string) => {
    if (!text) return;
    const last = out[out.length - 1];
    if (last?.t === 'text') last.text += unescape(text);
    else out.push({ t: 'text', text: unescape(text) });
  };
  while (rest) {
    // The earliest mark wins. A mark after a backslash is not one.
    let best: { at: number; len: number; node: Inline } | null = null;
    const consider = (re: RegExp, node: (m: RegExpExecArray) => Inline | null) => {
      const m = re.exec(rest);
      if (!m || (best && m.index >= best.at)) return;
      if (escapedAt(rest, m.index)) return;
      const n = node(m);
      if (n) best = { at: m.index, len: m[0].length, node: n };
    };
    consider(LINK, (m) => {
      const text = unescape(m[1]);
      if (m[2].startsWith('page:')) {
        const page = m[2].slice(5);
        return PAGE_ID.test(page) ? { t: 'page', page, text } : null;
      }
      const href = safeUrl(m[2]);
      return href ? { t: 'a', href, text } : null;
    });
    consider(NUM, (m) => (MEASURE_ID.test(m[1]) ? { t: 'num', id: m[1], part: (m[2] as NumPart | undefined) ?? 'value' } : null));
    if (depth < MAX_DEPTH) {
      consider(BOLD, (m) => ({ t: 'b', children: inline(m[1], depth + 1) }));
      consider(ITALIC, (m) => ({ t: 'i', children: inline(m[1], depth + 1) }));
    }
    if (!best) {
      push(rest);
      break;
    }
    const hit = best as { at: number; len: number; node: Inline };
    push(rest.slice(0, hit.at));
    out.push(hit.node);
    rest = rest.slice(hit.at + hit.len);
  }
  return out;
}

/** True when the character at `i` follows an odd number of backslashes. */
function escapedAt(s: string, i: number): boolean {
  let n = 0;
  for (let j = i - 1; j >= 0 && s[j] === '\\'; j--) n++;
  return n % 2 === 1;
}

/** Text with line breaks between its lines. */
function lines(parts: string[]): Inline[] {
  const out: Inline[] = [];
  parts.forEach((p, i) => {
    if (i) out.push({ t: 'br' });
    out.push(...inline(p));
  });
  return out;
}

const CALLOUT = /^\[!(note|good|warn)\]\s*(.*)$/i;

export function parseMarkdown(src: string): Block[] {
  const blocks: Block[] = [];
  let para: string[] = [];
  let quote: string[] | null = null;
  let list: { t: 'ul' | 'ol'; items: Inline[][] } | null = null;
  const flushPara = () => {
    if (para.length) blocks.push({ t: 'p', children: lines(para) });
    para = [];
  };
  const flushQuote = () => {
    if (!quote) return;
    const first = CALLOUT.exec(quote[0] ?? '');
    if (first) {
      const body = [first[2], ...quote.slice(1)].filter((l, i) => i > 0 || l.trim());
      blocks.push({ t: 'callout', kind: first[1].toLowerCase() as CalloutKind, children: lines(body) });
    } else blocks.push({ t: 'quote', children: lines(quote) });
    quote = null;
  };
  const flush = () => {
    flushPara();
    flushQuote();
    list = null;
  };
  for (const raw of src.replace(/\r\n?/g, '\n').split('\n')) {
    const line = raw.trimEnd();
    if (!line.trim()) {
      flush();
      continue;
    }
    const q = /^> ?(.*)$/.exec(line);
    if (q) {
      if (!quote) {
        flushPara();
        list = null;
        quote = [];
      }
      quote.push(q[1]);
      continue;
    }
    if (quote) flushQuote();
    if (/^-{3,}$/.test(line.trim())) {
      flush();
      blocks.push({ t: 'hr' });
      continue;
    }
    const head = /^(#{1,3}) +(.*\S)\s*$/.exec(line);
    if (head) {
      flush();
      blocks.push({ t: (['h1', 'h2', 'h3'] as const)[head[1].length - 1], children: inline(head[2]) });
      continue;
    }
    const item = /^- +(.*\S)\s*$/.exec(line);
    const num = /^(\d{1,9})\. +(.*\S)\s*$/.exec(line);
    if (item || num) {
      flushPara();
      const t = item ? 'ul' : 'ol';
      if (!list || list.t !== t) {
        list = { t, items: [] };
        blocks.push(t === 'ul' ? { t, items: list.items } : { t, start: Number(num![1]), items: list.items });
      }
      list.items.push(inline(item ? item[1] : num![2]));
      continue;
    }
    // A plain line after a list starts a paragraph.
    if (list) list = null;
    para.push(line);
  }
  flush();
  return blocks;
}

// ---- back to text ---------------------------------------------------------------------------------------------------

/** Text written so it reads back as itself: every mark character gets a backslash. */
export function escapeText(s: string): string {
  return s.replace(/[\\*[\]{}]/g, (c) => `\\${c}`);
}

/** A line written so it does not start a heading, a list, a quote or a line across. */
function escapeStart(line: string): string {
  // "1. " is kept from a numbered list by its dot: "1\. ".
  const num = /^(\d+)\. /.exec(line);
  if (num) return `${num[1]}\\.${line.slice(num[1].length + 1)}`;
  return /^(#|- |>|-{3,}$)/.test(line) ? `\\${line}` : line;
}

export function inlineToMarkdown(nodes: readonly Inline[]): string {
  return nodes
    .map((n) => {
      switch (n.t) {
        case 'text':
          return escapeText(n.text);
        case 'b':
          return `**${inlineToMarkdown(n.children)}**`;
        case 'i':
          return `*${inlineToMarkdown(n.children)}*`;
        case 'a':
          return `[${escapeText(n.text)}](${n.href})`;
        case 'page':
          return `[${escapeText(n.text)}](page:${n.page})`;
        case 'num':
          return `{{measure:${n.id}${n.part === 'value' ? '' : `|${n.part}`}}}`;
        case 'br':
          return '\n';
      }
    })
    .join('');
}

/** The lines of a run of inline nodes, each kept from starting a block. */
const bodyLines = (nodes: readonly Inline[]) => inlineToMarkdown(nodes).split('\n').map(escapeStart);

/** The tree written back as the Markdown subset: `parseMarkdown(toMarkdown(b))` gives `b` again. */
export function toMarkdown(blocks: readonly Block[]): string {
  return blocks
    .map((b) => {
      switch (b.t) {
        case 'h1':
        case 'h2':
        case 'h3':
          return `${'#'.repeat(Number(b.t[1]))} ${inlineToMarkdown(b.children)}`;
        case 'p':
          return bodyLines(b.children).join('\n');
        case 'quote':
          return bodyLines(b.children)
            .map((l) => `> ${l}`.trimEnd())
            .join('\n');
        case 'callout':
          return [`> [!${b.kind}]`, ...bodyLines(b.children).map((l) => `> ${l}`.trimEnd())].join('\n');
        case 'ul':
          return b.items.map((it) => `- ${inlineToMarkdown(it)}`).join('\n');
        case 'ol':
          return b.items.map((it, k) => `${b.start + k}. ${inlineToMarkdown(it)}`).join('\n');
        case 'hr':
          return '---';
      }
    })
    .join('\n\n');
}

/** The live numbers a text names, each once: what the engine works out for the tile. */
export function liveNumbers(src: string): { id: string; part: NumPart }[] {
  const seen = new Set<string>();
  const out: { id: string; part: NumPart }[] = [];
  const walk = (nodes: readonly Inline[]) => {
    for (const n of nodes) {
      if (n.t === 'num' && !seen.has(`${n.id}|${n.part}`)) {
        seen.add(`${n.id}|${n.part}`);
        out.push({ id: n.id, part: n.part });
      } else if (n.t === 'b' || n.t === 'i') walk(n.children);
    }
  };
  for (const b of parseMarkdown(src)) {
    if ('children' in b) walk(b.children);
    else if ('items' in b) for (const it of b.items) walk(it);
  }
  return out;
}
