<script lang="ts">
  // An image tile. Only PNG, JPEG and WebP data URLs are drawn, whatever a hand-edited file holds.
  import type { TileImage } from '../../lib/api';
  import { imageKind } from '../../lib/image';
  import { t } from '../../lib/i18n';

  let { image, editing = false }: { image?: TileImage; editing?: boolean } = $props();
  const ok = $derived(!!image && /^data:image\/(png|jpeg|webp);base64,[A-Za-z0-9+/=]+$/.test(image.data) && imageKind(image.data) !== null);
  // The bytes can look right and still not decode. Then the tile says so in words.
  let broken = $state(false);
  $effect(() => {
    image?.data;
    broken = false;
  });
</script>

{#if image && ok && !broken}
  <img src={image.data} alt={image.alt} style:object-fit={image.fit ?? 'contain'} onerror={() => (broken = true)} />
{:else if editing || image}
  <div class="none">{image ? t('This picture cannot be shown. Pick a PNG, JPEG or WebP file.') : t('Pick a picture in the panel on the right.')}</div>
{/if}

<style>
  img { display: block; width: 100%; height: 100%; border-radius: 6px; }
  .none { height: 100%; display: grid; place-items: center; text-align: center; color: var(--text-2); font-size: 12px; }
</style>
