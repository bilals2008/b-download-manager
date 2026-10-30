---
title: Toolbar packs
description: Change the button bar, install a custom toolbar from a JSON file, or make one yourself.
---

The button bar under the menu can change its look and its buttons. Five styles
ship with the app — **Default**, **3D Style**, **Classic**, **Isometric** and
**Blueprint** — and you can add more from a toolbar pack file.

A toolbar pack is one JSON file. It can pick the buttons, choose the colours
and replace any icon with your own SVG. Nothing is downloaded from the
internet: the app only reads the file you choose.

## Switch the style

Open **View → Toolbar** and pick a style. The choice is remembered.

**Settings → Toolbar** has the same styles, plus:

- **Large buttons** / small buttons, for more room in the download list.
- **Show text labels**, which narrows every button to its icon. The tooltip
  still names the button.
- **Customize...**, to choose the buttons, reorder them and add separators.
- **Manage toolbars...**, to install or remove extra packs.

Each style keeps its own button order. Switching to another style and back
does not undo your customization.

## Install a custom toolbar

1. Open **View → Toolbar → Look for new toolbars...**.
2. Choose **Install from file...** and pick the `.json` file.
3. The pack appears in the style list at once.

If a pack with the same id is already installed, the app asks before replacing
it.

You can also copy the file into the toolbars folder yourself and choose
**Rescan** in that window. **Open toolbars folder** takes you there.

### Where the folder is

- **Windows:** `%APPDATA%\com.bilal.bdownloadmanager\toolbars`
- **macOS:** `~/Library/Application Support/com.bilal.bdownloadmanager\toolbars`
- **Linux:** `~/.local/share/com.bilal.bdownloadmanager\toolbars`

The folder is created the first time the app starts. The folder window only
shows packs you installed; the five built-in styles live in **View → Toolbar**
and **Settings → Toolbar**.

## Make a toolbar pack

The smallest useful pack only needs an id, a name and the buttons to show:

```json
{
  "id": "quiet",
  "name": "Quiet",
  "description": "Only the buttons I use.",
  "buttons": ["add-download", "resume", "stop", "separator", "delete"]
}
```

Save it as `quiet.json` and install it from the toolbars window. A pack without
`buttons` keeps the standard layout. Without `style`, the bar follows the
theme.

### A full example

```json
{
  "format": 1,
  "id": "my-toolbar",
  "name": "My Toolbar",
  "version": "1.0.0",
  "author": "Your name",
  "description": "A short line shown in the toolbar list.",
  "style": {
    "background": "#0b1416",
    "text": "#d7e3df",
    "accent": "#7f92cc",
    "hover": "#11201f",
    "active": "#16302b",
    "line": "#13312a",
    "glow": false
  },
  "buttons": ["add-download", "resume", "stop", "separator", "options"],
  "icons": {
    "add-download": "<svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"#56d364\" stroke-width=\"2\" stroke-linecap=\"round\"><path d=\"M12 5v14M5 12h14\"/></svg>"
  }
}
```

Only the icon for `add-download` is replaced above. The other buttons keep the
style's own icons.

### Fields

| Field | What it does | Rules |
| --- | --- | --- |
| `format` | File format version | Must be `1`; leaving it out means `1` |
| `id` | Stable name, used when replacing or removing | Lower-case letters, digits, `.`, `-`, `_`; up to 48 characters; `default`, `3d`, `classic`, `isometric` and `blueprint` are taken |
| `name` | The name in the menu and the style list | Up to 48 characters |
| `version` | Optional version shown in the toolbars window | Up to 24 characters |
| `author` | Optional author shown beside the version | Up to 64 characters |
| `description` | Optional sentence shown under the name | Up to 240 characters |
| `style` | Optional colours for the bar | See below |
| `buttons` | Optional button order | Up to 64 entries |
| `icons` | Optional SVG drawings, by button | Up to 32 icons |

Unknown fields are ignored.

### Buttons

Use these names in `buttons`:

`add-download`, `resume`, `stop`, `stop-all`, `delete`, `delete-completed`,
`options`, `scheduler`, `start-queue`, `stop-queue`, `tell-friend`.

Add `"separator"` between groups. A button may appear once; separators at the
ends or beside each other are removed. A name the app does not know is dropped,
so a pack made for a newer build still loads.

### Colours

`style` is optional. Every colour is a hex value: `#rgb`, `#rgba`, `#rrggbb` or
`#rrggbbaa`. Anything else is ignored.

| Key | What it paints |
| --- | --- |
| `background` | The bar's surface |
| `text` | Button names and icons |
| `accent` | The glow and, with `glow`, the label colour |
| `hover` | A button under the pointer |
| `active` | A button being pressed |
| `line` | The separator between groups |
| `glow` | `true` lights the icons and labels with `accent` |

Leave a colour out to use the app's theme instead. That also keeps the bar
readable in both the light and dark appearance.

### Icons

`icons` maps a button name to raw SVG markup:

```json
{
  "icons": {
    "stop": "<svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"#e05252\" stroke-width=\"2\"><circle cx=\"12\" cy=\"12\" r=\"9\"/><path d=\"M8 12h8\"/></svg>"
  }
}
```

- One icon may be up to 16 KB, and a pack may hold up to 32 icons.
- The root should have a `viewBox`. If it has only `width` and `height`, the
  app builds a view box from them.
- The drawing is scaled to the button, so `width` and `height` on the root are
  removed.
- Scripts, embedded images, `<foreignObject>`, `<style>`, event handlers,
  `javascript:` links and external `href`/`url()` references are stripped or
  refused.
- A pack icon is drawn as an image, never inserted as live markup.
- If an icon cannot be read, that button keeps the built-in drawing.

### Limits and checks

A pack is refused with a readable reason when:

- the file is larger than 512 KB or is not valid JSON;
- the id is missing, malformed or uses a built-in id;
- a button name is unknown or the colour is not a hex value;
- an icon is too large, is not SVG, or carries something the app will not draw.

A file already in the folder that cannot be used appears under **Files that
could not be used** in the toolbars window. The rest of the folder still loads.
Duplicate ids keep the first file in the folder and report the other one.

## Replace or remove a pack

Open **Look for new toolbars...** and use **Remove** beside an installed pack.
The pack in use cannot leave the bar empty — removing it switches back to
**Default**. The built-in styles cannot be removed.

To update a pack, install the new file with the same id and confirm **Replace**.
Your customized button order for that pack is kept.

Special toolbar packs are coming soon. Until then, the five built-in styles
and any pack you make work with the same Customize window.
