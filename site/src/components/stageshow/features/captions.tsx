import { Download } from 'lucide-react'
import { Button, Check, Segmented } from '../kit'
import { Fs, Label, Row, ToggleRow } from './parts'

// 1.2.0 — pick the captions a page offers, or save just the sound of a video
// as MP3, Opus or FLAC.
export default function Captions() {
  return (
    <Fs
      name="captions"
      lead="sundance-2026-keynote-1080p.mp4"
      toolbar={
        <Button variant="ghost" size="small" icon={<Download size={12} strokeWidth={2} />}>
          Save
        </Button>
      }
    >
      <Label>Captions on this page</Label>
      <Row cols="16px minmax(0, 1fr) auto">
        <Check on />
        <span className="fs-name">English</span>
        <span className="fs-val">SRT · 41 KB</span>
      </Row>
      <Row cols="16px minmax(0, 1fr) auto">
        <Check on />
        <span className="fs-name">Français</span>
        <span className="fs-val">SRT · 44 KB</span>
      </Row>
      <Row cols="16px minmax(0, 1fr) auto">
        <Check on={false} />
        <span className="fs-name">العربية</span>
        <span className="fs-val">SRT · 52 KB</span>
      </Row>
      <Label>Save the sound as</Label>
      <Segmented items={['MP3', 'Opus', 'FLAC']} value={0} />
      <ToggleRow on hint="tags and cover kept from the page">
        Sound only, skip the video
      </ToggleRow>
      <Row faint>
        <span className="fs-name">Captions save beside the video, under the same name</span>
      </Row>
    </Fs>
  )
}