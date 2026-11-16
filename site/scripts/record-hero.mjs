// Record the landing hero's stageshow animation into real video files.
//
//   node scripts/record-hero.mjs [--url http://localhost:4321] [--layout all|wide|compact]
//        [--theme all|light|dark] [--fps 30]
//
// Needs the site dev server running (pnpm dev), ffmpeg on PATH, and a
// Chromium browser (Edge is found automatically; CHROME_PATH overrides).
// playwright-core is borrowed from the installed stageshow skill when the
// site does not have it. Nothing is downloaded or installed.
//
// The kit's virtual clock (kit/clock.ts) is injected before page code, so
// time only moves when this script steps it: every frame is deterministic,
// no dropped or doubled frames. One full play is captured at 2x per layout
// and per theme (the page ground is baked in, so each theme gets its own
// cut), the ending is dissolved back into the opening with a 0.6s crossfade
// so the video loops without a jump, and ffmpeg encodes H.264 MP4 + VP9 WebM
// plus a WebP poster of the finished state.
//
// Outputs: public/videos/hero-{wide,compact}-{light,dark}.{mp4,webm},
//          public/images/hero-poster-{wide,compact}-{light,dark}.webp

import { mkdirSync, rmSync, copyFileSync, statSync } from 'node:fs'
import { resolve, dirname } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { spawnSync } from 'node:child_process'

const here = dirname(fileURLToPath(import.meta.url))
const siteRoot = resolve(here, '..')

const args = process.argv.slice(2)
const opt = (name, fallback) => {
  const i = args.indexOf(`--${name}`)
  return i === -1 ? fallback : args[i + 1]
}
const base = opt('url', 'http://localhost:4321').replace(/\/$/, '')
const layoutArg = opt('layout', 'all')
const themeArg = opt('theme', 'all')
const fps = Number(opt('fps', 30))
const layouts = layoutArg === 'all' ? ['wide', 'compact'] : [layoutArg]
const themes = themeArg === 'all' ? ['light', 'dark'] : [themeArg]

let chromium
try {
  ;({ chromium } = await import('playwright-core'))
} catch {
  ;({ chromium } = await import(pathToFileURL('C:/Users/muham/.agents/skills/stageshow/node_modules/playwright-core/index.mjs').href))
}

const edge = ['C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', 'C:/Program Files/Microsoft/Edge/Application/msedge.exe'].find((p) => {
  try { return statSync(p).isFile() } catch { return false }
})
const executablePath = process.env.CHROME_PATH || edge
if (!executablePath) {
  console.error('record: no Chromium browser found; set CHROME_PATH')
  process.exit(2)
}

// The virtual clock, straight from the kit's source (Node strips the types).
const { installVirtualClock } = await import(new URL('../src/components/stageshow/kit/clock.ts', import.meta.url).href)
const clockInit = `(${installVirtualClock.toString()})({ fps: ${fps} });`

const ffmpeg = (argv, label) => {
  const r = spawnSync('ffmpeg', ['-hide_banner', '-loglevel', 'error', '-y', ...argv], { encoding: 'utf8' })
  if (r.status !== 0) {
    console.error(`record: ffmpeg failed (${label})\n${r.stderr}`)
    process.exit(2)
  }
}

const SIZES = { wide: { w: 1120, h: 540 }, compact: { w: 358, h: 330 } }
const FADE = Math.round(fps * 0.6) // frames dissolved at the loop seam

async function record(layout, theme) {
  const { w, h } = SIZES[layout]
  const tag = `${layout}-${theme}`
  const frames = resolve(siteRoot, `.hero-frames/${tag}`)
  rmSync(frames, { recursive: true, force: true })
  mkdirSync(frames, { recursive: true })

  const browser = await chromium.launch({ executablePath, headless: true })
  const page = await browser.newPage({ viewport: { width: w, height: h }, deviceScaleFactor: 2 })
  page.on('pageerror', (e) => console.error(`record: page exception: ${e.message}`))
  await page.addInitScript(clockInit)
  // The dev toolbar is a fixed overlay at the foot of the page; it must not
  // be baked into the film.
  await page.addInitScript(() => {
    const hide = () => {
      const style = document.createElement('style')
      style.textContent = 'astro-dev-toolbar{display:none!important}'
      ;(document.head || document.documentElement).append(style)
    }
    if (document.documentElement) hide()
    else document.addEventListener('DOMContentLoaded', hide)
  })
  const route = `hero${layout === 'compact' ? '-compact' : ''}${theme === 'dark' ? '-dark' : ''}`
  await page.goto(`${base}/capture/${route}`, { waitUntil: 'load', timeout: 60000 })
  await page.waitForSelector('.ss-stage', { timeout: 30000 })
  // The island drops `ssr` once hydrated; stepping before that would let the
  // script start mid-film and bake dead air into the opening.
  await page.waitForFunction(() => {
    const island = document.querySelector('astro-island')
    return !!island && !island.hasAttribute('ssr')
  }, null, { timeout: 30000 })
  await page.evaluate(() => document.fonts.ready)
  // Hydration and ResizeObserver run on real time; let them land, then flush
  // the kit's layout frames so the stage finishes placing.
  for (let i = 0; i < 6; i++) {
    await new Promise((r) => setTimeout(r, 150))
    await page.evaluate(() => window.__stageshow.clock.flushLayout())
  }
  await page.waitForSelector('.ss-stage:not([data-placing])', { timeout: 15000 })
  await page.evaluate(() => window.__stageshow.clock.settle())

  const shot = (f) => resolve(frames, `f${String(f).padStart(4, '0')}.png`)
  // The scene plays once and holds its ending. After the 'done' mark the
  // script still waits 900ms, moves the pointer off (~650ms) and holds 2s;
  // record 4.2s past the mark so the tail is a settled, static hold. The
  // seam fade dissolves that hold back into the opening one.
  let n = 0
  let sawDone = false
  let endAt = -1
  let posterAt = -1
  let posterSaved = false
  const maxFrames = fps * 30
  for (let f = 0; f < maxFrames; f++) {
    await page.screenshot({ path: shot(f) })
    const mark = await page.evaluate(() => document.querySelector('.ss-stage')?.getAttribute('data-mark') ?? null)
    if (mark === 'done' && !sawDone) {
      sawDone = true
      posterAt = f + Math.round(fps * 0.7) // after the check chip's pop lands
      endAt = f + Math.round(fps * 4.2)
    }
    if (f === posterAt && !posterSaved) {
      copyFileSync(shot(f), resolve(frames, 'poster.png'))
      posterSaved = true
    }
    if (sawDone && f >= endAt) { n = f + 1; break }
    await page.evaluate(() => window.__stageshow.clock.step())
  }
  await browser.close()
  if (!n || !posterSaved) {
    console.error(`record: ${tag} never completed a loop (frames: ${n}, poster: ${posterSaved})`)
    process.exit(2)
  }
  if (n <= FADE * 2 + 2) {
    console.error(`record: ${tag} loop too short for a seam fade: ${n} frames`)
    process.exit(2)
  }
  console.log(`record: ${tag} captured ${n} frames (${(n / fps).toFixed(2)}s) at ${w * 2}x${h * 2}`)

  // One loop, seam dissolved: middle = frames FADE..n-FADE, then the last
  // FADE frames crossfading into the first FADE. The video ends exactly on
  // its own opening frame, so `loop` never jumps.
  const seq = `-framerate ${fps} -i ${frames}/f%04d.png`
  const graph = [
    `[0:v]split=3[a][b][c]`,
    `[a]trim=start_frame=${FADE}:end_frame=${n - FADE},setpts=PTS-STARTPTS,format=yuv420p[mid]`,
    `[b]trim=start_frame=${n - FADE}:end_frame=${n},setpts=PTS-STARTPTS,format=yuv420p[tail]`,
    `[c]trim=start_frame=0:end_frame=${FADE},setpts=PTS-STARTPTS,format=yuv420p[head]`,
    `[tail][head]xfade=transition=fade:duration=${FADE / fps}:offset=0[xf]`,
    `[mid][xf]concat=n=2:v=1:a=0,scale=${w * 2}:${h * 2}:flags=lanczos,setsar=1[v]`,
  ].join(';')
  const videos = resolve(siteRoot, 'public/videos')
  const images = resolve(siteRoot, 'public/images')
  mkdirSync(videos, { recursive: true })
  ffmpeg([...seq.split(' '), '-filter_complex', graph, '-map', '[v]', '-c:v', 'libx264', '-preset', 'slow', '-crf', '19', '-pix_fmt', 'yuv420p', '-movflags', '+faststart', resolve(videos, `hero-${tag}.mp4`)], `${tag} mp4`)
  ffmpeg([...seq.split(' '), '-filter_complex', graph, '-map', '[v]', '-c:v', 'libvpx-vp9', '-crf', '32', '-b:v', '0', '-cpu-used', '4', '-row-mt', '1', resolve(videos, `hero-${tag}.webm`)], `${tag} webm`)
  ffmpeg(['-i', resolve(frames, 'poster.png'), '-c:v', 'libwebp', '-quality', '92', resolve(images, `hero-poster-${tag}.webp`)], `${tag} poster`)
  rmSync(frames, { recursive: true, force: true })
  for (const file of [`videos/hero-${tag}.mp4`, `videos/hero-${tag}.webm`, `images/hero-poster-${tag}.webp`]) {
    console.log(`record: public/${file} (${(statSync(resolve(siteRoot, 'public', file)).size / 1024).toFixed(0)} KB)`)
  }
}

for (const layout of layouts) for (const theme of themes) await record(layout, theme)
console.log('record: done')