import { describe, expect, it } from "vitest";

import { cn, formatBytes, formatPercent } from "@/lib/utils";

describe("cn", () => {
  it("lets the later class win where two utilities collide", () => {
    expect(cn("p-2", "p-4")).toBe("p-4");
    expect(cn("text-xs", false && "hidden", "text-sm")).toBe("text-sm");
  });
});

describe("formatBytes", () => {
  it("keeps small sizes in bytes and steps up through the units", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(-4)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(1024 ** 3)).toBe("1.0 GB");
  });
});

describe("formatPercent", () => {
  it("never divides by zero", () => {
    expect(formatPercent(10, 0)).toBe(0);
  });

  it("rounds and caps at 100", () => {
    expect(formatPercent(50, 200)).toBe(25);
    expect(formatPercent(300, 200)).toBe(100);
  });
});
