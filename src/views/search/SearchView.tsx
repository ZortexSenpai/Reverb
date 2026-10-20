import { useQuery } from "@tanstack/react-query";
import { keepPreviousData } from "@tanstack/react-query";
import { Search as SearchIcon } from "lucide-react";
import { useEffect, useRef } from "react";
import { useSearchParams } from "react-router";
import { AlbumCard } from "../../components/library/AlbumCard";
import { CoverImage } from "../../components/library/CoverImage";
import { TrackList } from "../../components/library/TrackList";
import { Input } from "../../components/ui/Input";
import { Spinner } from "../../components/ui/Spinner";
import { useT } from "../../lib/i18n";
import { cmd } from "../../lib/ipc/commands";
import { qk } from "../../lib/subsonic/keys";
import { useDebounced } from "../../lib/useDebounced";
import { useNavigate } from "react-router";

export function SearchView() {
  const t = useT();
  const [params, setParams] = useSearchParams();
  const query = params.get("q") ?? "";
  const debouncedQuery = useDebounced(query.trim(), 250);
  const inputRef = useRef<HTMLInputElement>(null);
  const navigate = useNavigate();

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const search = useQuery({
    queryKey: qk.search(debouncedQuery),
    queryFn: () =>
      cmd("library_search", {
        query: debouncedQuery,
        artistCount: 6,
        albumCount: 12,
        songCount: 30,
      }),
    enabled: debouncedQuery.length >= 2,
    placeholderData: keepPreviousData,
    staleTime: 30_000,
  });

  const result = search.data;
  const nothing =
    result && result.song.length === 0 && result.album.length === 0 && result.artist.length === 0;

  return (
    <div className="flex h-full flex-col">
      <div className="px-6 py-4">
        <div className="relative max-w-lg">
          <SearchIcon className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-faint" />
          <Input
            ref={inputRef}
            value={query}
            onChange={(e) => setParams({ q: e.target.value }, { replace: true })}
            placeholder={t("search.placeholder")}
            className="pl-9"
            spellCheck={false}
          />
          {search.isFetching && (
            <Spinner className="absolute top-1/2 right-3 size-4 -translate-y-1/2" />
          )}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {debouncedQuery.length < 2 ? (
          <div className="py-24 text-center text-sm text-faint">{t("search.hint")}</div>
        ) : nothing ? (
          <div className="py-24 text-center text-sm text-dim">
            {t("search.noResults", { q: debouncedQuery })}
          </div>
        ) : result ? (
          <div className="px-4 pb-8">
            {result.artist.length > 0 && (
              <section className="pt-2">
                <h2 className="px-2 pb-2 text-lg font-semibold">{t("nav.artists")}</h2>
                <div className="flex flex-wrap gap-2 px-2">
                  {result.artist.map((artist) => (
                    <button
                      key={artist.id}
                      onClick={() => navigate(`/artists/${artist.id}`)}
                      className="flex w-36 cursor-pointer flex-col items-center gap-2 rounded-lg p-3 transition-colors hover:bg-surface"
                    >
                      <CoverImage
                        coverArt={artist.coverArt}
                        size={200}
                        alt=""
                        className="w-full rounded-full"
                      />
                      <div className="w-full truncate text-center text-sm font-medium">
                        {artist.name}
                      </div>
                    </button>
                  ))}
                </div>
              </section>
            )}
            {result.album.length > 0 && (
              <section className="pt-6">
                <h2 className="px-2 pb-2 text-lg font-semibold">{t("nav.albums")}</h2>
                <div className="grid grid-cols-[repeat(auto-fill,minmax(190px,1fr))] gap-1">
                  {result.album.map((album) => (
                    <AlbumCard key={album.id} album={album} />
                  ))}
                </div>
              </section>
            )}
            {result.song.length > 0 && (
              <section className="pt-6">
                <h2 className="px-2 pb-2 text-lg font-semibold">{t("nav.songs")}</h2>
                <TrackList songs={result.song} showArt />
              </section>
            )}
          </div>
        ) : null}
      </div>
    </div>
  );
}
