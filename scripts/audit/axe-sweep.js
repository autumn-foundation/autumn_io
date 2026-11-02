// Sweep every sitemap URL at 1280px and 320px: axe-core violations, page-level horizontal scroll (WCAG 1.4.10),
// and raw HTML-comment text leaking into prose (outside pre/code).
// Usage: serve locally on :3000, then in a scratch dir `npm i playwright-core axe-core` and
// `NODE_PATH=<scratch>/node_modules CHROMIUM_PATH=/opt/pw-browsers/chromium node scripts/audit/axe-sweep.js`.
const { chromium } = require('playwright-core');
const fs = require('fs');
const axeSrc = fs.readFileSync(require.resolve('axe-core/axe.min.js'), 'utf8');
(async () => {
  const sm = await (await fetch('http://127.0.0.1:3000/sitemap.xml')).text();
  const paths = [...sm.matchAll(/<loc>https:\/\/autumn-web\.app([^<]*)/g)].map(m => m[1] || '/');
  const b = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH || undefined, args: ['--no-sandbox'] });
  const axeBad = [], hscroll = [], leaks = [];
  for (const vw of [1280, 320]) {
    const p = await (await b.newContext({ bypassCSP: true, viewport: { width: vw, height: 800 } })).newPage();
    for (const u of paths) {
      await p.goto('http://127.0.0.1:3000' + u, { waitUntil: 'load' });
      await p.addScriptTag({ content: axeSrc });
      const r = await p.evaluate(async () => {
        const x = await axe.run();
        const leak = []; const root = document.querySelector('.article-body');
        if (root) { const w = document.createTreeWalker(root, NodeFilter.SHOW_TEXT); let n;
          while ((n = w.nextNode())) if (!n.parentElement.closest('pre,code') && n.textContent.includes('<!--')) leak.push(n.textContent.trim().slice(0, 50)); }
        const d = document.documentElement;
        return { v: x.violations.map(v => v.id + ':' + v.impact + ':' + v.nodes.length), sw: d.scrollWidth, w: d.clientWidth, leak };
      });
      if (r.v.length) axeBad.push([vw, u, r.v.join(',')]);
      if (vw === 320 && r.sw > r.w) hscroll.push([u, r.sw + '>' + r.w]);
      if (vw === 1280 && r.leak.length) leaks.push([u, r.leak.length]);
    }
  }
  console.log('pages', paths.length, 'axe-violating page/viewports', axeBad.length, 'hscroll@320', hscroll.length, 'pages with leaked <!-- text', leaks.length);
  axeBad.forEach(x => console.log('AXE', x.join(' '))); hscroll.forEach(x => console.log('HSCROLL', x.join(' '))); leaks.forEach(x => console.log('LEAK', x.join(' ')));
  await b.close();
})();
