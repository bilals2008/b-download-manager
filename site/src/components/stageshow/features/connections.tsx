import { Fs, Label, Meter, Row } from './parts'

// The engine story: one file cut into byte ranges, each on its own
// connection, with the per-site and global budget above the list.
export default function Connections() {
  return (
    <Fs name="connections" lead="ubuntu-24.04.iso · Connections">
      <Row cols="minmax(0, 1fr) 150px 64px">
        <span className="fs-name">237 MB of 5.8 GB on disk</span>
        <Meter pct={68.35} />
        <span className="fs-val">68.35%</span>
      </Row>
      <Label>Open connections · 6 of 32</Label>
      <div className="fs-cols" style={{ gridTemplateColumns: '24px minmax(0, 1fr) 84px 96px' }}>
        <span>#</span>
        <span>Range</span>
        <span>Downloaded</span>
        <span>Status</span>
      </div>
      <Row cols="24px minmax(0, 1fr) 84px 96px">
        <span className="fs-val">1</span>
        <span className="fs-name">0 – 16 MiB</span>
        <span className="fs-val">16.0 MB</span>
        <span className="fs-val" data-tone="done">Complete</span>
      </Row>
      <Row cols="24px minmax(0, 1fr) 84px 96px">
        <span className="fs-val">2</span>
        <span className="fs-name">16 – 32 MiB</span>
        <span className="fs-val">16.0 MB</span>
        <span className="fs-val" data-tone="done">Complete</span>
      </Row>
      <Row cols="24px minmax(0, 1fr) 84px 96px">
        <span className="fs-val">3</span>
        <span className="fs-name">32 – 48 MiB</span>
        <span className="fs-val">11.2 MB</span>
        <span className="fs-val" data-tone="run">4.2 MB/s</span>
      </Row>
      <Row cols="24px minmax(0, 1fr) 84px 96px">
        <span className="fs-val">4</span>
        <span className="fs-name">48 – 64 MiB</span>
        <span className="fs-val">9.7 MB</span>
        <span className="fs-val" data-tone="run">4.2 MB/s</span>
      </Row>
      <Row cols="24px minmax(0, 1fr) 84px 96px">
        <span className="fs-val">5</span>
        <span className="fs-name">64 – 80 MiB</span>
        <span className="fs-val">0 MB</span>
        <span className="fs-val">Queued</span>
      </Row>
      <Row cols="24px minmax(0, 1fr) 84px 96px" faint>
        <span className="fs-val">6</span>
        <span className="fs-name">80 – 96 MiB</span>
        <span className="fs-val">0 MB</span>
        <span className="fs-val">Queued</span>
      </Row>
    </Fs>
  )
}