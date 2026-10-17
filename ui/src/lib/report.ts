// "Report a problem" (0.7 build 05): the text of a GitHub issue, made on this computer and shown before
// anything leaves it. It holds the version, the system and the last error's words, never data, file names,
// paths or links: `scrub` takes those out of the error first.

export const ISSUES = 'https://github.com/galohot/tenun/issues/new';
/** GitHub refuses longer addresses; the typed text is cut to fit. */
export const MAX_URL = 8000;

export interface Report {
  what: string;
  version: string;
  system: string;
  error: string;
}

/** The error's words without what could be the user's: paths, links, e-mail addresses and quoted values. */
export function scrub(text: string): string {
  return text
    .replace(/[\w.+-]+@[\w-]+(\.[\w-]+)+/g, '[address]')
    .replace(/\b[a-z][a-z0-9+.-]*:\/\/\S+/gi, '[link]')
    .replace(/\b[A-Za-z]:[\\/][^\s"'`,;)]*/g, '[path]')
    .replace(/\\\\[^\s"'`,;)]+/g, '[path]')
    .replace(/(^|[\s("'`=])(~?\/[^\s"'`,;)]+)/g, '$1[path]')
    .replace(/"[^"\n]*"|'[^'\n]*'|`[^`\n]*`|“[^”\n]*”|‘[^’\n]*’/g, '"…"')
    .slice(0, 600);
}

/** The system in a few words, from the browser's user agent. */
export function systemOf(ua: string, desktop: boolean): string {
  const os = /Windows/i.test(ua) ? 'Windows' : /Mac OS X|Macintosh/i.test(ua) ? 'macOS' : /Android/i.test(ua) ? 'Android' : /iPhone|iPad/i.test(ua) ? 'iOS' : /Linux/i.test(ua) ? 'Linux' : 'Other';
  return `${os}, ${desktop ? 'desktop app' : 'browser'}`;
}

/** The issue as plain text, for "Copy instead". */
export function reportText(r: Report): string {
  return [`What happened:\n${r.what.trim() || '(not written)'}`, `Tenun version: ${r.version}`, `System: ${r.system}`, `Last error: ${r.error || '(none)'}`].join('\n\n');
}

/** The address of a new issue with the form filled in; the typed text is cut so the address fits. */
export function reportUrl(r: Report): { url: string; cut: boolean } {
  const make = (what: string) => {
    const q = new URLSearchParams({ template: 'problem.yml', title: 'Problem: ', what, version: r.version, system: r.system, error: r.error });
    return `${ISSUES}?${q}`;
  };
  let what = r.what;
  let url = make(what);
  let cut = false;
  while (url.length > MAX_URL && what.length) {
    what = what.slice(0, Math.max(0, what.length - Math.max(50, url.length - MAX_URL)));
    url = make(what);
    cut = true;
  }
  return { url, cut };
}
