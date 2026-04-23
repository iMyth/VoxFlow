/** Parse exported script text files and re-import them into a project. */

export interface ParsedLine {
  text: string;
  characterName: string | null;
  sectionName: string | null;
}

export interface ParseResult {
  lines: ParsedLine[];
  /** Unique character names found in the file, in order of first appearance */
  characterNames: string[];
  /** Unique section names found in the file, in order of first appearance */
  sectionNames: string[];
}

const SECTION_RE = /^===\s*(.+?)\s*===\s*$/;
const LINE_RE = /^\[(.+?)\]\s*(.*)$/;

export function parseScriptText(content: string): ParseResult {
  const lines: ParsedLine[] = [];
  const charSet = new Set<string>();
  const sectionSet = new Set<string>();
  let currentSection: string | null = null;

  for (const raw of content.split('\n')) {
    const line = raw.trim();
    if (!line) continue;

    // Skip the project name header line (first line followed by ===)
    const sectionMatch = line.match(SECTION_RE);
    if (sectionMatch) {
      const title = sectionMatch[1];
      currentSection = title;
      if (!sectionSet.has(title)) sectionSet.add(title);
      continue;
    }

    const lineMatch = line.match(LINE_RE);
    if (lineMatch) {
      const charName = lineMatch[1];
      const text = lineMatch[2];
      if (text.trim()) {
        lines.push({ text: text.trim(), characterName: charName, sectionName: currentSection });
        if (!charSet.has(charName)) charSet.add(charName);
      }
    } else if (line && !/^[=]+$/.test(line)) {
      // Plain text line without character marker (skip pure === lines)
      lines.push({ text: line, characterName: null, sectionName: currentSection });
    }
  }

  return {
    lines,
    characterNames: [...charSet],
    sectionNames: [...sectionSet],
  };
}
