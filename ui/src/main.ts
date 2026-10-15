import '@fontsource-variable/inter';
import './lib/theme.css';
import { mount } from 'svelte';
import App from './App.svelte';

// Charts draw on a canvas once and do not redraw when a font arrives, so the app starts after Inter
// is in (a local file: a few milliseconds). If it cannot load, the app starts anyway with the next font.
const start = () => mount(App, { target: document.getElementById('app')! });
Promise.race([document.fonts.load('13px "Inter Variable"'), new Promise((r) => setTimeout(r, 1500))]).then(start, start);
