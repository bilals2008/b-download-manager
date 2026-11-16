import { Field, Segmented } from '../kit'
import { Fs, Label, Meter, Row } from './parts'

// 1.2.0 — encrypted name lookup, one network adapter, and every transfer
// shows its server and its round trip.
export default function Network() {
  return (
    <Fs name="network" lead="Network">
      <Label>Name lookup</Label>
      <Segmented items={['Encrypted', 'NextDNS', 'System']} value={1} />
      <div style={{ marginTop: 10 }}>
        <Field value="https://dns.nextdns.io/a1b2c3" />
      </div>
      <Label>Adapter</Label>
      <Row>
        <span className="fs-name">Bind downloads to</span>
        <span className="fs-val">Wi-Fi · Intel AX200</span>
      </Row>
      <Label>Live transfers</Label>
      <Row cols="minmax(0, 1fr) 120px 52px">
        <span className="fs-name">cdn.releases.ubuntu.com</span>
        <Meter pct={62.4} />
        <span className="fs-val">24 ms</span>
      </Row>
      <Row cols="minmax(0, 1fr) 120px 52px">
        <span className="fs-name">mirror.example.org</span>
        <Meter pct={38.2} />
        <span className="fs-val">61 ms</span>
      </Row>
      <Row cols="minmax(0, 1fr) 120px 52px" faint>
        <span className="fs-name">dl.example.net</span>
        <Meter pct={12.6} />
        <span className="fs-val">118 ms</span>
      </Row>
    </Fs>
  )
}