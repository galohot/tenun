// A tiny Markdown subset for text tiles. It returns a tree of plain data, and the tile draws that
// tree with text nodes. No HTML is ever built from the text, so nothing in it can run.
//
//   # Heading        ## Smaller heading
//   a paragraph      (a line break inside a paragraph stays a line break)
//   - a list item
//   **bold**   *italic*   [a link](https://example.com)
//
// Only http and https links work. Everything else, including `<script>` and `javascript:` links,
// is shown as the text it is. Pure code with no imports, so the node tests can load it.

export type Inline =
  | { t: 'text'; text: string }
  | { t: 'b'; children: Inline[] }
  | { t: 'i'; children: Inline[] }
  | { t: 'a'; href: string; text: string }
  | { t: 'br' };

export type Block =
  | { t: 'h1' | 'h2' | 'p'; children: Inline[] }
  | { t: 'ul'; items: Inline[][] };

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

const LINK = /\[([^\]\n]+)\]\(([^)\s]+)\)/;
const BOLD = /\*\*(?=\S)(.+?)(?<=\S)\*\*/;
const ITALIC = /\*(?=[^\s*])(.+?)(?<=[^\s*])\*/;
const MAX_DEPTH = 4;

function inline(src: string, depth = 0): Inline[] {
  const out: Inline[] = [];
  let rest = src;
  const push = (text: string) => text && out.push({ t: 'text', text });
  while (rest) {
    // The earliest of the three marks wins.
    let best: { at: number; len: number; node: Inline } | null = null;
    const consider = (m: RegExpExecArray | null, node: (m: RegExpExecArray) => Inline | null) => {
      if (!m || (best && m.index >= best.at)) return;
      const n = node(m);
      if (n) best = { at: m.index, len: m[0].length, node: n };
    };
    consider(LINK.exec(rest), (m) => {
      const href = safeUrl(m[2]);
      return href ? { t: 'a', href, text: m[1] } : null;
    });
    if (depth < MAX_DEPTH) {
      consider(BOLD.exec(rest), (m) => ({ t: 'b', children: inline(m[1], depth + 1) }));
      consider(ITALIC.exec(rest), (m) => ({ t: 'i', children: inline(m[1], depth + 1) }));
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

/** Text with line breaks between its lines. */
function lines(parts: string[]): Inline[] {
  const out: Inline[] = [];
  parts.forEach((p, i) => {
    if (i) out.push({ t: 'br' });
    out.push(...inline(p));
  });
  return out;
}

export function parseMarkdown(src: string): Block[] {
  const blocks: Block[] = [];
  let para: string[] = [];
  let list: Inline[][] | null = null;
  const flush = () => {
    if (para.length) blocks.push({ t: 'p', children: lines(para) });
    para = [];
    list = null;
  };
  for (const raw of src.replace(/\r\n?/g, '\n').split('\n')) {
    const line = raw.trimEnd();
    if (!line.trim()) {
      flush();
      continue;
    }
    const head = /^(#{1,2}) +(.*\S)\s*$/.exec(line);
    if (head) {
      flush();
      blocks.push({ t: head[1].length === 1 ? 'h1' : 'h2', children: inline(head[2]) });
      continue;
    }
    const item = /^[-] +(.*\S)\s*$/.exec(line);
    if (item) {
      if (para.length) flush();
      if (!list) {
        list = [];
        blocks.push({ t: 'ul', items: list });
      }
      list.push(inline(item[1]));
      continue;
    }
    // A plain line after a list starts a paragraph.
    if (list) list = null;
    para.push(line);
  }
  flush();
  return blocks;
}
