// Cover art URLs served by the Rust reverb-art:// protocol (disk-cached proxy
// to Navidrome's getCoverArt). On Windows, Tauri exposes custom schemes as
// http://<scheme>.localhost/ instead.

const isWindows = navigator.userAgent.includes("Windows");

export const PLACEHOLDER =
  "data:image/svg+xml," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
      <rect width="100" height="100" fill="#26262c"/>
      <circle cx="50" cy="50" r="22" fill="none" stroke="#3d3d46" stroke-width="4"/>
      <circle cx="50" cy="50" r="5" fill="#3d3d46"/>
    </svg>`,
  );

export function artUrl(coverArtId: string | undefined | null, size: number): string {
  if (!coverArtId) return PLACEHOLDER;
  const id = encodeURIComponent(coverArtId);
  return isWindows
    ? `http://reverb-art.localhost/${id}?size=${size}`
    : `reverb-art://localhost/${id}?size=${size}`;
}
