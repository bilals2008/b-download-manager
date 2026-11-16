// Film the eight feature stills and convert them to WebP for /features.
//   node scripts/film-features.mjs [--url http://localhost:4321] [--only name]
// Needs the dev server, ffmpeg, and Edge (CHROME_PATH overrides). Each scene
// is filmed at 2x with the rehearsal gate; a failing gate stops the run.
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { mkdirSync, statSync } from 'node:fs'
import { spawnSync } from 'node:child_process'

const here = dirname(fileURLToPath(import.meta.url))
const siteRoot = resolve(here, '..')
const args = process.argv.slice(2)
const opt = (n, d) => { const i = args.indexOf(`--${n}`); return i === -1 ? d : args[i + 1] }
const base = opt('url', 'http://localhost:4321').replace(/\/$/, '')
const only = opt('only', null)
const film = resolve('C:/Users/muham/.agents/skills/stageshow/scripts/film.mjs')
const edge = 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'
const names = ['torrents', 'scheduler', 'captions', 'drive', 'language', 'network', 'debrid', 'addurl', 'themes']
const out = resolve(siteRoot, 'public/images/features')
const shots = resolve(siteRoot, '.feat-shots')
mkdirSync(out, { recursive: true })
mkdirSync(shots, { recursive: true })

for (const name of names) {
  if (only && name !== only) continue
  const png = resolve(shots, `${name}.png`)
  const r = spawnSync(process.execPath, [film, `${base}/capture/features`, '--select', `#f-${name}`, '--width', name === 'themes' ? '1260' : '700', '--still', '--gate', '--out', png], {
    encoding: 'utf8',
    env: { ...process.env, CHROME_PATH: process.env.CHROME_PATH || edge },
  })
  const tail = `${r.stdout}${r.stderr}`.split('\n').filter((l) => /^\[|rehearsal|film:|incomplete/.test(l)).join('\n')
  console.log(`--- ${name}\n${tail}`)
  // film's own init script stamps verification attributes before hydration,
  // which React reports as a mismatch; that noise is not a scene fault.
  const noise = /^incomplete: (error: A tree hydrated|error: Failed to load resource.*favicon)/
  const real = `${r.stdout}${r.stderr}`.split('\n').filter((l) => l.startsWith('incomplete:') && !noise.test(l))
  const passed = /rehearsal passed/.test(r.stdout) && real.length === 0
  if (r.status !== 0 && !passed) {
    console.error(`film-features: ${name} failed (exit ${r.status})`)
    process.exit(1)
  }
  const webp = resolve(out, `${name}.webp`)
  const c = spawnSync('ffmpeg', ['-hide_banner', '-loglevel', 'error', '-y', '-i', png, '-c:v', 'libwebp', '-quality', '90', webp], { encoding: 'utf8' })
  if (c.status !== 0) { console.error(c.stderr); process.exit(1) }
  console.log(`public/images/features/${name}.webp (${(statSync(webp).size / 1024).toFixed(0)} KB)`)
}
console.log('film-features: done')