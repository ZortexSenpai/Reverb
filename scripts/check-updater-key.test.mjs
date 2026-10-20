import { describe, expect, it } from "vitest";
import { verifyUpdaterSignature } from "./check-updater-key.mjs";

// Two throwaway key pairs made with `tauri signer generate` for this test (the
// private halves were discarded), and key A's signature over "probe\n".
const PUBKEY_A =
  "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDUzN0Q3NDEyMUY3MzlGNDgKUldSSW4zTWZFblI5VXpDRXhOK1lZbTdRUzNCRzdiN216N0lTaTdId1BkNTN0cmhVWUM1aTdXQUwK";
const PUBKEY_B =
  "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEYzRUUzNjJGMUYwN0M2QzIKUldUQ3hnY2ZMemJ1OC9XQXZOME1ZT2JIcHA5N2ZVYXRDc0FBbzduMGMxZVB2d2ZqbTZFWDNZNlMK";
const SIGNATURE_A =
  "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVSSW4zTWZFblI5VTBmdFpuSG9xVjVDeXhxcTkvQkxaY25IWVpnL3JTdDN0bHQ3ajFaN1dJYXczZ0N0Yk9sRGdJcG5ybDdaaFdIcGdLOGlQVitSZThWWG8vN2o3UmE5V2dJPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkyNTM2NzM1CWZpbGU6cHJvYmUudHh0CmtydDNTQVZCS1NVM0QrTUJ1Wmt2eGZORHFCODRlU0t1aUNkMGUrYTFpbzUrNmNaZUtnSDZobHUrZ0pZalJjb2dFTlpTYktVKzNxTC9zRHB5WmdlbkNRPT0K";
const PROBE = Buffer.from("probe\n");

describe("verifyUpdaterSignature", () => {
  it("accepts a signature from the private key matching the configured public key", () => {
    expect(verifyUpdaterSignature(PUBKEY_A, SIGNATURE_A, PROBE)).toEqual({ ok: true });
  });

  it("rejects a signature from a different key pair", () => {
    const result = verifyUpdaterSignature(PUBKEY_B, SIGNATURE_A, PROBE);

    expect(result.ok).toBe(false);
    expect(result.reason).toMatch(/different key/);
  });

  it("rejects a public key that is still a placeholder", () => {
    const result = verifyUpdaterSignature("REPLACE-WITH-THE-PUBLIC-KEY", SIGNATURE_A, PROBE);

    expect(result.ok).toBe(false);
    expect(result.reason).toMatch(/not a minisign public key/);
  });

  it("rejects a signature that does not cover these bytes", () => {
    const result = verifyUpdaterSignature(PUBKEY_A, SIGNATURE_A, Buffer.from("tampered\n"));

    expect(result.ok).toBe(false);
    expect(result.reason).toMatch(/does not verify/);
  });
});
