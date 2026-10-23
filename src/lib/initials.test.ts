import { describe, expect, it } from "vitest";
import { initials } from "./initials";

describe("initials", () => {
  it("takes the first and last word of a name", () => {
    expect(initials("Florence + the Machine")).toBe("FM");
    expect(initials("the xx")).toBe("TX");
  });

  it("keeps a one-word name to one letter", () => {
    expect(initials("Radiohead")).toBe("R");
    expect(initials("AC/DC")).toBe("A");
  });

  it("skips words without letters or digits", () => {
    expect(initials("Simon & Garfunkel")).toBe("SG");
    expect(initials("Artist - ")).toBe("A");
  });

  it("reads past punctuation and handles any script", () => {
    expect(initials("(hed) p.e.")).toBe("HP");
    expect(initials("50 Cent")).toBe("5C");
    expect(initials("Ólafur Arnalds")).toBe("ÓA");
    expect(initials("夜に駆ける")).toBe("夜");
  });

  it("falls back to a placeholder for a name with nothing to show", () => {
    expect(initials("")).toBe("?");
    expect(initials(" … ")).toBe("?");
  });
});
