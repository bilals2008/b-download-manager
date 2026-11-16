import { Plus } from 'lucide-react'
import { Button, Check } from '../kit'
import { Fs, Label, Meter, Row } from './parts'

// 1.2.0 — Real-Debrid, AllDebrid and Premiumize unlock the host links they
// take, at their speed instead of the free tier's.
export default function Debrid() {
  return (
    <Fs
      name="debrid"
      lead="Host accounts"
      toolbar={
        <Button variant="ghost" size="small" icon={<Plus size={12} strokeWidth={2} />}>
          Add account
        </Button>
      }
    >
      <Row cols="16px minmax(0, 1fr) auto">
        <Check on />
        <span className="fs-name">Real-Debrid</span>
        <span className="fs-val">Unlocked · 214 links today</span>
      </Row>
      <Row cols="16px minmax(0, 1fr) auto">
        <Check on />
        <span className="fs-name">AllDebrid</span>
        <span className="fs-val">Unlocked · 38 links today</span>
      </Row>
      <Row cols="16px minmax(0, 1fr) auto">
        <Check on={false} />
        <span className="fs-name">Premiumize</span>
        <span className="fs-val">Not connected</span>
      </Row>
      <Label>Host links</Label>
      <Row cols="minmax(0, 1fr) 120px 74px">
        <span className="fs-name">rapidgator.net/file/8f2c1/setup.exe</span>
        <Meter pct={46.8} />
        <span className="fs-val">12.4 MB/s</span>
      </Row>
      <Row cols="minmax(0, 1fr) 120px 74px">
        <span className="fs-name">uploaded.net/file/9d3e0/pack.7z</span>
        <Meter pct={0} phase="paused" />
        <span className="fs-val">Queued</span>
      </Row>
      <Row cols="minmax(0, 1fr) 120px 74px" faint>
        <span className="fs-name">1fichier.com/file/4b7a2/assets.7z</span>
        <Meter pct={0} phase="paused" />
        <span className="fs-val">Queued</span>
      </Row>
    </Fs>
  )
}