// Icon names: every icon the pages ask for is one lib/icons.js draws.

import assert from "node:assert/strict";
import vm from "node:vm";
import { read } from "./common.mjs";

const iconContext = vm.createContext({});
vm.runInContext(read("lib/icons.js"), iconContext, { filename: "icons.js" });
const iconNames = vm.runInContext("Object.keys(BDM_ICON_SHAPES)", iconContext);

for (const file of ["popup/popup.js", "options/options.js", "content/overlay.js"]) {
  for (const match of read(file).matchAll(/bdmIcon\("([a-z0-9-]+)"/g)) {
    assert.ok(
      iconNames.includes(match[1]),
      `${file} asks for the icon "${match[1]}", which lib/icons.js does not draw`,
    );
  }
}
assert.ok(
  read("popup/popup.html").includes("../lib/icons.js"),
  "the popup loads the icon helper it calls",
);