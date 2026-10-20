import { Navigate, Route, Routes } from "react-router";
import { AppShell } from "../components/layout/AppShell";
import { AlbumDetailView } from "../views/albums/AlbumDetailView";
import { AlbumsView } from "../views/albums/AlbumsView";
import { ArtistDetailView } from "../views/artists/ArtistDetailView";
import { ArtistsView } from "../views/artists/ArtistsView";
import { FavouritesView } from "../views/favourites/FavouritesView";
import { DirectoryView, FoldersView } from "../views/folders/FoldersView";
import { GenreDetailView } from "../views/genres/GenreDetailView";
import { GenresView } from "../views/genres/GenresView";
import { HomeView } from "../views/home/HomeView";
import { MixDetailView } from "../views/mixes/MixDetailView";
import { PlaylistDetailView } from "../views/playlists/PlaylistDetailView";
import { PlaylistsView } from "../views/playlists/PlaylistsView";
import { SearchView } from "../views/search/SearchView";
import { SettingsView } from "../views/settings/SettingsView";
import { SongsView } from "../views/songs/SongsView";
import { StatsView } from "../views/stats/StatsView";
import { usePrefsStore } from "../stores/prefsStore";

export function AppRoutes() {
  const startupView = usePrefsStore((s) => s.prefs.startupView);
  return (
    <Routes>
      <Route element={<AppShell />}>
        <Route index element={<Navigate to={startupView} replace />} />
        <Route path="/home" element={<HomeView />} />
        <Route path="/albums" element={<AlbumsView />} />
        <Route path="/albums/:albumId" element={<AlbumDetailView />} />
        <Route path="/songs" element={<SongsView />} />
        <Route path="/artists" element={<ArtistsView />} />
        <Route path="/artists/:artistId" element={<ArtistDetailView />} />
        <Route path="/genres" element={<GenresView />} />
        <Route path="/genres/:genre" element={<GenreDetailView />} />
        <Route path="/folders" element={<FoldersView />} />
        <Route path="/folders/:directoryId" element={<DirectoryView />} />
        <Route path="/playlists" element={<PlaylistsView />} />
        <Route path="/playlists/:playlistId" element={<PlaylistDetailView />} />
        <Route path="/mixes/:mixId" element={<MixDetailView />} />
        <Route path="/favourites" element={<FavouritesView />} />
        <Route path="/stats" element={<StatsView />} />
        <Route path="/search" element={<SearchView />} />
        <Route path="/settings" element={<SettingsView />} />
        <Route path="*" element={<Navigate to="/home" replace />} />
      </Route>
    </Routes>
  );
}
