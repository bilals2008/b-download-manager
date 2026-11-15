//! The page the phone shows.
//!
//! It is plain HTML with no framework and no bundler: the phone opens one
//! address, the page asks for the pairing code the first time and keeps the
//! token afterwards. It says in plain words that the connection is not
//! encrypted, because it is not.

/// The page itself.
pub const PAGE: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Downloads</title>
<style>
:root {
  color-scheme: light dark;
  --bg: #ffffff;
  --fg: #16181d;
  --muted: #5b6270;
  --line: #d9dde3;
  --accent: #1f6feb;
  --surface: #f4f6f8;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #121417;
    --fg: #e9ecf1;
    --muted: #9aa3b2;
    --line: #2a2f36;
    --accent: #5b9bff;
    --surface: #1a1d22;
  }
}
* { box-sizing: border-box; }
body {
  margin: 0;
  padding: 1rem 1rem 3rem;
  background: var(--bg);
  color: var(--fg);
  font: 16px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
}
main { max-width: 40rem; margin: 0 auto; }
h1 { font-size: 1.4rem; margin: 0 0 .25rem; }
.note { color: var(--muted); font-size: .9rem; margin: 0 0 1.25rem; }
h2 { font-size: 1rem; margin: 1.5rem 0 .5rem; }
label { display: block; font-size: .85rem; color: var(--muted); margin: .75rem 0 .25rem; }
input {
  width: 100%; padding: .7rem; font-size: 1rem; color: var(--fg);
  background: var(--surface); border: 1px solid var(--line); border-radius: 6px;
}
button {
  min-height: 44px; padding: .6rem 1rem; font-size: 1rem; cursor: pointer;
  color: var(--fg); background: var(--surface); border: 1px solid var(--line); border-radius: 6px;
}
button.primary { color: #ffffff; background: var(--accent); border-color: var(--accent); }
button:active { opacity: .8; }
.row { display: flex; gap: .5rem; }
.row > * { flex: 1; }
.item { padding: .9rem 0; border-top: 1px solid var(--line); }
.item:first-child { border-top: none; }
.name { font-weight: 600; word-break: break-word; }
.meta { color: var(--muted); font-size: .85rem; margin: .2rem 0 .5rem; }
.bar { height: 4px; background: var(--line); border-radius: 2px; overflow: hidden; }
.bar > span { display: block; height: 100%; background: var(--accent); }
.actions { display: flex; gap: .5rem; margin-top: .6rem; }
.actions button { flex: 1; font-size: .9rem; }
.status { min-height: 1.4rem; color: var(--muted); font-size: .9rem; margin: .75rem 0 0; }
.error { color: #c0392b; }
@media (prefers-color-scheme: dark) { .error { color: #ff8a80; } }
</style>
</head>
<body>
<main>
<h1>Downloads</h1>
<p class="note">This page uses plain HTTP on your local network. The connection is not encrypted, so anyone on the same Wi-Fi can read it.</p>

<section id="pair">
<h2>Pair this phone</h2>
<form id="pair-form">
<label for="code">Pairing code from the app</label>
<input id="code" name="code" inputmode="numeric" autocomplete="off" required>
<label for="name">Name this device</label>
<input id="name" name="name" autocomplete="off" value="Phone">
<button class="primary" type="submit">Pair</button>
</form>
<p class="status" id="pair-status"></p>
</section>

<section id="paired" hidden>
<form id="add-form">
<label for="url">Link to download</label>
<input id="url" name="url" type="url" inputmode="url" autocomplete="off" placeholder="https://example.org/file.zip" required>
<label for="folder">Folder (optional)</label>
<input id="folder" name="folder" autocomplete="off">
<button class="primary" type="submit">Add</button>
</form>
<p class="status" id="add-status"></p>

<h2>List</h2>
<div id="list"></div>
<p class="status" id="list-status"></p>
</section>
</main>
<script>
(function () {
  var token = window.localStorage.getItem('bdm-token') || '';
  var pairForm = document.getElementById('pair-form');
  var pairStatus = document.getElementById('pair-status');
  var addForm = document.getElementById('add-form');
  var addStatus = document.getElementById('add-status');
  var list = document.getElementById('list');
  var listStatus = document.getElementById('list-status');
  var timer = null;

  function show(section, other) {
    section.hidden = false;
    other.hidden = true;
  }

  function say(element, text, isError) {
    element.textContent = text;
    element.className = isError ? 'status error' : 'status';
  }

  function bytes(value) {
    if (!value) { return '0 B'; }
    var units = ['B', 'KB', 'MB', 'GB', 'TB'];
    var size = value;
    var unit = 0;
    while (size >= 1024 && unit < units.length - 1) { size = size / 1024; unit = unit + 1; }
    return (unit === 0 ? size : size.toFixed(1)) + ' ' + units[unit];
  }

  function request(method, path, body) {
    var options = { method: method, headers: {} };
    if (token) { options.headers['Authorization'] = 'Bearer ' + token; }
    if (body) {
      options.headers['Content-Type'] = 'application/json';
      options.body = JSON.stringify(body);
    }
    return fetch(path, options).then(function (answer) {
      return answer.json().catch(function () { return {}; }).then(function (data) {
        if (!answer.ok) { throw new Error(data.error || 'the request failed'); }
        return data;
      });
    });
  }

  function render(downloads) {
    list.textContent = '';
    if (!downloads.length) {
      var empty = document.createElement('p');
      empty.className = 'meta';
      empty.textContent = 'Nothing is downloading.';
      list.appendChild(empty);
      return;
    }
    downloads.forEach(function (download) {
      var item = document.createElement('div');
      item.className = 'item';

      var name = document.createElement('div');
      name.className = 'name';
      name.textContent = download.name;
      item.appendChild(name);

      var meta = document.createElement('div');
      meta.className = 'meta';
      var percent = download.size ? Math.floor(download.downloaded * 100 / download.size) : 0;
      meta.textContent = download.state + ' · ' + bytes(download.downloaded) + ' of ' +
        bytes(download.size) + ' · ' + bytes(download.speed) + '/s' +
        (download.size ? ' · ' + percent + '%' : '');
      item.appendChild(meta);

      var bar = document.createElement('div');
      bar.className = 'bar';
      var fill = document.createElement('span');
      fill.style.width = (download.size ? Math.min(100, percent) : 0) + '%';
      bar.appendChild(fill);
      item.appendChild(bar);

      var actions = document.createElement('div');
      actions.className = 'actions';
      actions.appendChild(action(download, 'pause', 'Pause'));
      actions.appendChild(action(download, 'resume', 'Resume'));
      actions.appendChild(action(download, 'remove', 'Remove'));
      item.appendChild(actions);

      list.appendChild(item);
    });
  }

  function action(download, name, label) {
    var button = document.createElement('button');
    button.type = 'button';
    button.textContent = label;
    button.addEventListener('click', function () {
      request('POST', '/api/downloads/' + download.id + '/' + name)
        .then(function () { load(); })
        .catch(function (error) { say(listStatus, error.message, true); });
    });
    return button;
  }

  function load() {
    if (!token) { return; }
    request('GET', '/api/downloads')
      .then(function (data) {
        say(listStatus, '');
        render(data.downloads || []);
      })
      .catch(function (error) {
        say(listStatus, error.message, true);
        if (error.message.indexOf('token') >= 0) { forget(); }
      });
  }

  function forget() {
    token = '';
    window.localStorage.removeItem('bdm-token');
    if (timer) { window.clearInterval(timer); timer = null; }
    show(document.getElementById('pair'), document.getElementById('paired'));
  }

  function paired() {
    show(document.getElementById('paired'), document.getElementById('pair'));
    load();
    if (!timer) { timer = window.setInterval(load, 2000); }
  }

  pairForm.addEventListener('submit', function (event) {
    event.preventDefault();
    var code = document.getElementById('code').value.trim();
    var name = document.getElementById('name').value.trim() || 'Phone';
    request('POST', '/api/pair', { code: code, name: name })
      .then(function (data) {
        token = data.token;
        window.localStorage.setItem('bdm-token', token);
        say(pairStatus, 'This device is paired.', false);
        paired();
      })
      .catch(function (error) { say(pairStatus, error.message, true); });
  });

  addForm.addEventListener('submit', function (event) {
    event.preventDefault();
    var url = document.getElementById('url').value.trim();
    var folder = document.getElementById('folder').value.trim();
    request('POST', '/api/downloads', { url: url, folder: folder })
      .then(function () {
        document.getElementById('url').value = '';
        say(addStatus, 'Added.', false);
        load();
      })
      .catch(function (error) { say(addStatus, error.message, true); });
  });

  // A code in the address is read from the QR code and used at once.
  var hash = window.location.hash.replace(/^#/, '');
  if (hash.indexOf('code=') === 0) {
    document.getElementById('code').value = decodeURIComponent(hash.slice(5));
  }

  if (token) { paired(); } else { show(document.getElementById('pair'), document.getElementById('paired')); }
})();
</script>
</body>
</html>
"##;

#[cfg(test)]
mod tests {
    use super::PAGE;

    #[test]
    fn the_page_says_the_connection_is_not_encrypted() {
        assert!(PAGE.contains("HTTP"));
        assert!(PAGE.contains("not encrypted"));
    }

    #[test]
    fn the_page_has_what_the_phone_needs() {
        assert!(PAGE.contains("/api/downloads"));
        assert!(PAGE.contains("/api/pair"));
        assert!(PAGE.contains("Pause"));
        assert!(PAGE.contains("Resume"));
        assert!(PAGE.contains("Remove"));
    }
}
