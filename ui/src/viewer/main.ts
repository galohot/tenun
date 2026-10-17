import '../lib/theme.css';
import { mount } from 'svelte';
import Viewer from './Viewer.svelte';
import SiteApp from './SiteApp.svelte';
import type { ExportPayload, SitePayload } from '../lib/export';
import { setDigits, setLocale } from '../lib/format';
import { setColumnFormats } from '../lib/measures';
import { setCalendar, type Calendar } from '../lib/locale';
import { useCarried } from '../lib/geoassets';
import { setLang, t } from '../lib/i18n';
import { guessLocale } from '../lib/locale';

// An exported file carries its data inline (window.TENUN). A dashboard hosted on
// tenun.cloud names its snapshot and freshness in meta tags instead.
const meta = (name: string) => document.querySelector(`meta[name="${name}"]`)?.getAttribute('content') ?? '';

async function start() {
  const target = document.getElementById('app')!;
  let data = (window as unknown as { TENUN?: ExportPayload }).TENUN;
  const url = meta('tenun-snapshot');
  if (!data && url) {
    try {
      const res = await fetch(url, { credentials: 'same-origin' });
      if (!res.ok) throw new Error(String(res.status));
      data = (await res.json()) as ExportPayload;
    } catch {
      // No workspace to say its language yet: the reader's browser says it.
      setLang(guessLocale(navigator.language));
      target.textContent = t('This dashboard could not be loaded. Reload the page to try again.');
      return;
    }
  }
  if (!data) return;
  // The numbers and dates read the way the author saw them, and so do the words: the readers chose nothing (AD-D10).
  setLocale(data.locale);
  setLang(data.locale === 'id' ? 'id' : 'en');
  // Digits and column formats as the author set them (formats §2.2, §2.3).
  setDigits(data.digits);
  setColumnFormats(data.columnFormats);
  // Periods are named in the calendar of the workspace the file came from (v5 build 04).
  setCalendar((data as unknown as { calendar?: Calendar }).calendar);
  // The maps the file carries are drawn from it, with no network (build 06).
  useCarried(data.geo);
  const every = Number(meta('tenun-every')) || null;
  // An invited reader may ask for email updates (v5 build 13): the page says where.
  const hosted = url ? { updated: meta('tenun-updated') || data.generated, source: meta('tenun-source'), every, updates: meta('tenun-updates') || null } : null;
  // A whole site (build 13): its navigation around the page the address names. On tenun.cloud (build 14), each page
  // says how fresh it is, as a hosted dashboard does.
  if ((data as unknown as SitePayload).kind === 'site') {
    mount(SiteApp, { target, props: { data: data as unknown as SitePayload, hosted } });
    return;
  }
  mount(Viewer, { target, props: { data, hosted } });
}

start();
