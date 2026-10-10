import type { DType } from './api';
import type { IconName } from '../components/Icon.svelte';

export const typeIcon: Record<DType, IconName> = {
  number: 'hash',
  date: 'calendar',
  datetime: 'clock',
  text: 'text',
  bool: 'toggle',
};
