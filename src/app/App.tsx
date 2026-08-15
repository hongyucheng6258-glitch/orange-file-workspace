import { HashRouter, Route, Routes } from "react-router-dom";
import { AppShell } from "../layouts/AppShell";
import { FilePage } from "../features/files/routes/FilePage";
import { PagePage } from "../features/pages/routes/PagePage";
import { ProjectPage } from "../features/projects/routes/ProjectPage";
import { TaskCenterPage } from "../features/tasks/components/TaskCenter";
import { SearchPage } from "../features/search/routes/SearchPage";
import { FavoritesPage } from "../features/favorites/routes/FavoritesPage";
import { TrashPage } from "../features/trash/routes/TrashPage";
import { SettingsPage } from "../features/settings/routes/SettingsPage";
import { PlaceholderPage } from "./PlaceholderPage";

export default function App() {
  return (
    <HashRouter>
      <Routes>
        <Route element={<AppShell />}>
          <Route path="/" element={<PlaceholderPage title="首页" />} />
          <Route path="/files" element={<FilePage />} />
          <Route path="/pages" element={<PagePage />} />
          <Route path="/projects" element={<ProjectPage />} />
          <Route path="/favorites" element={<FavoritesPage />} />
          <Route path="/trash" element={<TrashPage />} />
          <Route path="/tasks" element={<TaskCenterPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="/search" element={<SearchPage />} />
        </Route>
      </Routes>
    </HashRouter>
  );
}
