import { Plus } from 'lucide-react'
import { Button, Check } from '../kit'
import { FileIcon, Fs, Label, Meter, Row, ToggleRow } from './parts'

// 1.2.1 — magnets and .torrent files: pick the files, then choose how long
// the finished torrent keeps sharing.
export default function Torrents() {
  return (
    <Fs
      name="torrents"
      lead="ubuntu-24.04.torrent"
      toolbar={
        <Button variant="ghost" size="small" icon={<Plus size={12} strokeWidth={2} />}>
          Add torrent
        </Button>
      }
    >
      <Label>Files in this torrent</Label>
      <Row cols="16px minmax(0, 1fr) 74px 120px 64px">
        <Check on />
        <span className="fs-name">
          <FileIcon name="ubuntu-24.04.iso" />
          ubuntu-24.04.iso
        </span>
        <span className="fs-val">5.8 GB</span>
        <Meter pct={62.4} />
        <span className="fs-val">62.40%</span>
      </Row>
      <Row cols="16px minmax(0, 1fr) 74px 120px 64px">
        <Check on />
        <span className="fs-name">
          <FileIcon name="SHA256SUMS.txt" />
          SHA256SUMS
        </span>
        <span className="fs-val">4 KB</span>
        <Meter pct={100} phase="done" />
        <span className="fs-val">Done</span>
      </Row>
      <Row cols="16px minmax(0, 1fr) 74px 120px 64px">
        <Check on={false} />
        <span className="fs-name">
          <FileIcon name="samples.mkv" />
          extras/samples.mkv
        </span>
        <span className="fs-val">1.2 GB</span>
        <Meter pct={0} />
        <span className="fs-val">Skipped</span>
      </Row>
      <Row cols="16px minmax(0, 1fr) 74px 120px 64px">
        <Check on />
        <span className="fs-name">
          <FileIcon name="release-notes.pdf" />
          docs/release-notes.pdf
        </span>
        <span className="fs-val">2.1 MB</span>
        <Meter pct={100} phase="done" />
        <span className="fs-val">Done</span>
      </Row>
      <div className="fs-foot">
        <ToggleRow on>Keep sharing</ToggleRow>
        <span className="fs-hint">stops at ratio 2.0 · upload 1.2 MB/s</span>
      </div>
      <Row faint>
        <span className="fs-name">14 seeds · 3 leeches · pieces verified as they land</span>
      </Row>
    </Fs>
  )
}