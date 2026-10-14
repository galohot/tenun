<script lang="ts">
  // "Email me updates" on a hosted page (v5 build 13): a reader the owner invited asks tenun.cloud for the page's
  // numbers by email, and can stop them here or with the link in each email. Shown only when tenun.cloud sends emails.
  let { path }: { path: string } = $props();
  /** null: not offered to this reader (or not known yet). */
  let on = $state<boolean | null>(null);
  let busy = $state(false);
  let error = $state('');

  $effect(() => {
    fetch(`/api/v1/${path}/updates`)
      .then((r) => (r.ok ? r.json() : null))
      .then((j: { on?: boolean } | null) => (on = j ? !!j.on : null))
      .catch(() => {});
  });

  async function flip() {
    busy = true;
    error = '';
    try {
      const r = await fetch(`/api/v1/${path}/updates`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ on: !on }) });
      const j = (await r.json()) as { on?: boolean; message?: string };
      if (r.ok) on = !!j.on;
      else error = j.message ?? 'That did not work. Try again later.';
    } catch {
      error = 'tenun.cloud could not be reached.';
    } finally {
      busy = false;
    }
  }
</script>

{#if on !== null}
  <span class="email">
    {' · '}<button class="linkbtn" onclick={flip} disabled={busy}>{on ? 'Stop email updates' : 'Email me updates'}</button>
    {#if error}<span class="err" role="alert">{error}</span>{/if}
  </span>
{/if}

<style>
  .linkbtn { border: 0; background: none; padding: 0; font: inherit; color: var(--accent-text); cursor: pointer; }
  .linkbtn:disabled { opacity: 0.6; }
  .err { color: var(--danger); margin-left: 6px; }
</style>
