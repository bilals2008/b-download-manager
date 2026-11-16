import type { ReactNode } from 'react'
import { Stage, Toggle, Window } from '../kit'
import { APP_TOKENS } from '../app-tokens'
import './features.css'

// Shared furniture for the feature stills: one 640x400 window on the app's
// palette, bleeding off the bottom the way the real list does. Every scene
// is a still (no script), filmed at 2x for the features page.

export function Fs({ name, lead, toolbar, children, w = 640, h = 400, tokens }: { name: string; lead: string; toolbar?: ReactNode; children: ReactNode; w?: number; h?: number; tokens?: Record<string, string> }) {
  return (
    <Stage width={w} height={h} theme="dark" tokens={{ ...APP_TOKENS, ...tokens }} className={`fs fs-${name}`}>
      {() => (
        <Window
          className="fs-win"
          lights="grey"
          lead={
            <span className="fs-brand">
              <img src="/images/app-icon.png" alt="" width={16} height={16} />
              {lead}
            </span>
          }
          toolbar={toolbar}
        >
          <div className="fs-body">{children}</div>
        </Window>
      )}
    </Stage>
  )
}

export function Label({ children }: { children: ReactNode }) {
  return <p className="fs-label">{children}</p>
}

export function Row({ cols, children, faint, active }: { cols?: string; children: ReactNode; faint?: boolean; active?: boolean }) {
  return (
    <div className="fs-row" data-faint={faint || undefined} data-active={active || undefined} style={cols ? { gridTemplateColumns: cols } : undefined}>
      {children}
    </div>
  )
}

export function ToggleRow({ on, children, hint }: { on: boolean; children: ReactNode; hint?: string }) {
  return (
    <div className="fs-trow">
      <Toggle on={on} />
      <span>{children}</span>
      {hint && <span className="fs-hint">{hint}</span>}
    </div>
  )
}

export function Meter({ pct, phase = 'running' }: { pct: number; phase?: 'running' | 'paused' | 'done' }) {
  return (
    <span className="fs-track" data-phase={phase}>
      <i className="fs-fill" style={{ width: `${pct}%`, backgroundSize: `${pct > 0 ? 10000 / pct : 100}% 100%` }} />
    </span>
  )
}

export function FileIcon({ name }: { name: string }) {
  const n = name.toLowerCase()
  const src = n.endsWith('.iso')
    ? '/images/file-icons/disc.svg'
    : n.endsWith('.mp4') || n.endsWith('.mkv')
      ? '/images/file-icons/video.svg'
      : n.endsWith('.exe') || n.endsWith('.msi')
        ? '/images/file-icons/exe.svg'
        : n.endsWith('.pdf')
          ? '/images/file-icons/pdf.svg'
          : n.endsWith('.pptx') || n.endsWith('.ppt')
            ? '/images/file-icons/powerpoint.svg'
            : n.endsWith('.docx') || n.endsWith('.doc')
        ? '/images/file-icons/word.svg'
        : n.endsWith('.xlsx') || n.endsWith('.xls')
          ? '/images/file-icons/excel.svg'
          : '/images/file-icons/zip.svg'
  return <img className="fs-file-icon" src={src} alt="" width={18} height={18} />
}