import { AppWindow, FileArchive, FileText, Film, Folder, Gamepad2, Music, Play, Plus, Square } from 'lucide-react'
import { Button, Field, Popover, Stage, Window, type Act, type Layout } from '../kit'
import { APP_TOKENS } from '../app-tokens'
import './hero.css'

// B Download Manager as it looks on the landing page hero: the whole loop of
// the app in one pass. Beats: the pointer opens Add URL, a link types into the
// field, Add commits it and a new row arrives Downloading with its speed and
// time left; the row is selected, Stop holds it Paused on a blue meter, Resume
// lights lime and the click turns it Downloading again, the meter walks to the
// end, the row lands Completed with the green check, and the pointer steps off.
//
// What makes it read as this app:
// - One canvas, one colour (#111214). Panes and the toolbar find their edge
//   with a hairline, never a second shade; the stage plate is one step darker,
//   which is the only depth a dark window gets on a light page.
// - The meter is the app's: emerald through cyan to blue while it runs, a held
//   blue when paused (it never turns grey), green when done. Its gradient spans
//   the whole track, so the colours do not shift as the fill grows.
// - The status chip is the app's own geometry: filled disc, bright ring, gloss,
//   white glyph. It swaps in place on the same row; the words change beside it.
// - Real strings: the typed link, the file name, its size, the speed and the
//   time left. The rows under it are placeholder at the app's row height, and
//   the list runs off the bottom edge the way the window does.

type S = { open: boolean; url: string; added: boolean; sel: boolean; phase: 'running' | 'paused' | 'done'; pct: number }

const URL_TEXT = 'https://releases.ubuntu.com/24.04/ubuntu-24.04.iso'

const initial: S = { open: false, url: '', added: false, sel: false, phase: 'running', pct: 0 }

async function script(a: Act<S>) {
  await a.wait(600)
  await a.move('add')
  await a.click(a.layout === 'compact' ? 'add' : { at: 'add', x: 0.15, y: 0.5 })
  a.put({ open: true })
  a.mark('add-open')
  await a.wait(320)
  await a.move('url')
  await a.type('url', URL_TEXT)
  await a.wait(240)
  await a.move('confirm')
  await a.click({ at: 'confirm', x: 0.15, y: 0.5 })
  a.put({ open: false, added: true, phase: 'running', pct: 5.2 })
  a.mark('added')
  await a.wait(680)
  a.put({ pct: 12.8 })
  await a.move('row')
  await a.click('row')
  a.put({ sel: true })
  a.mark('selected')
  await a.wait(430)
  a.put({ pct: 21.6 })
  await a.move('stop')
  await a.click(a.layout === 'compact' ? 'stop' : { at: 'stop', x: 0.2, y: 0.5 })
  a.put({ phase: 'paused' })
  a.mark('stopped')
  await a.wait(760)
  await a.move('resume')
  await a.click(a.layout === 'compact' ? 'resume' : { at: 'resume', x: 0.15, y: 0.5 })
  a.put({ phase: 'running' })
  a.mark('resumed')
  for (const pct of [46.3, 68.9, 87.4, 97.1]) {
    await a.wait(470)
    a.put({ pct })
  }
  await a.wait(430)
  a.put({ phase: 'done', pct: 100 })
  a.mark('done')
  await a.wait(900)
  await a.move({ wide: { x: 1060, y: 490 }, compact: { x: 320, y: 336 } })
  await a.wait(1800)
}



const SUBJECT = { name: 'ubuntu-24.04.iso', size: '5.8 GB' }

const ROWS = [
  { name: 'blender-4.2.1-windows-x64.msi', size: '342 MB', pct: 100, phase: 'done' as const },
  { name: 'sundance-2026-keynote-1080p.mp4', size: '1.4 GB', pct: 38.2, phase: 'running' as const, speed: '4.2 MB/s', eta: '2 min 14 s' },
  { name: 'godot-4.4-stable-win64.exe', size: '118 MB', pct: 100, phase: 'done' as const },
]

const COMPLETED = [
  { name: 'opencode-1.0.168-windows-x64.zip', size: '86 MB', pct: 100 },
  { name: 'annual-report-2026.pdf', size: '4.2 MB', pct: 100 },
  { name: 'product-shots-final.zip', size: '318 MB', pct: 100 },
  { name: 'slides-valuation-v3.pptx', size: '27 MB', pct: 100 },
  { name: 'benchmark-suite-2026.7z', size: '1.1 GB', pct: 100 },
  { name: 'fonts-archive.zip', size: '12 MB', pct: 100 },
]

const LABEL = { paused: 'Paused', running: 'Downloading', done: 'Completed' } as const

/** Time left at the app's parallel-connection speed, from the bytes remaining. */
function etaFor(pct: number) {
  const secs = Math.round(((100 - pct) / 100) * 5800 / 12.6)
  const m = Math.floor(secs / 60)
  return m ? `${m} min ${secs % 60} s` : `${secs} s`
}

/** The app's material file icons, matched by extension. */
function fileIcon(name: string) {
  const n = name.toLowerCase()
  if (n.endsWith('.iso')) return '/images/file-icons/disc.svg'
  if (n.endsWith('.mp4') || n.endsWith('.mkv')) return '/images/file-icons/video.svg'
  if (n.endsWith('.exe') || n.endsWith('.msi')) return '/images/file-icons/exe.svg'
  if (n.endsWith('.pdf')) return '/images/file-icons/pdf.svg'
  if (n.endsWith('.pptx') || n.endsWith('.ppt')) return '/images/file-icons/powerpoint.svg'
  return '/images/file-icons/zip.svg'
}
const CHIP = { paused: 'paused', running: 'downloading', done: 'completed' } as const

const VIEWS = [
  { label: 'All Downloads', count: 16, view: 'all' as const },
  { label: 'Unfinished', count: 2, view: 'unfinished' as const },
  { label: 'Finished', count: 14, view: 'finished' as const },
]

const CATEGORIES = [
  { label: 'General', Icon: Folder },
  { label: 'Video', Icon: Film },
  { label: 'Music', Icon: Music },
  { label: 'Documents', Icon: FileText },
  { label: 'Programs', Icon: AppWindow },
  { label: 'Games', Icon: Gamepad2 },
  { label: 'Compressed', Icon: FileArchive },
]

export default function DownloadHero() {
  return (
    <Stage<S>
      width={1120}
      height={540}
      compact={{ below: 760, width: 358, height: 330 }}
      theme="dark"
      tokens={APP_TOKENS}
      initial={initial}
      poster={{ open: false, url: URL_TEXT, added: true, sel: true, phase: 'done', pct: 100 }}
      script={script}
      play="once"
      fill
      cursor={{ at: { wide: { x: 1060, y: 490 }, compact: { x: 300, y: 300 } }, idle: 3200 }}
      className="dl"
    >
      {(s, rect, layout) => {
        const pct = s.phase === 'done' ? 100 : s.pct
        const add = rect('add')
        const popW = layout === 'compact' ? 294 : 380
        const designW = layout === 'compact' ? 358 : 1120
        const popLeft = Math.max(32, Math.min(add.x + add.width - popW, designW - popW - 32))
        return (
          <>
            <Window
              className="dl-win"
              lights="grey"
              lead={<Brand />}
              toolbar={
                <span className="dl-tools">
                  <Button variant="ghost" size="small" icon={<Plus size={12} strokeWidth={2} />} data-target="add">
                    {layout === 'compact' ? false : 'Add URL'}
                  </Button>
                  <Button variant="ghost" size="small" className="dl-resume" icon={<Play size={12} strokeWidth={2} />} data-target="resume" data-lit={(s.sel && s.phase === 'paused') || undefined}>
                    {layout === 'compact' ? false : 'Resume'}
                  </Button>
                  <Button variant="ghost" size="small" icon={<Square size={12} strokeWidth={2} />} data-target="stop">
                    {layout === 'compact' ? false : 'Stop'}
                  </Button>
                </span>
              }
            >
              <div className="dl-body">
                {layout === 'wide' && <Pane />}
                <div className="dl-main">
                  {layout === 'wide' && (
                    <div className="dl-cols" aria-hidden>
                      <span>File name</span>
                      <span>Size</span>
                      <span>Progress</span>
                      <span>Speed</span>
                      <span>Time left</span>
                      <span>Status</span>
                    </div>
                  )}
                  <div className="dl-rows">
                    {s.added && (
                      <div className="dl-arrive">
                        <Row
                          layout={layout}
                          name={SUBJECT.name}
                          size={SUBJECT.size}
                          pct={pct}
                          phase={s.phase}
                          speed={s.phase === 'running' ? '12.6 MB/s' : '-'}
                          eta={s.phase === 'running' ? etaFor(pct) : '-'}
                          sel={s.sel}
                          target="row"
                        />
                      </div>
                    )}
                    {ROWS.map((r) => (
                      <Row key={r.name} layout={layout} name={r.name} size={r.size} pct={r.pct} phase={r.phase} speed={r.speed ?? '-'} eta={r.eta ?? '-'} />
                    ))}
                    {COMPLETED.map((r) => (
                      <Row key={r.name} layout={layout} name={r.name} size={r.size} pct={100} phase="done" speed="-" eta="-" />
                    ))}
                  </div>
                </div>
              </div>
            </Window>
            <Popover
              show={s.open}
              origin="top right"
              className="dl-add"
              style={{ left: popLeft, top: add.y + add.height + 10, width: popW }}
            >
              <Field value={s.url} placeholder="https://…" focused={s.open} box={{ 'data-target': 'url' }} />
              <div className="dl-add-foot">
                <Button variant="primary" icon={<Plus size={12} strokeWidth={2} />} data-target="confirm">
                  Add
                </Button>
              </div>
            </Popover>
          </>
        )
      }}
    </Stage>
  )
}

function Brand() {
  return (
    <span className="dl-brand">
      <img src="/images/app-icon.png" alt="" width={16} height={16} />
      <span className="dl-brand-name">B Download Manager</span>
    </span>
  )
}

function Pane() {
  return (
    <nav className="dl-pane" aria-hidden>
      <p className="dl-label">Downloads</p>
      <ul className="dl-group">
        {VIEWS.map((v) => (
          <li key={v.label} className="dl-item" data-active={v.view === 'all' || undefined}>
            <ViewGlyph view={v.view} />
            <span>{v.label}</span>
            <span className="dl-count">{v.count}</span>
          </li>
        ))}
      </ul>
      <p className="dl-label dl-label--sub">Categories</p>
      <ul className="dl-group">
        {CATEGORIES.map(({ label, Icon }) => (
          <li key={label} className="dl-item">
            <Icon size={15} strokeWidth={1.7} className="dl-cat-icon" />
            <span>{label}</span>
          </li>
        ))}
      </ul>
    </nav>
  )
}

function Row({ layout, name, size, pct, phase, speed, eta, sel, target }: { layout: Layout; name: string; size: string; pct: number; phase: S['phase']; speed: string; eta: string; sel?: boolean; target?: string }) {
  const status = (
    <span className="dl-status">
      <Chip kind={CHIP[phase]} />
      {LABEL[phase]}
    </span>
  )
  return (
    <div className="dl-row" data-phase={phase} data-sel={sel || undefined} data-target={target}>
      {layout === 'compact' ? (
        <>
          <div className="dl-top">
            <span className="dl-name">
              <img className="dl-file-icon" src={fileIcon(name)} alt="" width={20} height={20} />
              {name}
            </span>
            {status}
          </div>
          <div className="dl-bottom">
            <Track pct={pct} />
            <span className="dl-pct">{pct.toFixed(2)}%</span>
          </div>
        </>
      ) : (
        <>
          <span className="dl-name">
            <img className="dl-file-icon" src={fileIcon(name)} alt="" width={20} height={20} />
            {name}
          </span>
          <span className="dl-size">{size}</span>
          <span className="dl-prog">
            <Track pct={pct} />
            <span className="dl-pct">{pct.toFixed(2)}%</span>
          </span>
          <span className="dl-speed">{speed}</span>
          <span className="dl-eta">{eta}</span>
          {status}
        </>
      )}
    </div>
  )
}
function Track({ pct }: { pct: number }) {
  return (
    <span className="dl-track">
      <i className="dl-fill" style={{ width: `${pct}%`, backgroundSize: `${pct > 0 ? 10000 / pct : 100}% 100%` }} />
    </span>
  )
}

/** The app's round status chip: filled disc, bright ring, gloss, white glyph. */
function Chip({ kind }: { kind: 'paused' | 'downloading' | 'completed' }) {
  return (
    <svg className="dl-chip" viewBox="0 0 16 16" data-kind={kind} aria-hidden>
      <circle cx="8" cy="8" r="6.9" className="dl-chip-dep" />
      <circle cx="8" cy="8" r="6.2" fill="none" strokeWidth="0.9" className="dl-chip-ring" />
      <ellipse cx="8" cy="4.8" rx="4.4" ry="1.4" className="dl-chip-gloss" />
      {kind === 'paused' ? (
        <>
          <rect x="5.6" y="5.3" width="1.7" height="5.4" rx="0.5" fill="#fff" />
          <rect x="8.7" y="5.3" width="1.7" height="5.4" rx="0.5" fill="#fff" />
        </>
      ) : kind === 'completed' ? (
        <path d="m5.1 8.2 2 2 3.9-4.3" fill="none" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
      ) : (
        <>
          <path d="M8 4.4v6.2" stroke="#fff" strokeWidth="1.7" strokeLinecap="round" />
          <path d="m5.4 8.3 2.6 2.6 2.6-2.6" fill="none" stroke="#fff" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
        </>
      )}
    </svg>
  )
}

/** The pane's three view marks, drawn from the app's own 24-unit geometry. */
function ViewGlyph({ view }: { view: 'all' | 'unfinished' | 'finished' }) {
  return (
    <svg className="dl-view-icon" viewBox="0 0 24 24" data-view={view} aria-hidden>
      <g transform="translate(12 12) scale(1.2) translate(-12 -12)">
        {view === 'all' ? (
          <>
            <path d="M10.7 5.4h2.6v4.9h2.6L12 15.2l-3.9-4.9h2.6z" />
            <path d="M5.4 15.8h13.2v1.7a2 2 0 0 1-2 2H7.4a2 2 0 0 1-2-2z" />
          </>
        ) : view === 'unfinished' ? (
          <>
            <circle cx="12" cy="12" r="5.1" />
            <path d="M12 8.7V12l2.5 1.6" fill="none" strokeWidth="1.7" strokeLinecap="round" className="dl-view-deep" />
          </>
        ) : (
          <path d="m6.5 12.4 2.2-2.2 2.6 2.6 5.1-5.3 2.2 2.2-7.3 7.5z" />
        )}
      </g>
    </svg>
  )
}