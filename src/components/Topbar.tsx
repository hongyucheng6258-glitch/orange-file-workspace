import { useState } from "react";
import { Search, ChevronDown, SlidersHorizontal } from "lucide-react";
import { useNavigate } from "react-router-dom";

export function Topbar() {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (query.trim()) {
      navigate(`/search?q=${encodeURIComponent(query.trim())}`);
    }
  };

  return (
    <header className="topbar">
      <form className="topbar-search" onSubmit={submit}>
        <Search size={15} className="search-icon" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="搜索文件、页面、项目…"
        />
        <button type="submit" className="search-kbd" title="搜索">
          <SlidersHorizontal size={14} />
        </button>
      </form>
      <div className="topbar-right">
        <button className="btn btn-ghost">
          最近 <ChevronDown size={13} />
        </button>
      </div>
    </header>
  );
}
