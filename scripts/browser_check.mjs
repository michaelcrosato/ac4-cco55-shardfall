#!/usr/bin/env node
// Browser check for the single-file build (PLAN.md §15.5): headless Chromium with WebGPU on
// SwiftShader opens the HTML from file:// and checks the front end through the page's
// `document.body.dataset.frontend` (the app writes its front-end mode there) and screenshots.
//   desktop 960x540: title → 11 s untouched → attract (DEMO PLAY) → a key → title again
//   phone 844x390 (isMobile, hasTouch): boots into attract → only ☰ → tap ☰ → its overlay
// Console errors and page errors fail the check.
//   node scripts/browser_check.mjs dist/<file>.html [out/browser]
import { execSync } from 'node:child_process';
import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const file = process.argv[2];
const outDir = process.argv[3] || 'out/browser';
if (!file || !fs.existsSync(file)) {
  console.error('usage: node scripts/browser_check.mjs <game.html> [outdir]');
  process.exit(2);
}
fs.mkdirSync(outDir, { recursive: true });

async function loadPlaywright() {
  try {
    return await import('playwright');
  } catch {
    const root = execSync('npm root -g').toString().trim();
    return createRequire(path.join(root, 'noop.js'))('playwright');
  }
}

const { chromium } = await loadPlaywright();
const args = [
  '--enable-unsafe-webgpu',
  '--enable-features=Vulkan',
  '--use-vulkan=swiftshader',
  '--use-angle=swiftshader',
  '--ignore-gpu-blocklist',
];
const browser = await chromium.launch({ headless: true, args });
const url = pathToFileURL(path.resolve(file)).href;
const results = [];
let failed = false;
const t0 = Date.now();
const secs = () => ((Date.now() - t0) / 1000).toFixed(1);

function report(name, ok, detail = '') {
  results.push({ name, ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ' — ' + detail : ''} [${secs()} s]`);
  if (!ok) failed = true;
}

function watchConsole(page, label) {
  const errors = [];
  page.on('console', (m) => {
    const text = m.text();
    if (m.type() === 'error') errors.push(text);
    if (/frontend:|demo_stall|panic|error|touch/i.test(text)) console.log(`  [${label} console.${m.type()}] ${text.slice(0, 200)}`);
  });
  page.on('pageerror', (e) => errors.push(String(e)));
  return errors;
}

const mode = (page) => page.evaluate(() => document.body.dataset.frontend || '');

// Screenshots wait for a frame; software WebGPU frames can take seconds.
async function shot(page, file) {
  try {
    await page.screenshot({ path: file, timeout: 180000 });
  } catch (e) {
    console.log(`  (screenshot ${file} failed: ${String(e).split('\n')[0]})`);
  }
}

// Animation frames per second over `ms` (the page's own clock).
async function fps(page, ms = 4000) {
  return page.evaluate(
    (ms) =>
      new Promise((done) => {
        let n = 0;
        const t0 = performance.now();
        const tick = () => {
          n++;
          if (performance.now() - t0 < ms) requestAnimationFrame(tick);
          else done((n * 1000) / (performance.now() - t0));
        };
        requestAnimationFrame(tick);
      }),
    ms,
  );
}

async function waitMode(page, want, timeoutMs) {
  const end = Date.now() + timeoutMs;
  let last = '';
  while (Date.now() < end) {
    last = await mode(page).catch(() => '');
    if (want.includes(last)) return last;
    await page.waitForTimeout(250);
  }
  return last;
}

// ---- desktop
if (!process.argv.includes('--phone-only')) {
  const page = await browser.newPage({ viewport: { width: 960, height: 540 } });
  const errors = watchConsole(page, 'desktop');
  await page.goto(url);
  const m1 = await waitMode(page, ['title'], 180000);
  report('desktop boots to the title', m1 === 'title', `mode=${m1}`);
  await page.waitForTimeout(1500);
  console.log(`  title fps on this machine: ${(await fps(page)).toFixed(2)}`);
  await shot(page, `${outDir}/desktop_title.png`);
  // Untouched for 11 s (wall clock): attract.
  const idleStart = Date.now();
  const m2 = await waitMode(page, ['attract'], 60000);
  const idle = (Date.now() - idleStart) / 1000;
  report('11 s idle → attract (DEMO PLAY)', m2 === 'attract', `mode=${m2} after ${idle.toFixed(1)} s untouched`);
  await page.waitForTimeout(6000);
  await shot(page, `${outDir}/desktop_attract.png`);
  // Any input: straight back to the title.
  const before = Date.now();
  await page.keyboard.press('KeyK');
  const m3 = await waitMode(page, ['title'], 20000);
  report('a key in attract → title', m3 === 'title', `mode=${m3} after ${((Date.now() - before) / 1000).toFixed(1)} s`);
  await page.waitForTimeout(2500);
  await shot(page, `${outDir}/desktop_back_to_title.png`);
  report('desktop: no console errors', errors.length === 0, errors.slice(0, 5).join(' | '));
  // Persistence (localStorage): the first run saved its settings; a save put there survives a
  // reload and the app finds both (it lists them in data-store).
  const keys = await page.evaluate(() => Object.keys(localStorage));
  report('settings saved to localStorage', keys.some((k) => k.endsWith('.settings')), `keys=${keys.join(',')}`);
  const prefix = (keys.find((k) => k.endsWith('.settings')) || 'blacktap.settings').replace(/\.settings$/, '');
  await page.evaluate((p) => localStorage.setItem(`${p}.progress`, JSON.stringify({ version: 1, rounds_done: 2, best_round: 3, continue_scene: 'blacktap/round/3' })), prefix);
  await page.reload();
  const m4 = await waitMode(page, ['title'], 180000);
  const found = await page.evaluate(() => document.body.dataset.store || '');
  report('after a reload the app finds settings and progress', m4 === 'title' && found.includes('settings') && found.includes('progress'), `data-store=${found}`);
  await page.close();
}

// ---- phone
{
  const ctx = await browser.newContext({ viewport: { width: 844, height: 390 }, isMobile: true, hasTouch: true, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  const errors = watchConsole(page, 'phone');
  await page.goto(url);
  const m1 = await waitMode(page, ['attract'], 180000);
  report('phone boots into attract', m1 === 'attract', `mode=${m1}`);
  await page.waitForTimeout(6000);
  await shot(page, `${outDir}/phone_attract.png`);
  // Touching the game does not leave attract.
  const cdp = await ctx.newCDPSession(page);
  const tap = async (x, y) => {
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  };
  await tap(420, 220);
  await page.waitForTimeout(4000);
  const m2 = await mode(page);
  report('a tap on the game stays in attract', m2 === 'attract', `mode=${m2}`);
  // The ☰ button: the app publishes its centre (CSS px) in data-menu-x/y.
  const pos = await page.evaluate(() => [Number(document.body.dataset.menuX || 0), Number(document.body.dataset.menuY || 0)]);
  const [mx, my] = pos[0] > 0 ? pos : [844 - 34, 34];
  await tap(mx, my);
  const m3 = await waitMode(page, ['mobile_menu'], 20000);
  report('tap ☰ → the minimal overlay', m3 === 'mobile_menu', `mode=${m3} (tapped ${mx},${my})`);
  await page.waitForTimeout(3000);
  await shot(page, `${outDir}/phone_menu.png`);
  report('phone: no console errors', errors.length === 0, errors.slice(0, 5).join(' | '));
  await ctx.close();
}

await browser.close();
fs.writeFileSync(`${outDir}/results.json`, JSON.stringify({ file, results }, null, 2));
console.log(failed ? 'browser_check: FAILED' : 'browser_check: all passed');
process.exit(failed ? 1 : 0);
