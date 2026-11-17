import { test } from "node:test";
import assert from "node:assert/strict";
import { transcript, withTranscript } from "../src/core/chat-import.ts";
test("transcripts accept only nonempty bounded UTF-8 text or Markdown", () => {
  assert.deepEqual(transcript("Conversa.MD", "\uFEFF  Usuário: oi\nIA: olá "), { name: "Conversa.MD", text: "Usuário: oi\nIA: olá" });
  assert.throws(() => transcript("export.zip", "abc"));
  assert.throws(() => transcript("vazio.txt", "  "));
  assert.throws(() => transcript("grande.txt", "á".repeat(50_001)));
});
test("the next request carries the transcript as reference, leaving plain queries alone", () => {
  assert.equal(withTranscript("olá", null), "olá");
  const result = withTranscript("continue", transcript("chat.txt", "Conversa anterior"));
  assert.match(result, /<conversa_importada>\nConversa anterior/);
  assert.ok(result.endsWith("Pedido atual do usuário: continue"));
});
