# Toolbars

The button bar is a **toolbar pack** plus a **layout**. The pack decides how
the bar looks and may bring its own icons; the layout decides which buttons it
shows, in which order. Both are local: nothing is downloaded to install a pack.

Open **View > Toolbar**, or **Settings > Toolbar** for the same choices with a
live preview:

| Row | What it does |
| --- | --- |
| Customize... | Move buttons on and off the bar, reorder them, add separators, reset |
| Large Buttons / Small Buttons | Icon size; remembered per machine |
| Look for new toolbars... | Install, replace or remove a pack; open the folder; rescan |
| Default / 3D Style / Classic / Isometric / Blueprint / installed packs | Switch the bar. Each pack keeps its own layout |

**Settings > Toolbar** repeats the style and size choices and adds a **Show
text labels** switch; with the labels off the buttons narrow to their icons.
Both the menu and the page open the same Customize and Manage dialogs. Manage
is only for packs in the folder; the built-in styles live in the menu and the
Settings style list.

**Default** keeps the application's green-and-red action icons. **3D Style**
draws glossy folders, globes, gears and an alarm clock with bevels and
highlights. **Classic** draws the same motifs flat, with dark outlines.
**Isometric** turns every body into a chunky slab with a thick ink outline,
painted in riso inks. **Blueprint** is the same isometric geometry in one blue
hue at six lightnesses. All four sets are original vector drawings for this
application: no bitmap from another download manager is bundled.

## Where packs live

The desktop app reads `toolbars/*.json` from its data folder:

- Windows: `%APPDATA%\com.bilal.bdownloadmanager\toolbars`
- macOS: `~/Library/Application Support/com.bilal.bdownloadmanager/toolbars`
- Linux: `~/.local/share/com.bilal.bdownloadmanager/toolbars`

**Look for new toolbars... > Open toolbars folder** creates the folder and
opens it. Drop a `.json` file there and choose **Rescan**, or install it with
**Install from file...**. Installing stores the file as `<id>.json`; replacing
an existing id asks first.

## File format

A pack is one JSON object. `id` and `name` are required; everything else is
optional. Unknown fields are ignored.

```json
{
  "format": 1,
  "id": "my-toolbar",
  "name": "My Toolbar",
  "version": "1.0.0",
  "author": "Your name",
  "description": "A short sentence shown in the toolbars window.",
  "style": {
    "background": "#0b1416",
    "text": "#d7e3df",
    "accent": "#7f92cc",
    "hover": "#11201f",
    "active": "#16302b",
    "line": "#13312a",
    "glow": false
  },
  "buttons": [
    "add-download",
    "resume",
    "stop",
    "separator",
    "options"
  ],
  "icons": {
    "add-download": "<svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"#56d364\" stroke-width=\"2\" stroke-linecap=\"round\"><path d=\"M12 5v14M5 12h14\"/></svg>"
  }
}
```

### Fields

| Field | Rules |
| --- | --- |
| `format` | Must be `1` (the default when the field is absent) |
| `id` | Lower-case letters, digits, `.`, `-`, `_`; up to 48 characters. `default`, `3d`, `classic`, `isometric` and `blueprint` are reserved |
| `name` | Required, up to 48 characters |
| `version` | Optional, up to 24 characters |
| `author` | Optional, up to 64 characters |
| `description` | Optional, up to 240 characters |
| `style` | Optional. Each colour is `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` |
| `style.glow` | `true` lights the icons and labels with `style.accent` |
| `buttons` | Optional order; see the command list below. `"separator"` groups buttons |
| `icons` | Optional; raw inline SVG, keyed by command |

A pack without `buttons` keeps the application's standard layout. A pack with
`buttons` starts every new profile with that order; each reader's later
Customize edits are remembered per pack.

A pack repaints the bar and may replace icons; it cannot add a command the
application does not already have.

### Commands

`add-download`, `resume`, `stop`, `stop-all`, `delete`, `delete-completed`,
`options`, `scheduler`, `start-queue`, `stop-queue`, `tell-friend`.

Unknown commands are dropped. A command may appear once; separators are
collapsed at the edges and between each other.

### Icons

- Raw `<svg>` markup, at most 16 KB per icon and 32 icons per pack.
- The root needs a `viewBox`; if it has none, `width` and `height` are used to
  make one.
- `width`/`height` are removed so the button size decides the drawing size.
- Scripts, `<foreignObject>`, embedded images, `<style>` and anything with an
  event handler, `javascript:` or an external `href`/`url()` are stripped or
  refused.
- Icons are drawn through `<img>`, never inserted as markup.
- An icon that the window cannot read is left out, and that button keeps the
  built-in drawing.

## Limits and failure handling

A file over 512 KB, invalid JSON, an unknown button name, a bad colour or a
hostile icon is refused with a sentence in the toolbars window. A file that is
already in the folder but broken appears under **Files that could not be used**
and the rest of the folder still loads. Duplicate ids keep the first file and
report the second.

Removing a pack deletes its file from the folder. Built-in bars cannot be
removed. Removing the pack in use switches back to Default.
