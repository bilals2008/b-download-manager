import { Segmented } from '../kit'
import { Fs, Label, Meter, Row } from './parts'

// 1.2.0 — the window reads in four languages, and Arabic reads right to left.
export default function Language() {
  return (
    <Fs name="language" lead="Appearance">
      <Label>Language</Label>
      <Segmented items={['English', 'Français', 'Deutsch', 'العربية']} value={3} />
      <Label>Preview</Label>
      <div className="fs-rtl" dir="rtl">
        <Row cols="minmax(0, 1fr) 28px" active>
          <span className="fs-name">كل التنزيلات</span>
          <span className="fs-val">16</span>
        </Row>
        <Row cols="minmax(0, 1fr) 28px">
          <span className="fs-name">غير المكتملة</span>
          <span className="fs-val">2</span>
        </Row>
        <Row cols="minmax(0, 1fr) 28px">
          <span className="fs-name">المكتملة</span>
          <span className="fs-val">14</span>
        </Row>
        <Row cols="minmax(0, 1fr) 110px 52px">
          <span className="fs-name" dir="ltr">ubuntu-24.04.iso</span>
          <Meter pct={62.4} />
          <span className="fs-val">62.40%</span>
        </Row>
      </div>
      <Row faint>
        <span className="fs-name">Menus, dates and numbers follow the same language</span>
      </Row>
    </Fs>
  )
}