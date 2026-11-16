// Verify the shipped hero video on the running site: desktop/mobile, both
// themes, runtime theme switch, loop wrap and reduced motion.
//   node scripts/verify-hero.mjs [--shots]   (--shots writes .hero-shots/*.png)
import { pathToFileURL } from 'node:url'
const { chromium } = await import(pathToFileURL('C:/Users/muham/.agents/skills/stageshow/node_modules/playwright-core/index.mjs').href)
const EDGE = 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'
const URL = process.env.HERO_URL || 'http://127.0.0.1:4400/'
const shots = process.argv.includes('--shots')
const { mkdirSync } = await import('node:fs')
if (shots) mkdirSync('.hero-shots', { recursive: true })
const snap = (page, name) => shots ? page.screenshot({ path: '.hero-shots/' + name }) : Promise.resolve()

const browser = await chromium.launch({ executablePath: EDGE, headless: true })
const fails = []
const check = (name, ok, extra = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${extra ? ` — ${extra}` : ''}`)
  if (!ok) fails.push(name)
}

async function session(opts) {
  const ctx = await browser.newContext(opts)
  const page = await ctx.newPage()
  const errors = []
  page.on('pageerror', (e) => errors.push(e.message))
  page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()) })
  await page.goto(URL, { waitUntil: 'load' })
  await page.waitForSelector('[data-hero-video]', { state: 'attached', timeout: 15000 })
  return { ctx, page, errors }
}

const state = (page) =>
  page.evaluate(() => {
    const v = document.querySelector('[data-hero-video]')
    const s = document.querySelector('[data-hero-still]')
    const r = v.getBoundingClientRect()
    return {
      cut: v.dataset.cut,
      src: v.currentSrc,
      paused: v.paused,
      time: v.currentTime,
      duration: v.duration,
      hidden: v.hidden,
      stillHidden: s.hidden,
      stillSrc: s.src,
      box: { w: Math.round(r.width), h: Math.round(r.height) },
      ready: v.readyState,
    }
  })

// Desktop, light
{
  const { ctx, page, errors } = await session({ viewport: { width: 1280, height: 900 }, colorScheme: 'light' })
  await page.waitForTimeout(1200)
  const a = await state(page)
  check('desktop light cut', a.cut === 'wide-light', a.cut)
  check('desktop light plays webm', /hero-wide-light\.webm$/.test(a.src), a.src)
  check('desktop light playing', !a.paused && a.ready >= 2, `paused=${a.paused} ready=${a.ready}`)
  await page.waitForTimeout(1000)
  const b = await state(page)
  check('desktop light time advances', b.time > a.time, `${a.time.toFixed(2)} -> ${b.time.toFixed(2)}`)
  check('desktop box ratio', Math.abs(b.box.w / b.box.h - 1120 / 540) < 0.02, JSON.stringify(b.box))
  await snap(page, 'site-desktop-light.png')
  // loop wrap
  await page.evaluate(() => { const v = document.querySelector('[data-hero-video]'); v.currentTime = v.duration - 0.4 })
  await page.waitForTimeout(1200)
  const c = await state(page)
  check('loop wraps and keeps playing', !c.paused && c.time < 2, `t=${c.time.toFixed(2)}`)
  // runtime theme switch
  await page.evaluate(() => { document.documentElement.dataset.theme = 'dark' })
  await page.waitForTimeout(900)
  const d = await state(page)
  check('theme switch swaps cut', d.cut === 'wide-dark' && /hero-wide-dark\.webm$/.test(d.src), d.src)
  check('theme switch keeps playing', !d.paused && d.ready >= 2)
  await snap(page, 'site-desktop-dark.png')
  check('desktop no console errors', errors.length === 0, errors.join(' | ').slice(0, 300))
  await ctx.close()
}

// Desktop, dark from load
{
  const { ctx, page, errors } = await session({ viewport: { width: 1280, height: 900 }, colorScheme: 'dark' })
  await page.waitForTimeout(1200)
  const a = await state(page)
  check('dark load cut', a.cut === 'wide-dark', a.cut)
  check('dark load playing', !a.paused)
  check('dark no console errors', errors.length === 0, errors.join(' | ').slice(0, 300))
  await ctx.close()
}

// Mobile
{
  const { ctx, page, errors } = await session({ viewport: { width: 390, height: 844 }, colorScheme: 'light', isMobile: true, hasTouch: true })
  await page.waitForTimeout(1200)
  const a = await state(page)
  check('mobile cut', a.cut === 'compact-light', a.cut)
  check('mobile plays', !a.paused && a.ready >= 2, `paused=${a.paused}`)
  check('mobile box ratio', Math.abs(a.box.w / a.box.h - 358 / 330) < 0.02, JSON.stringify(a.box))
  await snap(page, 'site-mobile.png')
  check('mobile no console errors', errors.length === 0, errors.join(' | ').slice(0, 300))
  await ctx.close()
}

// Reduced motion
{
  const { ctx, page, errors } = await session({ viewport: { width: 1280, height: 900 }, reducedMotion: 'reduce' })
  await page.waitForTimeout(800)
  const a = await state(page)
  check('reduced motion hides video', a.hidden && a.paused)
  check('reduced motion shows poster', !a.stillHidden && /hero-poster-wide-light\.webp$/.test(a.stillSrc), a.stillSrc)
  await snap(page, 'site-reduced.png')
  check('reduced no console errors', errors.length === 0, errors.join(' | ').slice(0, 300))
  await ctx.close()
}

await browser.close()
console.log(fails.length ? `VERIFY FAILED: ${fails.join(', ')}` : 'VERIFY PASSED')
process.exit(fails.length ? 1 : 0)