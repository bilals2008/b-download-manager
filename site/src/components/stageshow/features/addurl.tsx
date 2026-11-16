import { Plus } from 'lucide-react'
import { Button, Field } from '../kit'
import { Fs, Row, ToggleRow } from './parts'

// 1.2.0 small stuff — Add URL starts straight away, and archives unpack when
// they finish, asking for a password only when one is needed.
export default function AddUrl() {
  return (
    <Fs
      name="addurl"
      lead="Add URL"
      toolbar={
        <Button variant="primary" size="small" icon={<Plus size={12} strokeWidth={2} />}>
          Add
        </Button>
      }
    >
      <Field label="Link" value="https://cdn.example.com/product-shots-final.zip" />
      <ToggleRow on hint="no second confirm">Start straight away</ToggleRow>
      <ToggleRow on>Unpack when it finishes</ToggleRow>
      <Field label="Archive password" password value="hunter2secret" />
      <Row faint>
        <span className="fs-name">Unpacks into Downloads / Compressed</span>
      </Row>
      <Row faint>
        <span className="fs-name">A password is asked only when the archive needs one</span>
      </Row>
    </Fs>
  )
}