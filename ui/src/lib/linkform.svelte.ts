// The fields of a web link: address, key, records and pages. The Add sheet and Link settings share
// this one form, so a link reads the same in both. The pure parts live in sources.ts.

import { api, type KeyIn, type KeyPlace, type LinkLook, type LinkSettings, type Paging, type Source, type UrlEvery } from './api';
import { NO_PAGES, keyStart, linkProblem, pagesFromApi, pagesFromHint, pagesToApi, splitKey, type PagesBy, type PagesForm } from './sources';

export class LinkForm {
  url = $state('');
  name = $state('');
  every = $state<UrlEvery>('hourly');
  /** The Key section is open. */
  keyOpen = $state(false);
  place = $state<KeyPlace>('header');
  keyName = $state(keyStart('header'));
  keyValue = $state('');
  /** Link settings: a value for this link's key is stored on this computer. */
  stored = $state(false);
  /** Link settings: the key goes when the settings are saved. */
  removeKey = $state(false);
  /** Set when a key was found in the pasted link and moved to the Key section. */
  moved = $state('');
  /** The list of records the user chose; empty lets Tenun pick. */
  records = $state('');
  pages = $state<PagesForm>({ ...NO_PAGES });
  /** The user has set the pages: a new preview does not change them. */
  pagesTouched = $state(false);
  look = $state<LinkLook | null>(null);
  looking = $state(false);
  lookError = $state('');
  /** Link settings: the address cannot change. */
  readonly locked: boolean;
  #seq = 0;

  constructor(source?: Source, stored = false) {
    this.locked = !!source;
    if (!source) return;
    this.url = source.url ?? '';
    this.name = source.name;
    this.every = source.every ?? 'hourly';
    this.records = source.records ?? '';
    this.pages = pagesFromApi(source.pages);
    this.pagesTouched = true;
    if (source.key) {
      this.place = source.key.place;
      this.keyName = source.key.name;
      this.keyOpen = true;
    }
    this.stored = stored;
  }

  get problem(): string {
    return linkProblem(this.url);
  }

  /** The list that is read: the one chosen, else the one Tenun picked for the preview. */
  get chosen(): string {
    return this.records || this.look?.records || '';
  }

  /** A key will be sent: one is stored and stays, or a value is typed. */
  get hasKey(): boolean {
    return (this.stored && !this.removeKey) || !!this.keyValue;
  }

  /** Takes a key out of the link and puts it in the Key section. */
  tidy() {
    if (this.locked) return;
    const r = splitKey(this.url);
    if (!r.key) return;
    this.url = r.url;
    this.place = 'query';
    this.keyName = r.key.name;
    this.keyValue = r.key.value;
    this.removeKey = false;
    this.keyOpen = true;
    this.moved = `Moved ${r.key.name} out of the link. Tenun keeps keys out of the workspace file.`;
  }

  /**
   * The address was edited. What a preview showed, and the records and pages chosen from it, belong
   * to the old address: they are dropped, so Add does not send them with the new one.
   */
  addressEdited() {
    if (this.locked || (!this.look && !this.lookError && !this.records && !this.pagesTouched)) return;
    this.#seq++;
    this.look = null;
    this.looking = false;
    this.lookError = '';
    this.records = '';
    this.pages = { ...NO_PAGES };
    this.pagesTouched = false;
  }

  setPlace(p: KeyPlace) {
    // The starting name follows the place until the user writes their own.
    if (this.keyName.trim() === keyStart(this.place)) this.keyName = keyStart(p);
    this.place = p;
  }

  setPages(by: PagesBy) {
    this.pagesTouched = true;
    this.pages.by = by;
    if (by === 'page' && !this.pages.param) this.pages.param = 'page';
    if (by === 'cursor' && !this.pages.param) this.pages.param = 'cursor';
  }

  removeStored() {
    this.removeKey = true;
    this.keyValue = '';
  }

  /** The key to send for a look or for a new link. The value is empty when the stored one is meant. */
  keyIn(): KeyIn | null {
    const name = this.keyName.trim();
    if (!name || this.removeKey) return null;
    if (this.keyValue) return { place: this.place, name, value: this.keyValue };
    return this.stored ? { place: this.place, name, value: '' } : null;
  }

  /** Reads the first page: the format, the lists of records, the first rows. */
  async preview() {
    this.tidy();
    if (!this.url.trim() || this.problem) return;
    const seq = ++this.#seq;
    this.looking = true;
    this.lookError = '';
    try {
      const r = await api.linkPreview({ url: this.url.trim(), records: this.records, key: this.keyIn() });
      if (seq !== this.#seq) return;
      this.look = r;
      if (!this.pagesTouched) this.pages = pagesFromHint(r.pages_hint);
    } catch (e) {
      if (seq !== this.#seq) return;
      this.look = null;
      this.lookError = e instanceof Error ? e.message : String(e);
    } finally {
      if (seq === this.#seq) this.looking = false;
    }
  }

  setRecords(path: string) {
    this.records = path;
    void this.preview();
  }

  /** The settings of a new link. Without a preview, Tenun picks the records and reads one page. */
  settings(): LinkSettings {
    const json = this.look?.format === 'json';
    return {
      records: json ? this.chosen : undefined,
      pages: json ? pagesToApi(this.pages) : null,
      key: this.keyIn(),
    };
  }

  /** The change to save in Link settings. A key that is left out stays as it is. */
  change(): { records?: string; pages?: Paging | null; key?: KeyIn | null } {
    const json = this.look ? this.look.format === 'json' : true;
    const out: { records?: string; pages?: Paging | null; key?: KeyIn | null } = {
      records: json ? this.chosen : undefined,
      pages: json ? pagesToApi(this.pages) : null,
    };
    if (this.removeKey) out.key = null;
    else {
      const k = this.keyIn();
      if (k) out.key = k;
    }
    return out;
  }
}
