/** The Reverb mark (transparent glyph), served from public/. */
export function Logo({ className = "size-7" }: { className?: string }) {
  return <img src="/logo.png" alt="" draggable={false} className={className} />;
}
