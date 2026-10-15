// Landing page: the feature tour as tabs, and the download that fits this computer.
// Without this file the page still works: every panel shows, and all three downloads look the same.
(() => {
  const tabs = [...document.querySelectorAll('.tabs [role="tab"]')];
  if (tabs.length) {
    document.documentElement.classList.add('js');
    document.querySelector('.tabs').hidden = false;
    const select = (tab, focus) => {
      for (const t of tabs) {
        const on = t === tab;
        t.setAttribute('aria-selected', String(on));
        t.tabIndex = on ? 0 : -1;
        document.getElementById(t.getAttribute('aria-controls')).hidden = !on;
      }
      if (focus) tab.focus();
    };
    tabs.forEach((t) => {
      t.id = `tab-${t.getAttribute('aria-controls')}`;
      document.getElementById(t.getAttribute('aria-controls')).setAttribute('aria-labelledby', t.id);
      t.addEventListener('click', () => select(t, false));
      t.addEventListener('keydown', (e) => {
        const i = tabs.indexOf(t);
        const next = { ArrowRight: i + 1, ArrowLeft: i - 1, Home: 0, End: tabs.length - 1 }[e.key];
        if (next === undefined) return;
        e.preventDefault();
        select(tabs[(next + tabs.length) % tabs.length], true);
      });
    });
    select(tabs[0], false);
  }

  const ua = navigator.userAgent;
  const os = /Windows/.test(ua) ? 'windows' : /Mac OS X/.test(ua) && !/iPhone|iPad/.test(ua) ? 'mac' : /Linux/.test(ua) && !/Android/.test(ua) ? 'linux' : '';
  const mine = os && document.querySelector(`.platform[data-os="${os}"]`);
  if (mine) {
    mine.classList.add('mine');
    const label = document.querySelector('[data-os-label]');
    if (label) label.textContent = `Download for ${mine.querySelector('strong').textContent}`;
  }
})();
