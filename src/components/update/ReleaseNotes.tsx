// A release's CHANGELOG.md section, rendered as Markdown. The notes come from
// the release manifest on GitHub, so raw HTML is never rendered, only shown
// as text, and links leave the app for the browser.

import Markdown from "markdown-to-jsx/react";
import type { ReactNode } from "react";
import { useT } from "../../lib/i18n";

/**
 * markdown-to-jsx drops unsafe URLs (`javascript:`, `data:`) and passes no
 * href at all. `target="_blank"` hands the rest to the opener plugin, which
 * opens them in the browser.
 */
function NotesLink({ href, children }: { href?: string; children?: ReactNode }) {
  if (!href) return <>{children}</>;
  return (
    <a href={href} target="_blank" rel="noreferrer" className="text-accent hover:underline">
      {children}
    </a>
  );
}

const heading = { props: { className: "mt-3 mb-1 text-sm font-semibold first:mt-0" } };

const OPTIONS = {
  disableParsingRawHTML: true,
  overrides: {
    h1: heading,
    h2: heading,
    h3: heading,
    h4: heading,
    p: { props: { className: "my-1.5" } },
    ul: { props: { className: "my-1.5 list-disc space-y-0.5 pl-5" } },
    ol: { props: { className: "my-1.5 list-decimal space-y-0.5 pl-5" } },
    code: { props: { className: "rounded bg-surface-2 px-1 text-xs" } },
    a: NotesLink,
  },
};

/**
 * release-please opens each section with its own "## [1.0.1](compare) (date)"
 * heading. The panel already titles the version and date, so drop it.
 */
function withoutVersionHeading(markdown: string): string {
  return markdown.replace(/^\s*#{1,6}\s*\[?v?\d+\.\d+\.\d+[^\n]*\n*/, "");
}

export function ReleaseNotes({ markdown }: { markdown: string }) {
  const t = useT();
  const body = withoutVersionHeading(markdown);
  if (!body.trim()) return <p className="text-dim">{t("update.noNotes")}</p>;
  return <Markdown options={OPTIONS}>{body}</Markdown>;
}
