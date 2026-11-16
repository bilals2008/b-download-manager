import { Field, Segmented, Toggle } from '../kit'
import { Fs, Label, Row, ToggleRow } from './parts'

// 1.2.0 — the queue runs on the days and hours you set, and powers the
// computer off only when you say so.
export default function Scheduler() {
  return (
    <Fs
      name="scheduler"
      lead="Queue schedule"
      toolbar={
        <span className="fs-trow" style={{ height: 'auto' }}>
          <Toggle on />
          Schedule on
        </span>
      }
    >
      <Label>Days</Label>
      <Segmented items={['Weekdays', 'Weekends', 'Every day']} value={0} />
      <Label>Hours</Label>
      <div className="fs-grid2">
        <Field label="From" value="09:00" />
        <Field label="To" value="18:00" />
      </div>
      <ToggleRow on hint="only when the queue is empty">
        Power off when the queue finishes
      </ToggleRow>
      <Label>Next runs</Label>
      <Row>
        <span className="fs-name">Today · 09:00 – 18:00</span>
        <span className="fs-val">in 3 h</span>
      </Row>
      <Row>
        <span className="fs-name">Tomorrow · 09:00 – 18:00</span>
        <span className="fs-val">queued</span>
      </Row>
      <Row faint>
        <span className="fs-name">Saturday · outside the window</span>
        <span className="fs-val">skipped</span>
      </Row>
    </Fs>
  )
}