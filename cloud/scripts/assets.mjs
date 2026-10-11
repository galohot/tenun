// Gathers the static files: the landing page (../site) and the viewer bundle (../ui/dist-viewer).
import { cpSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..');
const pub = join(root, 'public');
const site = join(root, '..', 'site');
const viewer = join(root, '..', 'ui', 'dist-viewer');

for (const f of ['index.html', '404.html', 'style.css', 'icon.svg', 'favicon-32.png', 'apple-touch-icon.png', 'og.png', 'robots.txt', 'sitemap.xml']) {
  cpSync(join(site, f), join(pub, f));
}
for (const f of ['dashboard-light.webp', 'dashboard-dark.webp', 'model-light.webp', 'model-dark.webp', 'more-views-light.webp', 'more-views-dark.webp']) {
  if (existsSync(join(site, f))) cpSync(join(site, f), join(pub, f));
}
if (!existsSync(join(viewer, 'viewer.js'))) {
  console.error('Build the viewer first: pnpm -C ../ui build');
  process.exit(1);
}
rmSync(join(pub, 'viewer'), { recursive: true, force: true });
mkdirSync(join(pub, 'viewer'), { recursive: true });
cpSync(join(viewer, 'viewer.js'), join(pub, 'viewer', 'viewer.js'));
cpSync(join(viewer, 'viewer.css'), join(pub, 'viewer', 'viewer.css'));
console.log('assets ready');
