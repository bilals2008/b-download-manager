import { Folder, RefreshCw } from 'lucide-react'
import { Button } from '../kit'
import { FileIcon, Fs, Row } from './parts'

// 1.2.0 — Drive files land as files, documents as PDF or Word, and image
// boards as a folder of pictures.
export default function Drive() {
  return (
    <Fs
      name="drive"
      lead="Google Drive"
      toolbar={
        <Button variant="ghost" size="small" icon={<RefreshCw size={12} strokeWidth={2} />}>
          Refresh
        </Button>
      }
    >
      <div className="fs-cols" style={{ gridTemplateColumns: 'minmax(0, 1fr) 150px' }}>
        <span>Name</span>
        <span>Saves as</span>
      </div>
      <Row cols="minmax(0, 1fr) 150px">
        <span className="fs-name">
          <FileIcon name="Q3 report.docx" />
          Q3 report.docx
        </span>
        <span className="fs-val">Word document</span>
      </Row>
      <Row cols="minmax(0, 1fr) 150px">
        <span className="fs-name">
          <FileIcon name="Launch notes.docx" />
          Launch notes.docx
        </span>
        <span className="fs-val">PDF</span>
      </Row>
      <Row cols="minmax(0, 1fr) 150px">
        <span className="fs-name">
          <FileIcon name="hero-render.mp4" />
          hero-render.mp4
        </span>
        <span className="fs-val">File · 1.4 GB</span>
      </Row>
      <Row cols="minmax(0, 1fr) 150px">
        <span className="fs-name">
          <Folder size={18} strokeWidth={1.7} className="fs-folder" />
          shoots-board
        </span>
        <span className="fs-val">Folder · 24 pictures</span>
      </Row>
      <Row cols="minmax(0, 1fr) 150px">
        <span className="fs-name">
          <FileIcon name="budget-2026.zip" />
          budget-2026.xlsx
        </span>
        <span className="fs-val">File · 318 KB</span>
      </Row>
      <Row cols="minmax(0, 1fr) 150px" faint>
        <span className="fs-name">
          <FileIcon name="standup-notes.pdf" />
          standup-notes.pdf
        </span>
        <span className="fs-val">File · 96 KB</span>
      </Row>
    </Fs>
  )
}