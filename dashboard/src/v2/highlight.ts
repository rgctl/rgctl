/**
 * Minimal syntax highlighting for the Spine's source pane.
 *
 * Deliberately a tokenizer rather than a grammar. A real highlighter (Prism, highlight.js, Shiki) would
 * add tens of kilobytes gzipped to a bundle that has a recorded size budget, to decorate text a reader is
 * skimming — and this app is a reader, not an editor.
 *
 * **Known limits, stated because they are visible:** block comments and multi-line strings are matched
 * within a line only, so a `/* … *​/` spanning lines highlights each line's fragment rather than the whole
 * comment, and a template literal spanning lines greys each line separately. A `//` inside a string is
 * handled correctly, because strings are matched before comments. This is a reading aid; it is not a
 * parser and must not be used to decide anything.
 */

export type TokenKind = "plain" | "comment" | "string" | "number" | "keyword";

export interface Token {
  text: string;
  kind: TokenKind;
}

/** Keyword sets per language id, kept small on purpose — the words that carry structure. */
const KEYWORDS: Record<string, string[]> = {
  typescript: "const let var function return if else for while await async export import from new class extends implements interface type enum throw try catch finally switch case default break continue of in typeof instanceof as satisfies declare readonly public private protected static void null undefined true false this super yield delete".split(" "),
  javascript: "const let var function return if else for while await async export import from new class extends throw try catch finally switch case default break continue of in typeof instanceof void null undefined true false this super yield delete".split(" "),
  rust: "fn let mut const struct enum impl trait for while loop if else match return pub use mod crate self super where async await move ref dyn box unsafe static type as in break continue".split(" "),
  python: "def class return if elif else for while import from as with try except finally raise lambda yield async await global nonlocal pass break continue in is not and or None True False self".split(" "),
  go: "func var const type struct interface map chan package import return if else for range go defer select switch case default break continue nil true false".split(" "),
  java: "public private protected class interface extends implements static final void new return if else for while try catch finally throw throws import package this super null true false int long double float boolean char byte short enum switch case default break continue".split(" "),
  csharp: "public private protected internal class interface struct record static readonly void new return if else for foreach while try catch finally throw using namespace var this base null true false async await".split(" "),
  cpp: "int long double float char bool void auto const constexpr static struct class public private protected namespace using return if else for while switch case default break continue new delete nullptr true false template typename".split(" "),
  c: "int long double float char void const static struct union enum typedef return if else for while switch case default break continue sizeof NULL".split(" "),
  php: "function class interface extends implements public private protected static return if else elseif for foreach while try catch finally throw new echo namespace use null true false array".split(" "),
  ruby: "def class module end if elsif else unless while until for do return yield begin rescue ensure raise require attr_accessor nil true false self new".split(" "),
};

function keywordsFor(language: string): Set<string> {
  return new Set(KEYWORDS[language] ?? KEYWORDS.typescript!);
}

const IDENT = /[A-Za-z_$][A-Za-z0-9_$]*/;

/** Tokenize one line. State does not carry across lines — see the limits above. */
export function highlightLine(line: string, language: string): Token[] {
  const keywords = keywordsFor(language);
  const out: Token[] = [];
  let i = 0;
  let plainStart = 0;

  const flushPlain = (end: number) => {
    if (end > plainStart) out.push({ text: line.slice(plainStart, end), kind: "plain" });
  };

  while (i < line.length) {
    const c = line[i]!;

    // Strings before comments, so `"// not a comment"` survives.
    if (c === '"' || c === "'" || c === "`") {
      flushPlain(i);
      let j = i + 1;
      while (j < line.length && line[j] !== c) {
        if (line[j] === "\\") j++; // skip the escaped character
        j++;
      }
      out.push({ text: line.slice(i, Math.min(j + 1, line.length)), kind: "string" });
      i = j + 1;
      plainStart = i;
      continue;
    }

    if (c === "/" && line[i + 1] === "/") {
      flushPlain(i);
      out.push({ text: line.slice(i), kind: "comment" });
      return out;
    }

    if (c === "/" && line[i + 1] === "*") {
      flushPlain(i);
      const close = line.indexOf("*/", i + 2);
      const end = close === -1 ? line.length : close + 2;
      out.push({ text: line.slice(i, end), kind: "comment" });
      i = end;
      plainStart = i;
      continue;
    }

    if (c >= "0" && c <= "9") {
      flushPlain(i);
      let j = i;
      while (j < line.length && /[0-9a-fA-FxX._]/.test(line[j]!)) j++;
      out.push({ text: line.slice(i, j), kind: "number" });
      i = j;
      plainStart = i;
      continue;
    }

    const m = IDENT.exec(line.slice(i));
    if (m && m.index === 0) {
      const word = m[0];
      if (keywords.has(word)) {
        flushPlain(i);
        out.push({ text: word, kind: "keyword" });
        i += word.length;
        plainStart = i;
        continue;
      }
      i += word.length;
      continue;
    }

    i++;
  }

  flushPlain(line.length);
  return out;
}

/** Tailwind classes per token kind. Kept here so the two panes cannot drift apart. */
export const TOKEN_CLASS: Record<TokenKind, string> = {
  plain: "text-zinc-300",
  comment: "text-zinc-500 italic",
  string: "text-emerald-300/90",
  number: "text-amber-200/90",
  keyword: "text-sky-300",
};
