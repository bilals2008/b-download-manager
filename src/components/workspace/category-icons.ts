import type { ComponentType } from "react";
import {
  Archive,
  FileText,
  Film,
  Folder,
  Gamepad2,
  Music,
  Package,
  Shapes,
  type LucideProps,
} from "lucide-react";

import type { CategoryName } from "@/lib/downloads";

type Icon = ComponentType<LucideProps>;

interface CategoryIcon {
  icon: Icon;
  /** Token colour for the folder glyph. Decorative; the label carries the meaning. */
  tint: string;
}

export const CATEGORY_ICONS: Record<CategoryName, CategoryIcon> = {
  General: { icon: Folder, tint: "text-chrome-muted" },
  Video: { icon: Film, tint: "text-icon-red-bright" },
  Music: { icon: Music, tint: "text-icon-purple-bright" },
  Documents: { icon: FileText, tint: "text-icon-amber" },
  Programs: { icon: Package, tint: "text-icon-green-bright" },
  Games: { icon: Gamepad2, tint: "text-icon-blue-bright" },
  Compressed: { icon: Archive, tint: "text-icon-silver" },
  Other: { icon: Shapes, tint: "text-icon-silver" },
};
