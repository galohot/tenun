// A one-page PDF by hand: the dashboard picture, embedded as a JPEG (DCTDecode). No library.
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

  // The second line holds high bytes, which tells programs that this file is binary.
  put('%PDF-1.4\n');
  put(Uint8Array.of(0x25, 0xe2, 0xe3, 0xcf, 0xd3, 0x0a));
  object(1, ['<< /Type /Catalog /Pages 2 0 R >>']);
  object(2, ['<< /Type /Pages /Kids [3 0 R] /Count 1 >>']);
  object(3, [
    `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${page.w} ${page.h}] /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>`,
  ]);
  const draw = `q ${page.w} 0 0 ${page.h} 0 0 cm /Im0 Do Q`;
  object(4, [`<< /Length ${draw.length} >>\nstream\n${draw}\nendstream`]);
  object(5, [
    `<< /Type /XObject /Subtype /Image /Width ${px.w} /Height ${px.h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length ${jpeg.length} >>\nstream\n`,
    jpeg,
    '\nendstream',
  ]);
  object(6, [`<< /Title ${textString(title)} /Producer ${textString('Tenun')} >>`]);

  const xref = len;
  const count = offsets.length; // object 0 is the free entry
  put(`xref\n0 ${count}\n0000000000 65535 f \n`);
  for (let n = 1; n < count; n++) put(`${String(offsets[n]).padStart(10, '0')} 00000 n \n`);
  put(`trailer\n<< /Size ${count} /Root 1 0 R /Info 6 0 R >>\nstartxref\n${xref}\n%%EOF\n`);

  const out = new Uint8Array(len);
  let at = 0;
  for (const c of chunks) {
    out.set(c, at);
    at += c.length;
  }
  return out;
}
