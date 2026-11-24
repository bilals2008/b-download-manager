---
title: Using the app
description: Adding downloads, pausing them, finding them again, and changing how the app behaves.
---

## Add a download

Press <kbd>Ctrl</kbd>+<kbd>N</kbd> from anywhere in the app, or click the **+** button at the top. Paste the link and press <kbd>Enter</kbd>.

The app asks the website what the file really is before it starts, which is why the name and size can change after you paste: the file decides, not the address. If the link has expired, needs a sign-in or points at a page instead of a file, the small window says so and keeps your link there so you can correct it.

- <kbd>Enter</kbd> starts the download.
- <kbd>Esc</kbd> closes the window without starting anything.
- Pressing Enter twice does nothing extra, and a link that is already in the list is pointed out rather than downloaded twice.

## From your browser

Install the [browser extension](/installation/#browser-extension) — Firefox from Mozilla Add-ons, Chrome, Edge, Brave and Safari from this repository — keep the app running, and right-click any link, picture, video or sound file to choose **Download with B Download Manager**. On supported videos a small button shows the qualities the page offers, and the toolbar popup shows the connection and the downloads already in the app.

There is no code to copy and nothing to pair: the extension finds the app on your own computer.

## Pause, resume, stop

Select a download, then use the buttons at the top or right-click it:

| Button | What it does |
| --- | --- |
| Pause | Stops for now. What has already arrived is kept. |
| Resume | Carries on from where it stopped. |
| Stop | Ends the download. The entry stays in the list. |
| Redownload | Downloads the file again from the beginning. |
| Remove | Takes the entry off the list. Files already saved are not deleted. |

Removing one download happens straight away. Removing several, or choosing **Delete All Completed**, asks first, and the safe answer is already selected.

## The file is already there

If a file with the same name is already in the folder, the app asks before it overwrites anything:

- **Rename** saves the new one as `name (2).ext`.
- **Replace** overwrites the old file. It is only offered when the old file is not being downloaded by anything else.
- **Skip** leaves the old file alone and drops the new download.
- **Cancel** closes the window without changing anything.

## Where your files go

Files land in the save folder you choose in **Settings → Save locations**, sorted into a folder for their kind — Videos, Music, Documents, Programs, Compressed and so on. General and Other go in the save folder itself. You can give any category its own folder on that page, and pick a different folder for a single download when you add it.

## Slow it down

If the app is taking too much of your connection:

- **Everything at once:** open **Settings → Connections**, turn on **Limit the speed of all downloads** and type a number.
- **One file:** open the download, go to **Speed limiter**, and set a limit for that file alone.

**Settings → Downloads** sets how many pieces of a file are fetched at once, under **Default number of connections** (32 by default). If downloads keep failing, try fewer. **Settings → Connections** caps the total the whole app may open.

## Schedule the queue

**Settings → Downloads** can run the queue only on chosen days and hours. It starts and stops by itself, and can turn the computer off once the queue is empty — never in the middle of a download.

## Find a download

Press <kbd>Ctrl</kbd>+<kbd>F</kbd> and type part of a name or a link. Press <kbd>F3</kbd> to jump to the next match.

## The folders on the left

**All downloads**, **Unfinished** and **Finished** show everything by state. Below them the app sorts downloads by kind: General, Video, Music, Documents, Programs, Games, Compressed and Other. Click one to see only those, and read the number beside it to know how many it holds. **View → Hide categories** hides the whole column.

Click a column heading to sort by it, and click again to reverse the order. Drag the line between two headings to widen a column, or double-click that line to reset it.

## Closing the window

Closing the window stops nothing. The app carries on and its icon stays next to the clock. Click that icon to bring the window back, to start or stop the queue, or to quit.

To make closing the window quit the app instead, open **Options → General** and turn off **Keep the application in the tray when the window is closed**.

The icon itself is yours to choose: **View → B Download Manager tray icon** takes it out of the notification area altogether, or lets it follow the app's light and dark appearance. **Customize URL List...** on the same menu says which kinds of address a copied link has to be before the app offers it.

## Making it yours

**Settings → Appearance** holds the theme — **Auto** follows the system — with ten looks to choose from, and the font with its weight. The language switch is under **View → Language** (English, Français, Deutsch, العربية; Arabic mirrors the whole window). Changes appear immediately in every open window.

## The toolbar

The bar under the menu can switch between five styles and your own button order. **View → Toolbar** changes the style, size and buttons; **Settings → Toolbar** also switches text labels and opens the same Customize and Manage windows.

[Toolbar packs](/toolbars/) explains how to install a custom toolbar from a JSON file, or make one yourself.

Settings is one dialog with pages for General, Appearance, Toolbar, Shortcuts, Tips, Downloads, Save locations, Browser, Video download button, Debrid, Torrent, Connections, Proxy, Advanced and About. The Remote page is still on the way, and batch adding, export and import remain disabled in the Tasks menu.

## Keyboard

| Keys | What they do |
| --- | --- |
| <kbd>Ctrl</kbd>+<kbd>N</kbd> | Add a download |
| <kbd>Enter</kbd> | Start the download in the add window |
| <kbd>Esc</kbd> | Close the window you are in |
| <kbd>Delete</kbd> | Remove the selected download |
| <kbd>Shift</kbd>+click | Select a row and everything up to it |
| <kbd>Ctrl</kbd>+click | Add a row to the selection |
| Arrow keys, <kbd>Home</kbd>, <kbd>End</kbd> | Move through the list |
| <kbd>Ctrl</kbd>+<kbd>F</kbd> | Find a download |
| <kbd>F3</kbd> | Find the next one |
