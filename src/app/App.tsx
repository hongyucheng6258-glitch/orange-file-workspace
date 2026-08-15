import { HashRouter, Route, Routes } from "react-router-dom";
import { AppShell } from "../layouts/AppShell";
import { FilePage } from "../features/files/routes/FilePage";
import { PagePage } from "../features/pages/routes/PagePage";
import { TaskCenterPage } from "../features/tasks/components/TaskCenter";
import { PlaceholderPage } from "./PlaceholderPage";

export default function App() {
  return (
    <HashRouter>
      <Routes>
        <Route element={<AppShell />}>
          <Route path="/" element={<PlaceholderPage title="首页" />} />
          <Route path="/files" element={<FilePage />} />
          <Route path="/pages" element={<PagePage />} />
          <Route path="/projects" element={<PlaceholderPage title="代码项目" />} />
          <Route path="/favorites" element={<PlaceholderPage title="收藏" />} />
          <Route path="/trash" element={<PlaceholderPage title="回收站" />} />
          <Route path="/tasks" element={<TaskCenterPage />} />
          <Route path="/settings" element={<PlaceholderPage title="设置" />} />
          <Route path="/search" element={<PlaceholderPage title="搜索" />} />
        </Route>
      </Routes>
    </HashRouter>
  );
}
