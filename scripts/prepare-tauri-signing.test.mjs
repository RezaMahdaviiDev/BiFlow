import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  normalizeUpdaterPassword,
  normalizeUpdaterPrivateKey,
  prepareUpdaterSigning,
  verifyUpdaterSigning,
} from "./prepare-tauri-signing.mjs";

// Passwordless sample key from tauri-cli updater_signature tests.
const SAMPLE_KEY =
  "dW50cnVzdGVkIGNvbW1lbnQ6IHJzaWduIGVuY3J5cHRlZCBzZWNyZXQga2V5ClJXUlRZMEl5dkpDN09RZm5GeVAzc2RuYlNzWVVJelJRQnNIV2JUcGVXZUplWXZXYXpqUUFBQkFBQUFBQUFBQUFBQUlBQUFBQTZrN2RnWGh5dURxSzZiL1ZQSDdNcktiaHRxczQwMXdQelRHbjRNcGVlY1BLMTBxR2dpa3I3dDE1UTVDRDE4MXR4WlQwa1BQaXdxKy9UU2J2QmVSNXhOQWFDeG1GSVllbUNpTGJQRkhhTnROR3I5RmdUZi90OGtvaGhJS1ZTcjdZU0NyYzhQWlQ5cGM9Cg==";

const SAMPLE_DECODED = Buffer.from(SAMPLE_KEY, "base64").toString("utf8");

describe("prepare-tauri-signing", () => {
  it("treats blank passwords as unset", () => {
    assert.equal(normalizeUpdaterPassword(undefined), undefined);
    assert.equal(normalizeUpdaterPassword(""), undefined);
    assert.equal(normalizeUpdaterPassword("  \n"), undefined);
    assert.equal(normalizeUpdaterPassword("secret"), "secret");
  });

  it("accepts the Tauri Base64 private key and raw minisign text", () => {
    assert.equal(normalizeUpdaterPrivateKey(SAMPLE_KEY), SAMPLE_KEY);
    assert.equal(
      normalizeUpdaterPrivateKey(` \n${SAMPLE_KEY}\r\n `),
      SAMPLE_KEY,
    );
    const restored = normalizeUpdaterPrivateKey(SAMPLE_DECODED);
    assert.equal(
      Buffer.from(restored, "base64").toString("utf8").trim(),
      SAMPLE_DECODED.trim(),
    );
    const collapsed = SAMPLE_DECODED.replaceAll("\n", "").trim();
    const fromCollapsed = normalizeUpdaterPrivateKey(collapsed);
    assert.equal(
      Buffer.from(fromCollapsed, "base64").toString("utf8").trim(),
      SAMPLE_DECODED.trim(),
    );
  });

  it("rejects empty keys, public keys, and random text", () => {
    assert.throws(() => normalizeUpdaterPrivateKey(""), /empty/);
    assert.throws(
      () => normalizeUpdaterPrivateKey("not-a-key"),
      /missing untrusted comment/,
    );
    const pubkey =
      "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDRENDJBQzBEOUMyMTM0NUMKUldSY05DR2NEYXhDVFZUMVd6L0JZc1NHb0srR3JrKzd2S2RRdTBhMDkwenQrV05ITW9tejdkQkIK";
    assert.throws(
      () => normalizeUpdaterPrivateKey(pubkey),
      /missing untrusted comment|public key/,
    );
  });

  it("requires a key only when asked", () => {
    const skipped = prepareUpdaterSigning({ requireKey: false });
    assert.equal(skipped.unsigned, true);
    assert.throws(() => prepareUpdaterSigning({ requireKey: true }), /not set/);
    const prepared = prepareUpdaterSigning({
      rawKey: SAMPLE_KEY,
      rawPassword: "",
      requireKey: true,
    });
    assert.equal(prepared.unsigned, false);
    assert.equal(prepared.password, undefined);
    assert.equal(prepared.key, SAMPLE_KEY);
  });

  it("does not print the private key when a probe sign fails", () => {
    assert.throws(
      () =>
        verifyUpdaterSigning(SAMPLE_KEY, undefined, () => ({
          status: 1,
          error: { message: "spawn pnpm ENOENT" },
          stdout: "",
          stderr: `failed to decode secret key: incorrect updater private key password: Missing comment in secret key\n${SAMPLE_KEY}`,
        })),
      (error) => {
        assert.match(String(error), /repository secret/);
        assert.match(String(error), /spawn pnpm ENOENT/);
        assert.doesNotMatch(String(error), /dW50cnVzdGVk/);
        return true;
      },
    );
  });
});
