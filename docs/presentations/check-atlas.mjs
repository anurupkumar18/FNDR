import assert from 'node:assert/strict';
import { chromium } from '@playwright/test';
const target = new URL('./fndr-systems-atlas.html', import.meta.url);
const browser = await chromium.launch({ channel: process.env.PLAYWRIGHT_CHANNEL || 'chrome' });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
  const errors = [], network = [];
  page.on('pageerror', e => errors.push(e.message));
  page.on('request', r => { if (/^https?:/.test(r.url())) network.push(r.url()); });
  const now = new Date('2026-11-16T12:00:00Z');
  await page.clock.install({ time: now });
  await page.clock.pauseAt(now);
  await page.goto(target.href);
  const active = () => page.locator('.slide:not([hidden])').getAttribute('data-system');
  assert.equal(await page.locator('.slide').count(), 38);
  for (const group of 'ABCDEF') for (let n = 1; n <= 6; n++) assert.equal(await page.locator(`[data-system="${group}${n}"]`).count(), 1);
  await page.clock.runFor(9999);
  assert.equal(await active(), 'Poster');
  await page.clock.runFor(1);
  assert.equal(await active(), 'M0', 'Ten-second automatic advance');
  await page.keyboard.press('End');
  await page.clock.runFor(10000);
  assert.equal(await active(), 'Poster', 'Automatic wraparound');
  await page.keyboard.press('ArrowLeft');
  assert.equal(await active(), 'F6', 'Previous wraps to last');
  await page.keyboard.press('ArrowRight');
  assert.equal(await active(), 'Poster', 'Next wraps to first');
  await page.getByRole('button', { name: 'Pause autoplay' }).click();
  await page.clock.runFor(30000);
  assert.equal(await active(), 'Poster');
  await page.getByRole('button', { name: 'Resume autoplay' }).click();
  await page.clock.runFor(9000);
  await page.keyboard.press('ArrowRight');
  await page.clock.runFor(9999);
  assert.equal(await active(), 'M0', 'Manual navigation resets timer');
  await page.clock.runFor(1);
  assert.equal(await active(), 'A1');
  await page.getByRole('button', { name: 'Slides', exact: true }).click();
  await page.clock.runFor(20000);
  assert.equal(await active(), 'A1', 'Picker holds autoplay');
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await page.clock.runFor(10000);
  assert.equal(await active(), 'A2', 'Closing picker resumes timer');
  await page.keyboard.press('g');
  await page.keyboard.press('Escape');
  await page.clock.runFor(10000);
  assert.equal(await active(), 'A3', 'Escape resumes timer');
  await page.getByRole('button', { name: 'Pause autoplay' }).click();
  await page.keyboard.press('g');
  await page.getByRole('button', { name: 'C2 Exact reopen', exact: true }).click();
  await page.keyboard.press('+');
  assert.equal(await page.locator('#zoom').textContent(), '125%');
  await page.keyboard.press('0');
  assert.equal(await page.locator('#zoom').textContent(), '100%');
  await page.reload();
  assert.equal(await active(), 'C2', 'Deep link survives reload');
  await page.getByRole('button', { name: 'Pause autoplay' }).click();
  for (let n = 1; n <= 38; n++) {
    await page.evaluate(n => location.hash = 'slide-' + n, n);
    await page.waitForFunction(n => document.querySelector('.slide:not([hidden])').dataset.index === String(n - 1), n);
    const data = await page.locator('.slide:not([hidden])').evaluate(el => ({
      missing: [...el.querySelectorAll('img')].filter(i => !i.complete || !i.naturalWidth).length,
      svg: el.querySelectorAll('svg').length,
      overflow: [...el.querySelectorAll('.slide-copy')].some(e => e.scrollHeight > e.clientHeight + 2)
    }));
    assert.equal(data.missing, 0);
    assert(data.svg > 0);
    assert.equal(data.overflow, false);
  }
  for (const [width,height] of [[1920,1080],[768,1024],[390,844]]) {
    await page.setViewportSize({ width,height });
    await page.waitForFunction(() => { const r=document.querySelector('#stage').getBoundingClientRect(); return r.left>=0 && r.right<=innerWidth+1 && r.bottom<=innerHeight+1; });
  }
  assert.deepEqual(errors, []);
  assert.deepEqual(network, []);
  console.log('PASS: 38 slides, all source assets, 10-second timing, wraparound, pause/resume, picker, deep links, zoom, responsive sizing, offline');
  const real = await browser.newPage();
  await real.goto(target.href + '#slide-38');
  await real.waitForFunction(() => document.querySelector('.slide:not([hidden])').dataset.system === 'Poster', {}, { timeout: 13000 });
  console.log('PASS: real elapsed ten-second browser timer wraps slide 38 to slide 1');
} finally { await browser.close(); }
