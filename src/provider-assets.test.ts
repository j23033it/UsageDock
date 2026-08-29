import { describe, expect, it } from "vitest";
import openCodeGoIcon from "./assets/providers/opencode-go.svg?raw";

describe("プロバイダーアイコン", () => {
  it("OpenCodeロゴの色を画像内で明示する", () => {
    expect(openCodeGoIcon).toContain('fill="#111827"');
    expect(openCodeGoIcon).not.toContain("currentColor");
  });
});
