import '@fontsource-variable/inter';
import './lib/theme.css';
import { mount } from 'svelte';
import App from './App.svelte';

mount(App, { target: document.getElementById('app')! });
