<script lang="ts" module>
  // Line icons on a 20-unit grid, 1.6 stroke, in the spirit of SF Symbols.
  const paths: Record<string, string> = {
    plus: 'M10 4v12M4 10h12',
    x: 'M5 5l10 10M15 5L5 15',
    check: 'M4.5 10.5l3.5 3.5 7.5-8',
    table: 'M3.5 4.5h13v11h-13zM3.5 8.5h13M3.5 12h13M8 8.5v7',
    model: 'M5 6.5a2 2 0 1 0 0-.01M15 4.5a2 2 0 1 0 0-.01M15 15.5a2 2 0 1 0 0-.01M6.8 7.4l6.4 6.6M6.9 6l6.2-1',
    dashboard: 'M3.5 3.5h5.5v7h-5.5zM11 3.5h5.5v4h-5.5zM11 9.5h5.5v7h-5.5zM3.5 12.5h5.5v4h-5.5z',
    bar: 'M4 16.5h12M6 16V9M10 16V4.5M14 16v-5',
    hbar: 'M3.5 4v12M4 6h7M4 10h11M4 14h5',
    line: 'M3.5 15l4-5 3.5 3 5.5-7.5',
    area: 'M3.5 16.5V12l4-4 3.5 3 5.5-6v11.5z',
    share: 'M3.5 6h13v3h-13zM3.5 6v3M9.5 6v3M13.5 6v3M4 13h4M4 16h7.5',
    scatter: 'M3.5 3.5v13h13M7 13.5a.5.5 0 1 0 0-.01M9.5 9.5a.5.5 0 1 0 0-.01M12 12a.5.5 0 1 0 0-.01M14 6.5a.5.5 0 1 0 0-.01M7.5 8a.5.5 0 1 0 0-.01',
    histogram: 'M3.5 16.5h13M4.5 16.5V11h3v5.5M7.5 16.5V6h3v10.5M10.5 16.5V4h3v12.5M13.5 16.5V9h2v7.5',
    waterfall: 'M4 16.5v-10h3v10M8.5 6.5h3v5h-3zM13 11.5h3v5h-3z',
    image: 'M3.5 4.5h13v11h-13zM3.5 13l4-4 3 3 2-2 4 4M13 8.2a.5.5 0 1 0 0-.01',
    pivot: 'M3.5 4.5h13v11h-13zM3.5 8.5h13M7.5 4.5v11M7.5 12h9',
    heatmap: 'M3.5 3.5h13v13h-13zM3.5 8h13M3.5 12h13M8 3.5v13M12 3.5v13M9 9h2.5v2.5H9zM4.5 4.5H7V7H4.5zM13 13h2.5v2.5H13z',
    funnel: 'M3 3.5h14v3H3zM5.5 8.5h9v3h-9zM8 13.5h4v3H8z',
    box: 'M10 3.5v3M6.5 6.5h7v7h-7zM6.5 10h7M10 13.5v3M7.5 3.5h5M7.5 16.5h5',
    bullet: 'M3.5 5.5h13v9h-13zM3.5 10h8M13.5 6.5v7',
    compare: 'M5.5 7h8M5.5 7a.5.5 0 1 0 0-.01M13.5 7a.5.5 0 1 0 0-.01M8 13h7M8 13a.5.5 0 1 0 0-.01M15 13a.5.5 0 1 0 0-.01',
    sankey: 'M3.5 4.5h2v5h-2zM3.5 12h2v3.5h-2zM14.5 4.5h2v11h-2zM5.5 6c5 0 4 1.5 9 1.5M5.5 13.5c5 0 4-1.5 9-1.5',
    calendarheat: 'M4 5h12v11.5H4zM4 8.5h12M7.5 3.5v3M12.5 3.5v3M7 11.5h.01M10 11.5h.01M13 11.5h.01M7 14h.01M10 14h.01',
    cohort: 'M3.5 4.5h13v3.5h-13zM3.5 8h9.5v3.5h-9.5zM3.5 11.5h6v4h-6z',
    gantt: 'M3.5 4v12M6 5.5h5M8.5 9.5h6M5 13.5h4',
    kpi: 'M4 6h5M4 9.5h9M4 16.5V14M7.5 16.5v-3.5M11 16.5v-2M14.5 16.5v-5.5',
    trash: 'M4.5 6h11M8 6V4.5h4V6M6 6l.7 10h6.6L14 6',
    refresh: 'M15.5 8A6 6 0 1 0 16 11M16 4v4h-4',
    export: 'M10 3.5v9M6.5 7L10 3.5 13.5 7M5 10.5v6h10v-6',
    folder: 'M3 6V15.5h14V7.5H9.5L8 5.5H3z',
    file: 'M5 3.5h6.5L15 7v9.5H5zM11.5 3.5V7H15',
    database: 'M4.5 5.5c0-1.1 2.5-2 5.5-2s5.5.9 5.5 2-2.5 2-5.5 2-5.5-.9-5.5-2zM4.5 5.5v9c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2v-9M4.5 10c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2',
    sheet: 'M4 3.5h12v13H4zM4 7.5h12M4 11.5h12M8.5 3.5v13',
    chevronRight: 'M8 5l5 5-5 5',
    chevronDown: 'M5 8l5 5 5-5',
    chevronLeft: 'M12 5l-5 5 5 5',
    search: 'M9 4a5 5 0 1 0 0 10A5 5 0 0 0 9 4zM12.7 12.7L16 16',
    sun: 'M10 6.5a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7zM10 2.5v1.5M10 16v1.5M2.5 10H4M16 10h1.5M4.7 4.7l1 1M14.3 14.3l1 1M4.7 15.3l1-1M14.3 5.7l1-1',
    moon: 'M15.5 12.5A6 6 0 0 1 7.5 4.5a6 6 0 1 0 8 8z',
    auto: 'M10 3.5a6.5 6.5 0 1 0 0 13zM10 3.5a6.5 6.5 0 0 1 0 13',
    globe: 'M10 3.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM3.5 10h13M10 3.5c-2 2-2.6 4.2-2.6 6.5S8 14.5 10 16.5c2-2 2.6-4.2 2.6-6.5S12 5.5 10 3.5',
    more: 'M5 10h.01M10 10h.01M15 10h.01',
    pencil: 'M12.5 4.5l3 3L8 15H5v-3zM11 6l3 3',
    link: 'M8.5 11.5l3-3M7 9.5L5.2 11.3a2.5 2.5 0 0 0 3.5 3.5l1.8-1.8M13 10.5l1.8-1.8a2.5 2.5 0 0 0-3.5-3.5L9.5 7',
    sparkles: 'M8 3.5l1.3 3.7L13 8.5l-3.7 1.3L8 13.5l-1.3-3.7L3 8.5l3.7-1.3zM14.5 12l.6 1.6 1.4.4-1.4.5-.6 1.5-.5-1.5-1.5-.5 1.5-.4z',
    warning: 'M10 3.5L17 16H3zM10 8.5v3.5M10 14h.01',
    clock: 'M10 3.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM10 6.5V10l2.5 1.5',
    filter: 'M3.5 5h13M6 10h8M8.5 15h3',
    grip: 'M7.5 5h.01M12.5 5h.01M7.5 10h.01M12.5 10h.01M7.5 15h.01M12.5 15h.01',
    resize: 'M15.5 9.5l-6 6M15.5 13.5l-2 2',
    save: 'M4 3.5h9.5L16 6v10.5H4zM7 3.5V7h5V3.5M7 16.5V11h6v5.5',
    open: 'M3 6V15.5h12L17 9H6.5L4.5 15.5M3 6h5l1.5 2H15v1',
    hash: 'M8 3.5l-1.5 13M13.5 3.5L12 16.5M4 7.5h12.5M3.5 12.5H16',
    calendar: 'M4 5h12v11.5H4zM4 8.5h12M7.5 3.5v3M12.5 3.5v3',
    text: 'M5 5.5h10M10 5.5v10',
    toggle: 'M7 6.5h6a3.5 3.5 0 0 1 0 7H7a3.5 3.5 0 0 1 0-7zM13 8.5a1.5 1.5 0 1 0 0 3',
    undo: 'M7.5 4.5L4 8l3.5 3.5M4 8h7.5a4.5 4.5 0 0 1 0 9H8',
    redo: 'M12.5 4.5L16 8l-3.5 3.5M16 8H8.5a4.5 4.5 0 0 0 0 9H12',
    settings: 'M10 7a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM8.6 3.5h2.8l.4 1.9 1.3.7 1.7-1 2 2-1 1.7.7 1.3 1.9.4v2.8l-1.9.4-.7 1.3 1 1.7-2 2-1.7-1-1.3.7-.4 1.9H8.6l-.4-1.9-1.3-.7-1.7 1-2-2 1-1.7-.7-1.3-1.9-.4V8.6l1.9-.4.7-1.3-1-1.7 2-2 1.7 1 1.3-.7z',
    pin: 'M7.5 3.5h5M8.5 3.5v5L6 12h8l-2.5-3.5v-5M10 12v4.5',
    expand: 'M12 3.5h4.5V8M8 16.5H3.5V12M16.5 3.5l-5 5M3.5 16.5l5-5',
    rows: 'M4 5.5h12M4 10h12M4 14.5h12',
    info: 'M10 3.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM10 9v4.5M10 6.5h.01',
    // A slanted f with a crossbar: a formula (build 02).
    fx: 'M12.6 3.9c-2.2-.6-3.5.5-3.8 2.6L7.7 16.1M5.6 9.3h6.1',
  };
  export type IconName = keyof typeof paths;
</script>

<script lang="ts">
  let { name, size = 16, class: cls = '' }: { name: IconName; size?: number; class?: string } = $props();
</script>

<svg
  class={cls}
  width={size}
  height={size}
  viewBox="0 0 20 20"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"><path d={paths[name]} /></svg>

<style>
  svg { flex: none; display: block; }
</style>
