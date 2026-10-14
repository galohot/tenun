// Server-rendered pages: account, device approval, notices. Same look as the landing page
// (/style.css) plus /cloud.css. No inline scripts anywhere (the CSP forbids them).
import { esc } from './util';

export function page(title: string, body: string, opts: { account?: { email: string } | null; bare?: boolean } = {}): string {
  // A bare page (a password page, which may sit inside another site) has no sign-in links.
  const who = opts.bare
    ? ''
    : opts.account
      ? `<span class="who">${esc(opts.account.email)}</span><a href="/account">Account</a><a href="/auth/logout">Sign out</a>`
      : `<a href="/auth/login">Sign in</a>`;
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<meta name="robots" content="noindex">
<title>${esc(title)} — Tenun</title>
<link rel="icon" href="/icon.svg" type="image/svg+xml">
<link rel="stylesheet" href="/style.css">
<link rel="stylesheet" href="/cloud.css">
</head>
<body>
<header class="nav">
  <a class="brand" href="/"><img src="/icon.svg" alt="" width="28" height="28"> Tenun</a>
  <nav>${who}</nav>
</header>
<main class="cloud">${body}</main>
<footer><p><a href="/privacy">Privacy</a> · <a href="/terms">Terms</a> · <a href="https://github.com/galohot/tenun">Source code</a></p></footer>
</body>
</html>`;
}

export type DashRow = {
  id: string;
  /** A dashboard, or a whole site (build 14). */
  kind?: 'dashboard' | 'site';
  /** A site's pages. */
  pages?: number;
  title: string;
  /** `password` is a link with a password (v5 build 11). */
  access: 'link' | 'invited' | 'password';
  /** Other sites may frame it (v5 build 11). */
  embed?: number;
  source: string;
  every_minutes: number | null;
  version: number;
  generated_at: string | null;
  bytes: number;
};

export type TokenRow = { id: string; label: string; created_at: number; last_used_at: number | null };

const ago = (ms: number | null) => {
  if (!ms) return 'never';
  const s = (Date.now() - ms) / 1000;
  if (s < 90) return 'just now';
  if (s < 5400) return `${Math.round(s / 60)} min ago`;
  if (s < 129600) return `${Math.round(s / 3600)} h ago`;
  return `${Math.round(s / 86400)} days ago`;
};

const ACCESS: Record<string, string> = { link: 'Anyone with the link', password: 'Anyone with the password', invited: 'Only invited people' };

const size = (b: number) => (b < 1024 * 1024 ? `${Math.max(1, Math.round(b / 1024))} KB` : `${(b / 1048576).toFixed(1)} MB`);

export function accountPage(
  account: { email: string },
  dashboards: DashRow[],
  tokens: TokenRow[],
  limit: number,
): string {
  const dashList = dashboards.length
    ? dashboards
        .map(
          (d) => `<li class="row">
      <div><a class="title" href="/${d.kind === 'site' ? 's' : 'd'}/${esc(d.id)}">${esc(d.title)}</a>
      <p class="meta">${d.kind === 'site' ? `Site of ${d.pages ?? 1} ${(d.pages ?? 1) === 1 ? 'page' : 'pages'} · ` : ''}${ACCESS[d.access] ?? ACCESS.link}${d.embed ? ' · can be embedded' : ''} ·
      ${d.generated_at ? `updated ${esc(ago(Date.parse(d.generated_at)))}` : 'not published yet'}${d.source ? ` from ${esc(d.source)}` : ''} ·
      ${size(d.bytes)}</p></div>
      <form method="post" action="/account/dashboards/${esc(d.id)}/delete"><button class="button small danger">Delete</button></form>
    </li>`,
        )
        .join('')
    : `<li class="empty">No published dashboards or sites yet. In Tenun, open a dashboard and choose <b>Publish</b>.</li>`;
  const tokenList = tokens.length
    ? tokens
        .map(
          (t) => `<li class="row"><div><span class="title">${esc(t.label || 'Tenun')}</span>
      <p class="meta">connected ${esc(ago(t.created_at))} · last used ${esc(ago(t.last_used_at))}</p></div>
      <form method="post" action="/account/tokens/${esc(t.id)}/revoke"><button class="button small">Disconnect</button></form></li>`,
        )
        .join('')
    : `<li class="empty">No apps connected.</li>`;
  return page(
    'Account',
    `<h1>Your dashboards and sites</h1>
     <p class="lede">${dashboards.length} of ${limit} published dashboards and sites on the free plan. A site counts as one.</p>
     <ul class="list">${dashList}</ul>
     <h2>Connected apps</h2>
     <p class="meta">Each copy of Tenun or <code>tenun agent</code> that can publish for you.</p>
     <ul class="list">${tokenList}</ul>
     <h2>Delete account</h2>
     <p class="meta">Deletes every published dashboard and site, every version and every connection. It cannot be undone.</p>
     <form method="post" action="/account/delete" class="stack"><label class="check"><input type="checkbox" name="sure" value="yes" required> I understand</label>
     <button class="button danger">Delete my account</button></form>`,
    { account },
  );
}

export function connectPage(account: { email: string }, code: string, label: string | null, error = ''): string {
  if (!label) {
    return page(
      'Connect Tenun',
      `<h1>Connect Tenun</h1>
       <p class="lede">Type the code that Tenun shows.</p>
       ${error ? `<p class="error">${esc(error)}</p>` : ''}
       <form method="get" action="/connect" class="stack"><input name="code" class="field code" required placeholder="ABCD-EFGH" autocomplete="off" value="${esc(code)}"><button class="button primary">Continue</button></form>`,
      { account },
    );
  }
  return page(
    'Connect Tenun',
    `<h1>Connect “${esc(label)}”?</h1>
     <p class="lede">It will be able to publish dashboards to your account (${esc(account.email)}). You can disconnect it later on your account page.</p>
     <p class="code-show">${esc(code)}</p>
     <form method="post" action="/connect" class="row-buttons"><input type="hidden" name="code" value="${esc(code)}">
     <button class="button primary" name="ok" value="yes">Connect</button><a class="button" href="/account">Cancel</a></form>`,
    { account },
  );
}

export function notice(title: string, text: string, status: 'ok' | 'warn' = 'ok', account?: { email: string } | null): string {
  return page(title, `<h1>${esc(title)}</h1><p class="lede ${status}">${text}</p>`, { account });
}

/**
 * The page a password item shows until the right password is given (v5 build 11). It holds no data and not even the
 * title: the owner may not want that seen either.
 */
export function passwordPage(kind: 'dashboard' | 'site', id: string, error = ''): string {
  const what = kind === 'site' ? 'site' : 'dashboard';
  return page(
    'Password',
    `<h1>This ${what} has a password</h1>
     <p class="lede">Type the password its owner gave you.</p>
     ${error ? `<p class="error" role="alert">${esc(error)}</p>` : ''}
     <form method="post" action="/${kind === 'site' ? 's' : 'd'}/${esc(id)}/unlock" class="stack">
     <label for="pw">Password</label>
     <input id="pw" name="password" type="password" class="field" required autocomplete="current-password" autofocus>
     <button class="button primary">Open</button></form>`,
    { bare: true },
  );
}

/** The hosted dashboard or site: the viewer bundle reads the snapshot URL and freshness from meta tags. */
export function hostedShell(d: DashRow): string {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<meta name="robots" content="noindex, nofollow">
<title>${esc(d.title)} — Tenun</title>
<link rel="icon" href="/icon.svg" type="image/svg+xml">
<link rel="stylesheet" href="/viewer/viewer.css">
<meta name="tenun-snapshot" content="/api/v1/${d.kind === 'site' ? 's' : 'd'}/${esc(d.id)}/snapshot?v=${d.version}">
<meta name="tenun-updated" content="${esc(d.generated_at ?? '')}">
<meta name="tenun-source" content="${esc(d.source)}">
<meta name="tenun-every" content="${d.every_minutes ?? ''}">
</head>
<body>
<div id="app"></div>
<script type="module" src="/viewer/viewer.js"></script>
</body>
</html>`;
}
