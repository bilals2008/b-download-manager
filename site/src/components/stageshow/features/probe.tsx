import { Plus } from 'lucide-react'
import { Button, Field } from '../kit'
import { Fs, Label, Row } from './parts'

// Range requests where they work: the probe reports size, resumability and
// the folder before anything is written.
export default function Probe() {
  return (
    <Fs
      name="probe"
      lead="Add URL"
      toolbar={
        <Button variant="primary" size="small" icon={<Plus size={12} strokeWidth={2} />}>
          Add
        </Button>
      }
    >
      <Field label="Link" value="ftp://mirror.example.org/pub/ubuntu-24.04.iso" />
      <Label>What the server answered</Label>
      <Row>
        <span className="fs-name">Size</span>
        <span className="fs-val">5.8 GB · probed, not guessed</span>
      </Row>
      <Row>
        <span className="fs-name">Resumable</span>
        <span className="fs-val" data-tone="done">Yes · ranges accepted</span>
      </Row>
      <Row>
        <span className="fs-name">Protocol</span>
        <span className="fs-val">FTP · REST offset per range</span>
      </Row>
      <Label>Where it lands</Label>
      <Row>
        <span className="fs-name">Category</span>
        <span className="fs-val">General</span>
      </Row>
      <Row>
        <span className="fs-name">Folder</span>
        <span className="fs-val">Downloads / General</span>
      </Row>
      <Row faint>
        <span className="fs-name">A plain 200 answer falls back to one connection</span>
      </Row>
    </Fs>
  )
}