import { CATEGORIES, GROUPS, type CategoryId } from "../lib/categories";
import { useSettings } from "../lib/settings";
import { Icon } from "./Icon";
import "./Sidebar.css";

interface SidebarProps {
  active: CategoryId;
  onSelect: (id: CategoryId) => void;
  onOpenSettings: () => void;
}

export function Sidebar({ active, onSelect, onOpenSettings }: SidebarProps) {
  const { settings } = useSettings();
  const visible = new Set(settings.visibleCategories);

  const groups = GROUPS.map((group) => ({
    ...group,
    items: CATEGORIES.filter((c) => c.group === group.id && visible.has(c.id)),
  })).filter((g) => g.items.length > 0);

  return (
    <nav className="sidebar" aria-label="Categories">
      <div className="sidebar__scroll scroll">
        {groups.map((group) => (
          <section className="sidebar__group" key={group.id}>
            <h2 className="sidebar__group-title">{group.label}</h2>
            <ul className="sidebar__list">
              {group.items.map((cat) => (
                <li key={cat.id}>
                  <button
                    type="button"
                    className="sidebar__item"
                    aria-current={cat.id === active ? "page" : undefined}
                    onClick={() => onSelect(cat.id)}
                  >
                    <Icon name={cat.id} className="sidebar__icon" />
                    <span className="sidebar__label">{cat.label}</span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        ))}

        {groups.length === 0 && (
          <p className="sidebar__empty">
            Every category is hidden. Turn some back on in Settings.
          </p>
        )}
      </div>

      <footer className="sidebar__footer">
        <button type="button" className="sidebar__item" onClick={onOpenSettings}>
          <Icon name="settings" className="sidebar__icon" />
          <span className="sidebar__label">Settings</span>
        </button>
      </footer>
    </nav>
  );
}
