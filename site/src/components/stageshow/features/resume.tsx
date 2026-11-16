import { Play } from 'lucide-react'
import { Button } from '../kit'
import { Fs, Label, Meter, Row } from './parts'

// Resume, not restart: the manifest's finished ranges are the green blocks;
// the next run picks up at the first hole, never at zero.
const BLOCKS = [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0]

export default function Resume() {
  return (
    <Fs
      name="resume"
      lead="product-shots-final.zip · Stopped"
      toolbar={
        <Button variant="primary" size="small" icon={<Play size={12} strokeWidth={2} />}>
          Resume
        </Button>
      }
    >
      <Label>Bytes on disk after the crash</Label>
      <div className="fs-blocks" aria-hidden>
        {BLOCKS.map((b, i) => (
          <i key={i} data-state={b === 1 ? 'done' : b === 2 ? 'live' : 'empty'} />
        ))}
      </div>
      <Row cols="minmax(0, 1fr) 150px 64px">
        <span className="fs-name">237 MB of 347 MB kept</span>
        <Meter pct={68.35} phase="paused" />
        <span className="fs-val">68.35%</span>
      </Row>
      <Label>The manifest beside the part files</Label>
      <Row>
        <span className="fs-name">Finished ranges</span>
        <span className="fs-val">14 · verified on open</span>
      </Row>
      <Row>
        <span className="fs-name">Next range</span>
        <span className="fs-val">237 – 253 MiB</span>
      </Row>
      <Row>
        <span className="fs-name">Overlaps and out-of-order writes</span>
        <span className="fs-val">Rejected</span>
      </Row>
      <Row faint>
        <span className="fs-name">FTP resumes the same job with a REST offset</span>
      </Row>
    </Fs>
  )
}