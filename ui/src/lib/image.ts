// What a picture really is, read from its first bytes and not from its name or its MIME type.
// Pure code with no imports, so the node tests can load it.

export type ImageKind = 'png' | 'jpeg' | 'webp';

/** The kind of a base64 data URL, by its first 12 bytes: PNG, JPEG or WebP. Never SVG. `null` for anything else. */
export function imageKind(data: string): ImageKind | null {
  const at = data.indexOf(';base64,');
  if (!data.startsWith('data:') || at < 0) return null;
  let head: string;
  try {
    // 16 base64 characters hold 12 bytes.
    head = atob(data.slice(at + 8, at + 24));
  } catch {
    return null;
  }
  if (head.startsWith('\x89PNG\r\n\x1a\n')) return 'png';
  if (head.startsWith('\xff\xd8\xff')) return 'jpeg';
  if (head.length >= 12 && head.startsWith('RIFF') && head.slice(8, 12) === 'WEBP') return 'webp';
  return null;
}

/** The same picture with a data URL header that says what the bytes are, or `null` if they are not a picture we draw. */
export function normalisePicture(data: string): string | null {
  const kind = imageKind(data);
  const at = data.indexOf(';base64,');
  return kind && at >= 0 ? `data:image/${kind}${data.slice(at)}` : null;
}
