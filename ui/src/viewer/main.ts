import '../lib/theme.css';
import { mount } from 'svelte';
import Viewer from './Viewer.svelte';
import type { ExportPayload } from '../lib/export';

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
      target.textContent = 'This dashboard could not be loaded. Reload the page to try again.';
      return;
    }
  }
  if (!data) return;
  const every = Number(meta('tenun-every')) || null;
  mount(Viewer, {
    target,
    props: {
      data,
      hosted: url ? { updated: meta('tenun-updated') || data.generated, source: meta('tenun-source'), every } : null,
    },
  });
}

start();
