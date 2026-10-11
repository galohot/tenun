// App-wide state: the workspace snapshot, the route, the theme, live events.

import { guessLocale } from './locale';
import { PHONE_MAX } from './layout';
import { setLinkLabels } from './links';
import { api, type CloudStatus, type Dashboard, type Health, type Settings, type Snapshot, type Table } from './api';

export type Route =
  | { view: 'home' }
  | { view: 'data'; table?: string }
  | { view: 'model' }
  | { view: 'dashboard'; id: string };

export function parseHash(hash = location.hash): Route {
  const parts = hash.replace(/^#\/?/, '').split('/').map(decodeURIComponent);
  if (parts[0] === 'data') return { view: 'data', table: parts.slice(1).join('/') || undefined };
  if (parts[0] === 'model') return { view: 'model' };
  if (parts[0] === 'd' && parts[1]) return { view: 'dashboard', id: parts[1] };
  return { view: 'home' };
}

export function go(route: Route) {
  const h =
    route.view === 'data'
      ? `#/data${route.table ? '/' + route.table.split('/').map(encodeURIComponent).join('/') : ''}`
      : route.view === 'model'
        ? '#/model'
        : route.view === 'dashboard'
          ? `#/d/${encodeURIComponent(route.id)}`
          : '#/';
  if (location.hash !== h) location.hash = h;
}

export type ThemePref = 'system' | 'light' | 'dark';

interface Toast {
  id: number;
  kind: 'info' | 'error';
  text: string;
}

class App {
  #snap = $state<Snapshot | null>(null);
  health = $state<Health | null>(null);
  route = $state<Route>(parseHash());
  themePref = $state<ThemePref>((localStorage.getItem('tenun-theme') as ThemePref) || 'system');
  systemDark = $state(matchMedia('(prefers-color-scheme: dark)').matches);
  toasts = $state<Toast[]>([]);
  /** Bumped when source data changes; tiles re-query when it moves. */
  dataVersion = $state(0);
  /** Bumped when another window changes the dashboards; open views drop their undo history. */
  outsideEdits = $state(0);
  addOpen = $state(false);
  cloud = $state<CloudStatus | null>(null);
  /** Set while waiting for the browser approval of a tenun.cloud connection. */
  connecting = $state<{ code: string; url: string } | null>(null);
  busy = $state<string | null>(null);
  /** This computer's settings; null until they load. */
  settings = $state<Settings | null>(null);
  settingsOpen = $state(false);
  /** A narrow screen (a phone in browser mode): one column, no editing, and the sidebar is a drawer. */
  phone = $state(matchMedia(`(max-width: ${PHONE_MAX - 1}px)`).matches);
  navOpen = $state(false);

  theme = $derived<'light' | 'dark'>(
    this.themePref === 'system' ? (this.systemDark ? 'dark' : 'light') : this.themePref,
  );

  /** What the data says about each link, by relationship id (V4-D26). */
  linkHealth = $derived(this.snap?.health);
  tables = $derived<Table[]>(this.snap?.tables ?? []);
  dashboards = $derived<Dashboard[]>(this.snap?.workspace.dashboards ?? []);
  /** How numbers and dates read in the open workspace. A file without it reads as English. */
  locale = $derived<'id' | 'en'>(this.snap?.workspace.locale === 'id' ? 'id' : 'en');

  #toastId = 0;
  #saveTimer: ReturnType<typeof setTimeout> | null = null;
  #lastLocalSave = 0;

  get snap(): Snapshot | null {
    return this.#snap;
  }

  /** The names of the links are remembered for titles and column headings, which are plain functions. */
  set snap(v: Snapshot | null) {
    this.#snap = v;
    setLinkLabels(v?.workspace.relationships ?? [], v?.tables ?? []);
  }

  constructor() {
    addEventListener('hashchange', () => (this.route = parseHash()));
    matchMedia('(prefers-color-scheme: dark)').addEventListener('change', (e) => (this.systemDark = e.matches));
    matchMedia(`(max-width: ${PHONE_MAX - 1}px)`).addEventListener('change', (e) => (this.phone = e.matches));
  }

  setTheme(p: ThemePref) {
    this.themePref = p;
    localStorage.setItem('tenun-theme', p);
  }

  /** Saves a change to this computer's settings; the switch moves at once. */
  async saveSettings(patch: Partial<Settings>) {
    const prev = this.settings ?? { advanced: false };
    this.settings = { ...prev, ...patch };
    try {
      this.settings = await api.saveSettings(this.settings);
    } catch (e) {
      this.settings = prev;
      this.error(e);
    }
  }

  async init() {
    try {
      [this.health, this.snap, this.cloud, this.settings] = await Promise.all([
        api.health(),
        api.workspace(),
        api.cloud().catch(() => null),
        api.settings().catch(() => ({ advanced: false })),
      ]);
      await this.matchBrowserLanguage();
    } catch (e) {
      this.error(e);
    }
    this.listen();
  }

  /**
   * An empty workspace that nobody has saved reads in the language of the browser, so a
   * computer set to Indonesian starts in Indonesian. A workspace with content is left alone.
   */
  async matchBrowserLanguage() {
    const w = this.snap?.workspace;
    if (!w || this.snap?.path !== null || w.sources.length || w.dashboards.length) return;
    if (guessLocale(navigator.language) === 'id' && w.locale !== 'id') {
      this.snap = await api.newWorkspace();
    }
  }

  /** Changes how the open workspace reads numbers and dates. */
  async setLocale(locale: 'id' | 'en') {
    if (locale === this.locale) return;
    await this.run(null, () => api.setLocale(locale));
  }

  table(id: string): Table | undefined {
    return this.tables.find((t) => t.id === id);
  }

  toast(text: string, kind: 'info' | 'error' = 'info') {
    const id = ++this.#toastId;
    this.toasts.push({ id, kind, text });
    setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), kind === 'error' ? 7000 : 3500);
  }

  error(e: unknown) {
    this.toast(e instanceof Error ? e.message : String(e), 'error');
  }

  /** Runs an API call that returns a new snapshot, with a busy label. */
  async run(label: string | null, f: () => Promise<Snapshot>) {
    this.busy = label;
    try {
      this.snap = await f();
      return true;
    } catch (e) {
      this.error(e);
      return false;
    } finally {
      this.busy = null;
    }
  }

  /** Dashboard edits are applied locally at once and saved shortly after. */
  saveDashboards() {
    if (this.#saveTimer) clearTimeout(this.#saveTimer);
    this.#saveTimer = setTimeout(async () => {
      if (!this.snap) return;
      this.#lastLocalSave = Date.now();
      try {
        await api.putDashboards($state.snapshot(this.snap.workspace.dashboards));
        if (this.snap.path === null) this.snap.dirty = true;
      } catch (e) {
        this.error(e);
      }
    }, 350);
  }

  async refresh() {
    try {
      this.snap = await api.workspace();
    } catch (e) {
      this.error(e);
    }
  }

  listen() {
    const proto = location.protocol === 'https:' ? 'wss' : 'ws';
    const ws = new WebSocket(`${proto}://${location.host}/api/events`);
    ws.onmessage = (m) => {
      let ev: { type: string; message?: string };
      try {
        ev = JSON.parse(m.data);
      } catch {
        return;
      }
      if (ev.type === 'data-changed') {
        this.dataVersion++;
        this.refresh();
      } else if (ev.type === 'workspace-changed') {
        this.refresh();
      } else if (ev.type === 'dashboards-changed') {
        // Our own save echoes back; only other windows need to reload.
        if (Date.now() - this.#lastLocalSave > 1500) {
          this.outsideEdits++;
          this.refresh();
        }
      } else if (ev.type === 'cloud-connected') {
        this.connecting = null;
        api.cloud().then((c) => (this.cloud = c));
        if ((ev as { ok?: boolean }).ok) this.toast('Connected to tenun.cloud');
        else this.toast('The connection was not approved in time. Try again.', 'error');
      } else if (ev.type === 'published') {
        this.refresh();
      } else if (ev.type === 'error' && ev.message) {
        this.toast(ev.message, 'error');
      }
    };
    ws.onclose = () => setTimeout(() => this.listen(), 1500);
  }
}

export const app = new App();
