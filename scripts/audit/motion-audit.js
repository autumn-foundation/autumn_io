// Motion audit (tier 1): axe at two moments (mid-animation, settled), CLS/LCP, and content visibility
// under prefers-reduced-motion on/off, at 1280 and 320 wide. Serve on :3000 first.
// Usage: npm i playwright-core axe-core && CHROMIUM_PATH=... node scripts/audit/motion-audit.js
const { chromium } = require('playwright-core');
const axeSrc = require('fs').readFileSync(require.resolve('axe-core/axe.min.js'), 'utf8');
const URLS = ['/', '/docs/getting-started', '/search?q=routing'];
const VPS = [[1280, 800], [320, 700]];
const AXE = { runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'] } };
(async () => {
  const b = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH || undefined, args: ['--no-sandbox'] });
  for (const rm of ['no-preference', 'reduce']) for (const [w, h] of VPS) for (const u of URLS) {
    const ctx = await b.newContext({ viewport: { width: w, height: h }, reducedMotion: rm, bypassCSP: true });
    const p = await ctx.newPage();
    await p.addInitScript(() => {
      window.__cls = 0; window.__lcp = 0;
      new PerformanceObserver(l => { for (const e of l.getEntries()) if (!e.hadRecentInput) window.__cls += e.value; }).observe({ type: 'layout-shift', buffered: true });
      new PerformanceObserver(l => { for (const e of l.getEntries()) window.__lcp = e.startTime; }).observe({ type: 'largest-contentful-paint', buffered: true });
    });
    await p.goto('http://127.0.0.1:3000' + u, { waitUntil: 'domcontentloaded' });
    await p.waitForLoadState('load');
    await p.addScriptTag({ content: axeSrc });
    const early = await p.evaluate(o => axe.run(document, o), AXE);       // as soon as load fires
    await p.waitForTimeout(3000);
    // Scroll-reveal elements below the fold only animate once reached, so walk the page before measuring.
    const sh = await p.evaluate(() => document.documentElement.scrollHeight);
    for (let y = 0; y <= sh; y += h / 2) { await p.evaluate(y => scrollTo(0, y), y); await p.waitForTimeout(150); }
    await p.waitForTimeout(1500);
    await p.evaluate(() => scrollTo(0, 0));
    const late = await p.evaluate(o => axe.run(document, o), AXE);
    const m = await p.evaluate(() => ({ cls: +window.__cls.toFixed(4), lcp: Math.round(window.__lcp),
      hidden: [...document.querySelectorAll('[data-motion]')].filter(e => +getComputedStyle(e).opacity < 1).length,
      motionEls: document.querySelectorAll('[data-motion]').length }));
    const f = v => v.violations.map(x => x.id + '(' + x.impact + ',' + x.nodes.length + ')').join(' ') || '0';
    console.log(`rm=${rm} ${w}px ${u} | axe early: ${f(early)} | settled: ${f(late)} | CLS ${m.cls} LCP ${m.lcp}ms | motion els ${m.motionEls}, still<1 opacity after scrolling: ${m.hidden}`);
    await ctx.close();
  }
  // No-JS pass: the motion initializer must not be what makes content visible.
  for (const [w, h] of VPS) for (const u of URLS) {
    const ctx = await b.newContext({ viewport: { width: w, height: h }, javaScriptEnabled: false });
    const p = await ctx.newPage();
    await p.goto('http://127.0.0.1:3000' + u, { waitUntil: 'load' });
    const n = await p.locator('[data-motion]').evaluateAll(els => els.filter(e => +getComputedStyle(e).opacity < 1 || getComputedStyle(e).visibility === 'hidden').length);
    console.log(`js=off ${w}px ${u} | [data-motion] hidden: ${n}`);
    await ctx.close();
  }
  await b.close();
})();
