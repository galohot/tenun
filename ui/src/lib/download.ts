// Hands a file to the browser as a download. In the desktop app the webview decides where it goes.

export function saveBlob(blob: Blob, name: string): void {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 5000);
}

export function saveBytes(bytes: Uint8Array | ArrayBuffer, name: string, type: string): void {
  saveBlob(new Blob([bytes as BlobPart], { type }), name);
}
