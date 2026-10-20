#!/usr/bin/env node
// Fail unless the release signing key matches the public key Reverb ships.
//
// Installed copies of Reverb only accept updates signed by the private key
// whose public half is in src-tauri/tauri.conf.json. The Tauri CLI signs with
// whatever TAURI_SIGNING_PRIVATE_KEY holds and only *warns* when that doesn't
// match, so a release signed with the wrong key would build fine and then be
// refused by every install. The release workflow signs a probe file with the
// secret and runs this on it before building anything.
//
//     npx tauri signer sign probe.txt          # writes probe.txt.sig
//     node scripts/check-updater-key.mjs probe.txt

import { createHash, createPublicKey, verify } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

/**
 * Tauri keys and signatures are minisign files, base64-encoded once more as a
 * whole. The second line of each is the base64 payload: a 2-byte algorithm,
 * an 8-byte key id, then the key or signature.
 */
function minisignPayload(encoded) {
  const text = Buffer.from(encoded.trim(), "base64").toString("utf8");
  const line = text.split("\n")[1];
  return line ? Buffer.from(line.trim(), "base64") : Buffer.alloc(0);
}

/**
 * Check `signature` (a `.sig` file's contents) over `data` against `pubkey`
 * (tauri.conf.json's `plugins.updater.pubkey`).
 */
export function verifyUpdaterSignature(pubkey, signature, data) {
  const key = minisignPayload(pubkey);
  if (key.length !== 42 || key.subarray(0, 2).toString() !== "Ed") {
    return {
      ok: false,
      reason:
        "plugins.updater.pubkey in tauri.conf.json is not a minisign public key. " +
        "Paste the public key generated with the signing key (see RELEASING.md).",
    };
  }
  const sig = minisignPayload(signature);
  const algorithm = sig.subarray(0, 2).toString();
  if (sig.length !== 74 || (algorithm !== "ED" && algorithm !== "Ed")) {
    return { ok: false, reason: "the .sig file is not a minisign signature" };
  }
  if (!sig.subarray(2, 10).equals(key.subarray(2, 10))) {
    return {
      ok: false,
      reason:
        `signed with a different key (id ${sig.subarray(2, 10).toString("hex")}) than the ` +
        `public key in tauri.conf.json (id ${key.subarray(2, 10).toString("hex")})`,
    };
  }
  const publicKey = createPublicKey({
    key: { kty: "OKP", crv: "Ed25519", x: key.subarray(10).toString("base64url") },
    format: "jwk",
  });
  // "ED" signs the BLAKE2b-512 hash of the file rather than the file itself.
  const message = algorithm === "ED" ? createHash("blake2b512").update(data).digest() : data;
  if (!verify(null, message, publicKey, sig.subarray(10))) {
    return { ok: false, reason: "the signature does not verify against the public key" };
  }
  return { ok: true };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const file = process.argv[2];
  if (!file) {
    console.error("usage: node scripts/check-updater-key.mjs <signed file>");
    process.exit(2);
  }
  const root = join(dirname(fileURLToPath(import.meta.url)), "..");
  const config = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"));
  const result = verifyUpdaterSignature(
    config.plugins?.updater?.pubkey ?? "",
    readFileSync(`${file}.sig`, "utf8"),
    readFileSync(file),
  );
  if (!result.ok) {
    console.error(`Updater signing key check failed: ${result.reason}`);
    process.exit(1);
  }
  console.log("The signing key matches the public key in tauri.conf.json.");
}
