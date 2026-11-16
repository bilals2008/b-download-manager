import { Field, Segmented } from '../kit'
import { Fs, Label, Meter, Row } from './parts'

// A ceiling you choose: one cap per download, one for the whole application,
// applied across every open connection.
export default function Limits() {
  return (
    <Fs name="limits" lead="Speed limiter">
      <Label>This download</Label>
      <div className="fs-grid2">
        <Field value="4.0" />
        <Segmented items={['KB/s', 'MB/s']} value={1} />
      </div>
      <Label>Whole application</Label>
      <div className="fs-grid2">
        <Field value="12.0" />
        <Segmented items={['KB/s', 'MB/s']} value={1} />
      </div>
      <Label>Right now</Label>
      <Row cols="minmax(0, 1fr) 150px 64px">
        <span className="fs-name">6 connections · 1.6 MB/s each</span>
        <Meter pct={80} />
        <span className="fs-val">9.6 MB/s</span>
      </Row>
      <Row faint>
        <span className="fs-name">The cap holds across every open connection, not per socket</span>
      </Row>
    </Fs>
  )
}