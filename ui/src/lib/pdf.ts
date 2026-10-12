// A PDF by hand: the dashboard picture, embedded as a JPEG (DCTDecode), on one page; a story on A4 pages (build 11).
// No library.
// Pure code with no imports, so the node tests can load it and check the offsets of the xref table.

/** A4 landscape is 842 pt wide. The page keeps the picture's shape, and no side is longer than a viewer allows. */
export const PAGE_WIDTH = 842;
export const PAGE_MAX = 14400;

export function pageSize(pxW: number, pxH: number): { w: number; h: number } {
  let w = PAGE_WIDTH;
  let h = (PAGE_WIDTH * pxH) / pxW;
  if (h > PAGE_MAX) {
    w = (w * PAGE_MAX) / h;
    h = PAGE_MAX;
  }
  return { w: Math.round(w * 100) / 100, h: Math.round(h * 100) / 100 };
}

const ascii = (s: string): Uint8Array => Uint8Array.from(s, (c) => c.charCodeAt(0) & 0xff);

/** A PDF text string as UTF-16BE in hex, so any title is safe. */
function textString(s: string): string {
  let hex = 'FEFF';
  for (let i = 0; i < s.length; i++) hex += s.charCodeAt(i).toString(16).padStart(4, '0').toUpperCase();
  return `<${hex}>`;
}

/**
 * @param jpeg the bytes of a JPEG file
 * @param px the picture size in pixels
 * @param page the page size in points
 */
export function buildPdf(jpeg: Uint8Array, px: { w: number; h: number }, page: { w: number; h: number }, title = ''): Uint8Array {
  return buildPdfPages([{ jpeg, px, page, draw: { w: page.w, h: page.h } }], title);
}

/** One page: its picture, the picture's size in pixels, the page in points, and the size the picture is drawn at, from the top. */
export interface PdfPage {
  jpeg: Uint8Array;
  px: { w: number; h: number };
  page: { w: number; h: number };
  draw: { w: number; h: number };
}

/** A4 portrait, in points: a printed story (build 11). */
export const A4 = { w: 595.28, h: 841.89 };

/** A PDF of several pages, each one picture drawn at the top of its page. */
export function buildPdfPages(pages: readonly PdfPage[], title = ''): Uint8Array {
  const chunks: Uint8Array[] = [];
  const offsets: number[] = [];
  let len = 0;
  const put = (b: Uint8Array | string) => {
    const bytes = typeof b === 'string' ? ascii(b) : b;
    chunks.push(bytes);
    len += bytes.length;
  };
  const object = (n: number, body: (Uint8Array | string)[]) => {
    offsets[n] = len;
    put(`${n} 0 obj\n`);
    body.forEach(put);
    put('\nendobj\n');
  };
  const num = (x: number) => String(Math.round(x * 100) / 100);

  // The second line holds high bytes, which tells programs that this file is binary.
  put('%PDF-1.4\n');
  put(Uint8Array.of(0x25, 0xe2, 0xe3, 0xcf, 0xd3, 0x0a));
  // Objects: 1 the catalog, 2 the page tree, then three for each page (the page, its drawing, its picture), then the info.
  const kids = pages.map((_, i) => `${3 + 3 * i} 0 R`).join(' ');
  object(1, ['<< /Type /Catalog /Pages 2 0 R >>']);
  object(2, [`<< /Type /Pages /Kids [${kids}] /Count ${pages.length} >>`]);
  pages.forEach((p, i) => {
    const n = 3 + 3 * i;
    object(n, [
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${num(p.page.w)} ${num(p.page.h)}] /Resources << /XObject << /Im${i} ${n + 2} 0 R >> >> /Contents ${n + 1} 0 R >>`,
    ]);
    const draw = `q ${num(p.draw.w)} 0 0 ${num(p.draw.h)} 0 ${num(p.page.h - p.draw.h)} cm /Im${i} Do Q`;
    object(n + 1, [`<< /Length ${draw.length} >>\nstream\n${draw}\nendstream`]);
    object(n + 2, [
      `<< /Type /XObject /Subtype /Image /Width ${p.px.w} /Height ${p.px.h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length ${p.jpeg.length} >>\nstream\n`,
      p.jpeg,
      '\nendstream',
    ]);
  });
  const info = 3 + 3 * pages.length;
  object(info, [`<< /Title ${textString(title)} /Producer ${textString('Tenun')} >>`]);

  const xref = len;
  const count = offsets.length; // object 0 is the free entry
  put(`xref\n0 ${count}\n0000000000 65535 f \n`);
  for (let n = 1; n < count; n++) put(`${String(offsets[n]).padStart(10, '0')} 00000 n \n`);
  put(`trailer\n<< /Size ${count} /Root 1 0 R /Info ${info} 0 R >>\nstartxref\n${xref}\n%%EOF\n`);

  const out = new Uint8Array(len);
  let at = 0;
  for (const c of chunks) {
    out.set(c, at);
    at += c.length;
  }
  return out;
}
