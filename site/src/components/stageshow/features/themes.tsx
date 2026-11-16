import { RotateCcw } from 'lucide-react'
import { Button } from '../kit'
import { Fs, Label, Meter, Row } from './parts'

// 1.1.0 — ten appearances with a swatch each; picking one repaints every
// window at once. The pane is drawn in Bloom, the light side of the palette,
// with the app's own token values.
const BLOOM = {
  '--ss-canvas': '#fff8f3',
  '--ss-surface': '#fbf2eb',
  '--ss-chrome': '#fff8f3',
  '--ss-line': 'rgb(31 27 22 / 0.1)',
  '--ss-line-faint': 'rgb(31 27 22 / 0.07)',
  '--ss-ink-2': '#f5ece4',
  '--ss-ink-3': '#f5ece4',
  '--ss-fill': '#f5ece4',
  '--ss-text': 'rgb(31 27 22 / 0.62)',
  '--ss-text-strong': '#1f1b16',
  '--ss-text-faint': 'rgb(31 27 22 / 0.45)',
  '--ss-icon': 'rgb(31 27 22 / 0.7)',
  '--ss-accent': '#8a5100',
  '--ss-accent-2': '#8a5100',
  '--ss-accent-soft': 'rgb(138 81 0 / 0.12)',
  '--ss-accent-text': '#8a5100',
  '--ss-success': '#15803d',
  '--ss-shadow-window': '0 0 0 1px rgb(31 27 22 / 0.08), 0 34px 64px -26px rgb(31 27 22 / 0.28)',
}

const THEMES = [
  { name: 'Auto', swatch: 'linear-gradient(135deg, #f3f4f6 50%, #0b1416 50%)' },
  { name: 'Light', swatch: '#022c1e' },
  { name: 'Bloom', swatch: '#8a5100', on: true },
  { name: 'Dark', swatch: '#c8ff00' },
  { name: 'Cyberpunk', swatch: '#00f0ff' },
  { name: 'Volt', swatch: '#c8ff00' },
  { name: 'Soft Black', swatch: '#e9e9e6' },
  { name: 'Ayu Dark', swatch: '#e6b450' },
  { name: 'Graphite', swatch: '#e8eaed' },
  { name: 'Hive', swatch: '#ffb95f' },
]

export default function Themes() {
  return (
    <Fs
      name="themes"
      lead="Appearance"
      w={1200}
      h={420}
      tokens={BLOOM}
      toolbar={
        <Button variant="ghost" size="small" icon={<RotateCcw size={12} strokeWidth={2} />}>
          Reset
        </Button>
      }
    >
      <div className="fs-themes-body">
        <div className="fs-themes-pick">
          <Label>Theme</Label>
          <div className="fs-swatches">
            {THEMES.map((t) => (
              <span className="fs-swatch" data-on={t.on || undefined} key={t.name}>
                <i style={{ background: t.swatch }} />
                {t.name}
              </span>
            ))}
          </div>
          <Label>Typography</Label>
          <Row>
            <span className="fs-name">Face</span>
            <span className="fs-val">Instrument Sans</span>
          </Row>
          <Row>
            <span className="fs-name">Weight</span>
            <span className="fs-val">Regular</span>
          </Row>
        </div>
        <div className="fs-themes-preview">
          <Label>Preview</Label>
          <Row cols="minmax(0, 1fr) 150px 56px">
            <span className="fs-name">ubuntu-24.04.iso</span>
            <Meter pct={62.4} />
            <span className="fs-val">62.40%</span>
          </Row>
          <Row cols="minmax(0, 1fr) 150px 56px">
            <span className="fs-name">blender-4.2.1-windows-x64.msi</span>
            <Meter pct={100} phase="done" />
            <span className="fs-val">Done</span>
          </Row>
          <Row cols="minmax(0, 1fr) 150px 56px">
            <span className="fs-name">sundance-2026-keynote-1080p.mp4</span>
            <Meter pct={38.2} phase="paused" />
            <span className="fs-val">Paused</span>
          </Row>
          <Row cols="minmax(0, 1fr) 150px 56px">
            <span className="fs-name">annual-report-2026.pdf</span>
            <Meter pct={100} phase="done" />
            <span className="fs-val">Done</span>
          </Row>
          <Row cols="minmax(0, 1fr) 150px 56px">
            <span className="fs-name">product-shots-final.zip</span>
            <Meter pct={12.6} />
            <span className="fs-val">12.60%</span>
          </Row>
          <Row faint>
            <span className="fs-name">Every window follows at once — tray and browser panel too</span>
          </Row>
        </div>
      </div>
    </Fs>
  )
}