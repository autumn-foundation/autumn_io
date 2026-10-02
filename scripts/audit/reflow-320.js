// Reflow audit (WCAG 1.4.10): crawl every sitemap URL at 320px and report pages with page-level horizontal scroll.
// Usage: serve locally on :3000, `npm i playwright-core && npx playwright-core install chromium`,
// then `node scripts/audit/reflow-320.js`. Set CHROMIUM_PATH to use an existing Chromium binary instead.
const { chromium } = require('playwright-core');
(async () => {
  const sm = await (await fetch('http://127.0.0.1:3000/sitemap.xml')).text();
  const paths = [...sm.matchAll(/<loc>https:\/\/autumn-web\.app([^<]*)/g)].map(m => m[1] || '/');
  const b = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH || undefined, args: ['--no-sandbox'] });
  const p = await b.newPage({ viewport: { width: 320, height: 700 } });
  let bad = [];
  for (const u of paths) {
    await p.goto('http://127.0.0.1:3000' + u, { waitUntil: 'load' });
    const r = await p.evaluate(() => {
      const d = document.documentElement, w = d.clientWidth, o = [];
      for (const e of document.querySelectorAll('body *')) {
        const r = e.getBoundingClientRect();
        if (r.right > w + 1 && !e.closest('pre,table,.code-block,[style*="overflow"]')) o.push(e.tagName + '.' + e.className + ' "' + (e.textContent||'').slice(0,40) + '" right=' + Math.round(r.right));
      }
      return { sw: d.scrollWidth, w, o: o.slice(0, 3) };
    });
    if (r.sw > r.w) bad.push([u, r]);
  }
  console.log('pages', paths.length, 'hscroll', bad.length);
  for (const [u, r] of bad) console.log(u, r.sw, '>', r.w, JSON.stringify(r.o));
  await b.close();
})();
