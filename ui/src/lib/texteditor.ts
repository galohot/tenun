// The block editor of a text tile (build 10): the text as a list of blocks, each with its kind and its words, and the
// edits on them. A block's words keep the inline marks (**bold**, *italic*, links, live numbers) as they are written,
// so what is typed is what is saved. Pure, with only pure imports, so the node tests load it.

import { inlineToMarkdown, parseMarkdown, type Block, type CalloutKind, type NumPart } from './markdown.ts';

export type EdKind = 'p' | 'h1' | 'h2' | 'h3' | 'ul' | 'ol' | 'quote' | 'note' | 'good' | 'warn' | 'hr';

export interface EdBlock {
  kind: EdKind;
  /** The words, with their marks. A list has one item on each line; a paragraph, a quote or a callout one line each. */
  text: string;
  /** A numbered list's first number. */
  start?: number;
}

/** The names of the kinds, in the order the block menu lists them. */
export const KINDS: { kind: EdKind; label: string }[] = [
  { kind: 'p', label: 'Text' },
  { kind: 'h1', label: 'Heading 1' },
  { kind: 'h2', label: 'Heading 2' },
  { kind: 'h3', label: 'Heading 3' },
  { kind: 'ul', label: 'Bulleted list' },
  { kind: 'ol', label: 'Numbered list' },
  { kind: 'quote', label: 'Quote' },
  { kind: 'note', label: 'Note' },
  { kind: 'good', label: 'Good news' },
  { kind: 'warn', label: 'Warning' },
  { kind: 'hr', label: 'Line' },
];

/** Kinds that hold one line: Enter starts a new block. */
export const ONE_LINE: readonly EdKind[] = ['h1', 'h2', 'h3'];

function fromTree(b: Block): EdBlock {
  switch (b.t) {
    case 'ul':
      return { kind: 'ul', text: b.items.map(inlineToMarkdown).join('\n') };
    case 'ol':
      return { kind: 'ol', text: b.items.map(inlineToMarkdown).join('\n'), start: b.start };
    case 'hr':
      return { kind: 'hr', text: '' };
    case 'callout':
      return { kind: b.kind, text: inlineToMarkdown(b.children) };
    default:
      return { kind: b.t, text: inlineToMarkdown(b.children) };
  }
}

/** The blocks of a text. */
export const toBlocks = (md: string): EdBlock[] => parseMarkdown(md).map(fromTree);

/** A line kept from starting a block of its own: what `toMarkdown` does to the lines of a paragraph. */
function keepLine(line: string): string {
  const num = /^(\d+)\. /.exec(line);
  if (num) return `${num[1]}\\.${line.slice(num[1].length + 1)}`;
  return /^(#|- |>|-{3,}$)/.test(line) ? `\\${line}` : line;
}

/** The text of the blocks: the Markdown subset, as `toMarkdown` writes it. Empty blocks are left out. */
export function fromBlocks(blocks: readonly EdBlock[]): string {
  const out: string[] = [];
  for (const b of blocks) {
    const ls = b.text.split('\n').map((l) => l.trimEnd());
    const words = ls.filter((l) => l.trim());
    if (b.kind === 'hr') {
      out.push('---');
      continue;
    }
    if (!words.length) continue;
    switch (b.kind) {
      case 'h1':
      case 'h2':
      case 'h3':
        out.push(`${'#'.repeat(Number(b.kind[1]))} ${words.join(' ').trim()}`);
        break;
      case 'ul':
        out.push(words.map((l) => `- ${l.trim()}`).join('\n'));
        break;
      case 'ol':
        out.push(words.map((l, k) => `${(b.start ?? 1) + k}. ${l.trim()}`).join('\n'));
        break;
      case 'quote':
        out.push(trimEnds(ls).map((l) => `> ${keepLine(l)}`.trimEnd()).join('\n'));
        break;
      case 'note':
      case 'good':
      case 'warn':
        out.push([`> [!${b.kind}]`, ...trimEnds(ls).map((l) => `> ${keepLine(l)}`.trimEnd())].join('\n'));
        break;
      default:
        // An empty line inside a paragraph would end it: the paragraph splits there.
        out.push(
          ls
            .map((l) => (l.trim() ? keepLine(l) : ''))
            .join('\n')
            .replace(/\n{2,}/g, '\n\n')
            .trim(),
        );
    }
  }
  return out.join('\n\n');
}

/** Lines without the empty ones at the start and the end. */
function trimEnds(ls: string[]): string[] {
  let a = 0;
  let z = ls.length;
  while (a < z && !ls[a].trim()) a++;
  while (z > a && !ls[z - 1].trim()) z--;
  return ls.slice(a, z);
}

// ---- edits on the blocks --------------------------------------------------------------------------------------------

/** A block of another kind, with the same words. A heading takes one line; a line across keeps no words. */
export function convert(blocks: readonly EdBlock[], i: number, kind: EdKind): EdBlock[] {
  return blocks.map((b, j) => {
    if (j !== i) return b;
    if (kind === 'hr') return { kind, text: '' };
    const text = ONE_LINE.includes(kind) ? b.text.split('\n').map((l) => l.trim()).filter(Boolean).join(' ') : b.text;
    return kind === 'ol' ? { kind, text, start: b.start ?? 1 } : { kind, text };
  });
}

/** A new empty block at `at`. */
export const insert = (blocks: readonly EdBlock[], at: number, kind: EdKind = 'p'): EdBlock[] => [
  ...blocks.slice(0, at),
  { kind, text: '' },
  ...blocks.slice(at),
];

export const remove = (blocks: readonly EdBlock[], i: number): EdBlock[] => blocks.filter((_, j) => j !== i);

/** A block moved to another place. */
export function move(blocks: readonly EdBlock[], from: number, to: number): EdBlock[] {
  if (from === to || to < 0 || to >= blocks.length) return [...blocks];
  const list = [...blocks];
  const [b] = list.splice(from, 1);
  list.splice(to, 0, b);
  return list;
}

/** Enter in a heading: the words after the caret go to a new paragraph under it. */
export function split(blocks: readonly EdBlock[], i: number, caret: number): EdBlock[] {
  const b = blocks[i];
  return [...blocks.slice(0, i), { ...b, text: b.text.slice(0, caret).trimEnd() }, { kind: 'p', text: b.text.slice(caret).trimStart() }, ...blocks.slice(i + 1)];
}

/** Backspace at the start of a block: its words join the block above. Gives the caret's place in the joined block. */
export function merge(blocks: readonly EdBlock[], i: number): { blocks: EdBlock[]; caret: number } {
  if (i <= 0) return { blocks: [...blocks], caret: 0 };
  const a = blocks[i - 1];
  const b = blocks[i];
  if (a.kind === 'hr') return { blocks: remove(blocks, i - 1), caret: 0 };
  const glue = a.text && b.text && !ONE_LINE.includes(a.kind) ? '\n' : '';
  const text = ONE_LINE.includes(a.kind) ? `${a.text}${b.text.replace(/\n/g, ' ')}` : `${a.text}${glue}${b.text}`;
  return { blocks: [...blocks.slice(0, i - 1), { ...a, text }, ...blocks.slice(i + 1)], caret: a.text.length + glue.length };
}

// ---- inline marks ---------------------------------------------------------------------------------------------------

export interface Edit {
  text: string;
  start: number;
  end: number;
}

/**
 * Bold (`**`) or italic (`*`) on the selection: a selection that is already wrapped (inside or just outside) loses the
 * mark; any other gets it. With nothing selected, the marks go in with the caret between them.
 */
export function toggleMark(text: string, start: number, end: number, mark: '**' | '*'): Edit {
  const n = mark.length;
  const inside = text.slice(start, end);
  const isOther = (s: string) => mark === '*' && s.startsWith('**') && !s.startsWith('***');
  if (inside.length >= 2 * n && inside.startsWith(mark) && inside.endsWith(mark) && !isOther(inside)) {
    return { text: text.slice(0, start) + inside.slice(n, -n) + text.slice(end), start, end: end - 2 * n };
  }
  if (text.slice(start - n, start) === mark && text.slice(end, end + n) === mark && !(mark === '*' && (text[start - 2] === '*' || text[end + 1] === '*'))) {
    return { text: text.slice(0, start - n) + inside + text.slice(end + n), start: start - n, end: end - n };
  }
  return { text: text.slice(0, start) + mark + inside + mark + text.slice(end), start: start + n, end: end + n };
}

/** A link on the selection, to a web address or a page (`page:<id>`). With nothing selected, `label` is the words. */
export function addLink(text: string, start: number, end: number, target: string, label: string): Edit {
  const words = text.slice(start, end) || label;
  const link = `[${words.replace(/[\\[\]]/g, (c) => `\\${c}`)}](${target})`;
  return { text: text.slice(0, start) + link + text.slice(end), start: start + link.length, end: start + link.length };
}

/** The token of a live number. */
export const numberToken = (id: string, part: NumPart): string => `{{measure:${id}${part === 'value' ? '' : `|${part}`}}}`;

/** Text put in at the caret, or over the selection. */
export function insertAt(text: string, start: number, end: number, s: string): Edit {
  return { text: text.slice(0, start) + s + text.slice(end), start: start + s.length, end: start + s.length };
}

/** A block whose words are only "/" opens the block menu. */
export const opensMenu = (b: EdBlock): boolean => b.text === '/';

export type { CalloutKind };
