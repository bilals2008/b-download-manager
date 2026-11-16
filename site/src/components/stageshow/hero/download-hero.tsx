import { AppWindow, FileArchive, FileText, Film, Folder, Gamepad2, Music, Package, Play, Plus, Square } from 'lucide-react'
import { Bar, Button, Stage, Window, type Act, type Layout } from '../kit'
import './hero.css'

// B Download Manager as it looks on the landing page hero: a paused transfer
// is picked up again. Beats: the pointer clicks the paused row, Resume lights
// lime in the toolbar, the click on it turns the row to Downloading with its
// speed and time left, the meter walks to the end, the row lands Completed
// with the green check, and the pointer steps off. Two seconds on the result.
//
// What makes it read as this app:
// - One canvas, one colour (#0b1416). Panes and the toolbar find their edge
//   with a hairline, never a second shade; the stage plate is one step darker,
//   which is the only depth a dark window gets on a light page.
// - The meter is the app's: emerald through cyan to blue while it runs, a held
//   blue when paused (it never turns grey), green when done. Its gradient spans
//   the whole track, so the colours do not shift as the fill grows.
// - The status chip is the app's own geometry: filled disc, bright ring, gloss,
//   white glyph. It swaps in place on the same row; the words change beside it.
// - Real strings: the file name, its size, the speed and the time left. The
//   rows under it are placeholder at the app's row height, and the list runs
//   off the bottom edge the way the window does.

type S = { sel: boolean; phase: 'paused' | 'running' | 'done'; pct: number }

const initial: S = { sel: false, phase: 'paused', pct: 62.4 }

async function script(a: Act<S>) {
  await a.wait(700)
  await a.move('row')
  await a.click('row')
  a.put({ sel: true })
  a.mark('selected')
  await a.wait(560)
  await a.move('resume')
  await a.click('resume')
  a.put({ phase: 'running' })
  a.mark('running')
  for (const pct of [71.2, 80.6, 90.1, 97.3]) {
    await a.wait(560)
    a.put({ pct })
  }
  await a.wait(520)
  a.put({ phase: 'done', pct: 100 })
  a.mark('done')
  await a.wait(900)
  await a.move({ wide: { x: 1044, y: 470 }, compact: { x: 320, y: 336 } })
  await a.wait(2000)
}

const TOKENS = {
  '--ss-canvas': '#0b1416',
  '--ss-surface': '#0b1416',
  '--ss-chrome': '#0b1416',
  '--ss-line': '#13312a',
  '--ss-line-faint': '#102923',
  '--ss-ink-1': '#0f1c19',
  '--ss-ink-2': '#11201f',
  '--ss-ink-3': '#16302b',
  '--ss-ink-4': '#1a3a33',
  '--ss-ink-5': '#20443c',
  '--ss-fill': '#16302b',
  '--ss-text': '#869a93',
  '--ss-text-strong': '#d7e3df',
  '--ss-text-faint': '#5f726c',
  '--ss-icon': '#93a7a0',
  '--ss-icon-faint': '#4b5f59',
  '--ss-accent': '#cdf25a',
  '--ss-accent-2': '#cdf25a',
  '--ss-accent-soft': 'rgb(205 242 90 / 0.14)',
  '--ss-accent-text': '#cdf25a',
  '--ss-success': '#2fd06b',
  '--ss-radius-sm': '8px',
  '--ss-radius-md': '8px',
  '--ss-radius-lg': '10px',
  '--ss-radius-window': '12px',
  '--ss-font': '"Instrument Sans", ui-sans-serif, system-ui, sans-serif',
  '--ss-shadow-window': '0 0 0 1px rgb(255 255 255 / 0.07), 0 30px 70px -30px rgb(0 0 0 / 0.55)',
}

const SUBJECT = { name: 'ubuntu-24.04.1-desktop-amd64.iso', size: '5.8 GB' }

const ROWS = [
  { name: 'blender-4.2.1-windows-x64.msi', size: '342 MB', pct: 100, phase: 'done' as const },
  { name: 'sundance-2026-keynote-1080p.mp4', size: '1.4 GB', pct: 38.2, phase: 'running' as const, speed: '4.2 MB/s', eta: '2 min 14 s' },
  { name: 'godot-4.4-stable-win64.exe', size: '118 MB', pct: 100, phase: 'done' as const },
]

const FILL = [
  { name: 208, pct: 46 },
  { name: 164, pct: 12 },
  { name: 186, pct: 73 },
  { name: 138, pct: 28 },
  { name: 196, pct: 58 },
  { name: 152, pct: 35 },
]

const LABEL = { paused: 'Paused', running: 'Downloading', done: 'Completed' } as const
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
      tokens={TOKENS}
      initial={initial}
      poster={{ sel: true, phase: 'done', pct: 100 }}
      script={script}
      play="once"
      fill
      cursor={{ at: { wide: { x: 1044, y: 470 }, compact: { x: 300, y: 300 } }, idle: 3200 }}
      className="dl"
    >
      {(s, _rect, layout) => {
        const pct = s.phase === 'done' ? 100 : s.pct
        return (
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
                  <Row
                    layout={layout}
                    name={SUBJECT.name}
                    size={SUBJECT.size}
                    pct={pct}
                    phase={s.phase}
                    speed={s.phase === 'running' ? '4.2 MB/s' : '-'}
                    eta={s.phase === 'running' ? '2 min 14 s' : '-'}
                    sel={s.sel}
                    target="row"
                  />
                  {ROWS.map((r) => (
                    <Row key={r.name} layout={layout} name={r.name} size={r.size} pct={r.pct} phase={r.phase} speed={r.speed ?? '-'} eta={r.eta ?? '-'} />
                  ))}
                  {FILL.map((f, i) => (
                    <Fill key={i} layout={layout} name={f.name} pct={f.pct} />
                  ))}
                </div>
              </div>
            </div>
          </Window>
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
            <span className="dl-name">{name}</span>
            {status}
          </div>
          <div className="dl-bottom">
            <Track pct={pct} />
            <span className="dl-pct">{pct.toFixed(2)}%</span>
          </div>
        </>
      ) : (
        <>
          <span className="dl-name">{name}</span>
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

function Fill({ layout, name, pct }: { layout: Layout; name: number; pct: number }) {
  const track = (
    <span className="dl-track">
      <i className="dl-fill dl-fill--filler" style={{ width: `${pct}%` }} />
    </span>
  )
  return (
    <div className="dl-row dl-row--fill">
      {layout === 'compact' ? (
        <>
          <div className="dl-top">
            <span className="dl-name">
              <Bar w={name} h={7} tone={2} />
            </span>
            <Bar w={56} h={14} r={7} tone={1} />
          </div>
          <div className="dl-bottom">
            {track}
            <Bar w={38} h={7} tone={1} />
          </div>
        </>
      ) : (
        <>
          <span className="dl-name">
            <Bar w={name} h={7} tone={2} />
          </span>
          <span className="dl-size">
            <Bar w={34} h={7} tone={1} />
          </span>
          <span className="dl-prog">
            {track}
            <span className="dl-pct">
              <Bar w={38} h={7} tone={1} />
            </span>
          </span>
          <span className="dl-speed">
            <Bar w={44} h={7} tone={1} />
          </span>
          <span className="dl-eta">
            <Bar w={34} h={7} tone={1} />
          </span>
          <span className="dl-status">
            <Bar w={58} h={14} r={7} tone={1} />
          </span>
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
