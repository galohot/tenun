<script lang="ts">
  // An image tile. Only PNG, JPEG and WebP data URLs are drawn, whatever a hand-edited file holds.
  import type { TileImage } from '../../lib/api';

  let { image, editing = false }: { image?: TileImage; editing?: boolean } = $props();
  const ok = $derived(!!image && /^data:image\/(png|jpeg|webp);base64,[A-Za-z0-9+/=]+$/.test(image.data));
</script>

{#if image && ok}
  <img src={image.data} alt={image.alt} style:object-fit={image.fit ?? 'contain'} />
{:else if editing || image}
  <div class="none">{image ? 'This picture cannot be shown. Pick a PNG, JPEG or WebP file.' : 'Pick a picture in the panel on the right.'}</div>
{/if}

<style>
  img { display: block; width: 100%; height: 100%; border-radius: 6px; }
  .none { height: 100%; display: grid; place-items: center; text-align: center; color: var(--text-2); font-size: 12px; }
</style>
