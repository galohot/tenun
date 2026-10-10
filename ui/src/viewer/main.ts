import '../lib/theme.css';
import { mount } from 'svelte';
import Viewer from './Viewer.svelte';

mount(Viewer, { target: document.getElementById('app')! });
