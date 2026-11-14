import test from "node:test";
import assert from "node:assert/strict";
import {
  findWordAtPosition,
  findParagraphAtPosition,
  applyCorrectionToText,
  createCorrectionRecord,
  correctionsToCsv,
} from "../src/transcription.js";

const sampleText = `-improve abstract
-quitar source..
-originality small improve

From the control in Python and ROS, a final graph for Visual Servoing combining the Camera and robotic arm was generated as an RQT graph (see ).`;

test("findWordAtPosition extracts exact word without punctuation", () => {
  // Test "quitar" inside "-quitar source.."
  const posQuitar = sampleText.indexOf("quitar") + 2;
  const match = findWordAtPosition(sampleText, posQuitar);
  assert.ok(match);
  assert.equal(match.word, "quitar");
  assert.equal(sampleText.slice(match.startIndex, match.endIndex), "quitar");
});

test("findWordAtPosition handles words with surrounding punctuation like (see )", () => {
  const posSee = sampleText.indexOf("see");
  const match = findWordAtPosition(sampleText, posSee);
  assert.ok(match);
  assert.equal(match.word, "see");
  assert.equal(sampleText.slice(match.startIndex, match.endIndex), "see");
});

test("findWordAtPosition extracts words at boundary without trailing spaces", () => {
  const text = "word1    word2,  word3. ";
  const m1 = findWordAtPosition(text, 2);
  assert.equal(m1.word, "word1");
  assert.equal(text.slice(m1.startIndex, m1.endIndex), "word1");

  const m2 = findWordAtPosition(text, 11);
  assert.equal(m2.word, "word2");
  assert.equal(text.slice(m2.startIndex, m2.endIndex), "word2");
});

test("findParagraphAtPosition extracts the complete paragraph block", () => {
  const posQuitar = sampleText.indexOf("quitar");
  const p1 = findParagraphAtPosition(sampleText, posQuitar);
  assert.equal(
    p1,
    `-improve abstract\n-quitar source..\n-originality small improve`
  );

  const posServoing = sampleText.indexOf("Servoing");
  const p2 = findParagraphAtPosition(sampleText, posServoing);
  assert.equal(
    p2,
    `From the control in Python and ROS, a final graph for Visual Servoing combining the Camera and robotic arm was generated as an RQT graph (see ).`
  );
});

test("findParagraphAtPosition extracts paragraphs correctly with Windows CRLF newlines", () => {
  const crlfText = "Paragraph 1 line A\r\nParagraph 1 line B\r\n\r\nParagraph 2 with target word\r\n\r\nParagraph 3";
  const posTarget = crlfText.indexOf("target");
  const p = findParagraphAtPosition(crlfText, posTarget);
  assert.equal(p, "Paragraph 2 with target word");
});

test("applyCorrectionToText replaces target word cleanly preserving surrounding punctuation", () => {
  const posQuitar = sampleText.indexOf("quitar");
  const match = findWordAtPosition(sampleText, posQuitar);
  const updated = applyCorrectionToText(sampleText, match.startIndex, match.endIndex, "remover");

  assert.ok(updated.includes("-remover source.."));
  assert.ok(!updated.includes("-quitar source.."));
});

test("createCorrectionRecord creates structured object", () => {
  const rec = createCorrectionRecord({
    original: "quitar",
    corrected: "remover",
    context: "Full paragraph context here",
    file: "transcription.txt",
  });
  assert.equal(rec.original, "quitar");
  assert.equal(rec.corrected, "remover");
  assert.equal(rec.context, "Full paragraph context here");
  assert.equal(rec.file, "transcription.txt");
  assert.ok(rec.timestamp);
});

test("correctionsToCsv converts array of records to valid CSV", () => {
  const list = [
    {
      timestamp: "2026-09-14T12:00:00Z",
      original: "quitar",
      corrected: "remover",
      context: 'paragraph with "quotes" and commas, etc.',
      file: "doc.txt",
    },
  ];
  const csv = correctionsToCsv(list);
  assert.ok(csv.includes("Timestamp,Original,Corrected,Context,File"));
  assert.ok(csv.includes('"quitar","remover"'));
  assert.ok(csv.includes('""quotes""'));
});
