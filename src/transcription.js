/**
 * Transcription Helper Utilities
 * Pure functions for extracting words, paragraph context, and applying corrections.
 */

/**
 * Punctuation characters to strip from edges of words.
 */
const PUNCT_REGEX_LEADING = /^[^\p{L}\p{N}]+/u;
const PUNCT_REGEX_TRAILING = /[^\p{L}\p{N}]+$/u;

/**
 * Extract word and its exact range [startIndex, endIndex] in text at given position.
 * Handles surrounding whitespace and punctuation cleanly.
 *
 * @param {string} text - Full text content
 * @param {number} position - Cursor / click position index
 * @returns {{ word: string, rawChunk: string, startIndex: number, endIndex: number } | null}
 */
export function findWordAtPosition(text, position) {
  if (!text || position < 0 || position > text.length) {
    return null;
  }

  // If position is at whitespace, check if adjacent to a word
  let pos = position;
  if (pos === text.length && pos > 0) {
    pos = pos - 1;
  }
  if (pos > 0 && /\s/.test(text[pos]) && !/\s/.test(text[pos - 1])) {
    pos = pos - 1;
  }
  if (pos < text.length - 1 && /\s/.test(text[pos]) && !/\s/.test(text[pos + 1])) {
    pos = pos + 1;
  }
  if (/\s/.test(text[pos])) {
    return null;
  }

  // Find boundaries of contiguous non-whitespace chunk
  let start = pos;
  while (start > 0 && !/\s/.test(text[start - 1])) {
    start--;
  }
  let end = pos;
  while (end < text.length && !/\s/.test(text[end])) {
    end++;
  }

  const rawChunk = text.slice(start, end);
  if (!rawChunk.trim()) {
    return null;
  }

  // Strip leading punctuation but adjust start index
  const leadMatch = rawChunk.match(PUNCT_REGEX_LEADING);
  const leadLen = leadMatch ? leadMatch[0].length : 0;

  // Strip trailing punctuation but adjust end index
  const trailMatch = rawChunk.match(PUNCT_REGEX_TRAILING);
  const trailLen = trailMatch ? trailMatch[0].length : 0;

  const wordStart = start + leadLen;
  const wordEnd = end - trailLen;

  let word = "";
  if (wordEnd > wordStart) {
    word = text.slice(wordStart, wordEnd);
  } else {
    // If chunk only contained symbols, use raw chunk
    word = rawChunk.trim();
  }

  return {
    word,
    rawChunk,
    startIndex: wordEnd > wordStart ? wordStart : start,
    endIndex: wordEnd > wordStart ? wordEnd : end,
  };
}

/**
 * Extract the full paragraph block containing the given position.
 * A paragraph is delimited by double newlines (\n\n) or start/end of text.
 *
 * @param {string} text - Full text content
 * @param {number} position - Position within the paragraph
 * @returns {string} - The trimmed full paragraph text
 */
export function findParagraphAtPosition(text, position) {
  if (!text) return "";
  const pos = Math.max(0, Math.min(position, text.length));

  // Find start of paragraph (after previous double newline)
  let start = text.lastIndexOf("\n\n", pos);
  if (start === -1) {
    start = 0;
  } else {
    start += 2; // Move past \n\n
  }

  // Find end of paragraph (before next double newline)
  let end = text.indexOf("\n\n", pos);
  if (end === -1) {
    end = text.length;
  }

  const paragraph = text.slice(start, end).trim();
  return paragraph;
}

/**
 * Apply word correction to text by replacing [startIndex, endIndex] with newWord.
 *
 * @param {string} text - Original full text
 * @param {number} startIndex - Starting character index of original word
 * @param {number} endIndex - Ending character index of original word
 * @param {string} newWord - Replacement word
 * @returns {string} - Updated full text
 */
export function applyCorrectionToText(text, startIndex, endIndex, newWord) {
  if (startIndex < 0 || endIndex < startIndex || startIndex > text.length) {
    return text;
  }
  return text.slice(0, startIndex) + newWord + text.slice(endIndex);
}

/**
 * Create a structured correction record.
 *
 * @param {{ original: string, corrected: string, context: string, file?: string, timestamp?: string }} params
 * @returns {{ id: number, original: string, corrected: string, context: string, file: string, timestamp: string }}
 */
export function createCorrectionRecord({ original, corrected, context, file = "Untitled", timestamp }) {
  return {
    id: Date.now() + Math.floor(Math.random() * 1000),
    original: (original || "").trim(),
    corrected: (corrected || "").trim(),
    context: (context || "").trim(),
    file: file || "Untitled",
    timestamp: timestamp || new Date().toISOString(),
  };
}

/**
 * Format corrections list as CSV string.
 *
 * @param {Array<{ timestamp: string, original: string, corrected: string, context: string, file: string }>} list
 * @returns {string} CSV formatted content
 */
export function correctionsToCsv(list) {
  const header = ["Timestamp", "Original", "Corrected", "Context", "File"];
  const rows = list.map((item) => {
    const escapeCsv = (val) => `"${String(val || "").replace(/"/g, '""')}"`;
    return [
      escapeCsv(item.timestamp),
      escapeCsv(item.original),
      escapeCsv(item.corrected),
      escapeCsv(item.context),
      escapeCsv(item.file),
    ].join(",");
  });
  return [header.join(","), ...rows].join("\n");
}
