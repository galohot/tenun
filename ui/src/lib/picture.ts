// What a picture of a tile says under it (V4-D52, V4-D54). A picture leaves the computer, so it takes
// the notes that have no key value, no file name and no formula: `publicNotes`. The notes of the app
// (`notes`) name them and stay on the screen.

/** The line under a picture: the parts that are words already, then the public notes of the result. */
export function pictureNote(res: { notes?: string[]; publicNotes?: string[] } | null | undefined, ...parts: (string | null | undefined)[]): string {
  return [...parts, ...(res?.publicNotes ?? [])].filter((p): p is string => !!p).join(' ');
}
