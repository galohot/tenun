<script lang="ts">
  // The status bar (look 3): one line under every view on a desktop. It says only what the snapshot knows:
  // whether the sources read, how many rows there are, and when the newest source last loaded.
  import { app } from '../lib/state.svelte';
  import { stampLabel } from '../lib/format';
  import { palette, modKey } from './Palette.svelte';
  import { t, tn } from '../lib/i18n';

  const snap = $derived(app.snap!);
  // A new table made from other tables is not a source of its own: it follows its inputs.
  const sources = $derived(snap.workspace.sources.filter((s) => s.kind !== 'derived'));
  const failed = $derived(snap.workspace.sources.filter((s) => snap.errors[s.id]));
  const loadedAt = $derived(snap.loaded_at ?? {});
  const waiting = $derived(sources.filter((s) => !snap.errors[s.id] && !loadedAt[s.id]));
  const live = $derived(sources.length - sources.filter((s) => snap.errors[s.id]).length - waiting.length);

  const status = $derived<'none' | 'error' | 'waiting' | 'live'>(
    !snap.workspace.sources.length ? 'none' : failed.length ? 'error' : waiting.length && !live ? 'waiting' : 'live',
  );

  const stateWords = $derived.by(() => {
    if (status === 'none') return t('No data yet');
    if (status === 'error') return failed.length === 1 ? t('{name} could not be read', { name: failed[0].name }) : t('{n} sources could not be read', { n: failed.length });
    if (status === 'waiting') return tn('{n} source not loaded yet', '{n} sources not loaded yet', waiting.length);
    return tn('{n} source live', '{n} sources live', live);
  });
  const stateTitle = $derived(
    snap.workspace.sources
      .map((s) => {
        const at = loadedAt[s.id] ? stampLabel(loadedAt[s.id]) : '';
        if (snap.errors[s.id]) return t('{name}: could not be read. {error}', { name: s.name, error: snap.errors[s.id] });
        return at ? t('{name}: loaded {at}', { name: s.name, at }) : t('{name}: not loaded yet', { name: s.name });
      })
      .join('\n'),
  );

  // On a table in Data: its rows. Anywhere else: the rows of every table in the workspace.
  const nf = $derived(new Intl.NumberFormat(app.locale === 'id' ? 'id-ID' : 'en-US'));
  const table = $derived(app.route.view === 'data' && app.route.table ? app.table(app.route.table) : undefined);
  const rows = $derived.by(() => {
    if (table) return table.rows;
    const known = app.tables.filter((tb) => tb.rows !== null);
    return known.length ? known.reduce((n, tb) => n + (tb.rows ?? 0), 0) : null;
  });
  const rowsWords = $derived(rows === null ? '' : tn('{n} row', '{n} rows', rows, { n: nf.format(rows) }));
  const tableCount = $derived(app.tables.length);

  const newest = $derived(Object.values(loadedAt).filter(Boolean).sort().at(-1) ?? null);
</script>

<footer class="status" aria-label={t('Status')}>
  <span class="cell state {status}" title={stateTitle || undefined}>
    {#if status !== 'none'}<span class="dot" aria-hidden="true"></span>{/if}
    {stateWords}
  </span>
  {#if rowsWords}
    <span class="sep" aria-hidden="true"></span>
    <span class="cell num" title={table ? t('Rows in {name}', { name: table.label || table.name }) : tn('Rows in {n} table', 'Rows in {n} tables', tableCount)}>
      {table ? `${table.label || table.name} · ${rowsWords}` : rowsWords}{#if !table && tableCount > 1}<span class="soft"> {t('in {n} tables', { n: tableCount })}</span>{/if}
    </span>
  {/if}
  {#if newest}
    <span class="sep" aria-hidden="true"></span>
    <span class="cell num"><time datetime={newest}>{t('Updated {at}', { at: stampLabel(newest) })}</time></span>
  {/if}
  <button class="cell hint" onclick={() => (palette.open = true)} title={t('Command palette')}>
    <kbd>{modKey}K</kbd> {t('Commands')}
  </button>
</footer>

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 10px;
    height: var(--status-h);
    padding: 0 12px;
    flex: none;
    border-top: 1px solid var(--sep);
    background: var(--window);
    font-size: var(--fs-small);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    overflow: hidden;
  }
  .cell { display: inline-flex; align-items: center; gap: 6px; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .sep { width: 1px; height: 12px; background: var(--sep-strong); flex: none; }
  .dot { width: 7px; height: 7px; border-radius: 50%; flex: none; background: var(--success); }
  .state.waiting .dot { background: var(--text-3); }
  .state.error { color: var(--danger); }
  .state.error .dot { background: var(--danger); }
  .soft { color: var(--text-2); }
  .hint {
    margin-left: auto;
    height: 20px;
    padding: 0 6px;
    border: 0;
    border-radius: 5px;
    background: none;
    color: var(--text-2);
    font-size: var(--fs-small);
    transition: background var(--dur-fast) var(--ease);
  }
  .hint:hover { background: var(--hover); color: var(--text); }
  .hint kbd { height: 16px; padding: 0 4px; font-size: 10.5px; }
</style>
