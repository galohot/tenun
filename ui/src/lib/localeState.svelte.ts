// The locale the screen formats numbers and dates in. It is reactive, so every
// tile that read a number through `format.ts` draws again when it changes.

import type { Locale } from './locale';

export const localeState = $state<{ value: Locale }>({ value: 'en' });
