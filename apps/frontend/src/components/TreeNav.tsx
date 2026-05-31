import { Link, useLocation } from "@tanstack/react-router";
import { useState, useCallback } from "react";

export type NavTreeItem = {
  id: string;
  label: string;
  to?: string;
  children?: NavTreeItem[];
};

type TreeNavProps = {
  items: NavTreeItem[];
  /** Override pathname for testing. */
  pathname?: string;
};

function isActive(pathname: string, to: string): boolean {
  if (to === "/") return pathname === "/";
  return pathname === to || pathname.startsWith(to + "/");
}

function hasActiveChild(pathname: string, item: NavTreeItem): boolean {
  if (!item.children) return false;
  return item.children.some(
    (child) =>
      (child.to && isActive(pathname, child.to)) ||
      hasActiveChild(pathname, child),
  );
}

function getInitiallyExpanded(
  items: NavTreeItem[],
  pathname: string,
): Set<string> {
  const expanded = new Set<string>();
  for (const item of items) {
    if (item.children && hasActiveChild(pathname, item)) {
      expanded.add(item.id);
    }
  }
  return expanded;
}

export function TreeNav({ items, pathname: pathnameProp }: TreeNavProps) {
  const location = useLocation();
  const pathname = pathnameProp ?? location.pathname;
  const [expanded, setExpanded] = useState<Set<string>>(() =>
    getInitiallyExpanded(items, pathname),
  );

  const toggle = useCallback((id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  return (
    <nav className="tree-nav" aria-label="Primary navigation">
      {items.map((item) => (
        <TreeNode
          key={item.id}
          item={item}
          depth={0}
          expanded={expanded}
          onToggle={toggle}
          pathname={pathname}
        />
      ))}
    </nav>
  );
}

type TreeNodeProps = {
  item: NavTreeItem;
  depth: number;
  expanded: Set<string>;
  onToggle: (id: string) => void;
  pathname: string;
};

function TreeNode({
  item,
  depth,
  expanded,
  onToggle,
  pathname,
}: TreeNodeProps) {
  const isExpanded = expanded.has(item.id);
  const hasChildren = !!item.children && item.children.length > 0;
  const isItemActive = item.to ? isActive(pathname, item.to) : false;
  const hasActiveDescendant = hasChildren
    ? hasActiveChild(pathname, item)
    : false;

  const linkClass =
    `tree-link` +
    `${isItemActive ? " active" : ""}` +
    `${hasActiveDescendant && !isItemActive ? " has-active-child" : ""}` +
    `${depth > 0 ? " indented" : ""}`;

  return (
    <div className="tree-node">
      <div className="tree-node-row">
        <span className="tree-toggle-area">
          {hasChildren && (
            <button
              type="button"
              className={`tree-toggle${isExpanded ? " expanded" : ""}`}
              onClick={() => onToggle(item.id)}
              aria-expanded={isExpanded}
              aria-label={isExpanded ? "Collapse" : "Expand"}
            />
          )}
        </span>
        {item.to ? (
          <Link to={item.to} className={linkClass}>
            {item.label}
          </Link>
        ) : hasChildren ? (
          <button
            type="button"
            className={linkClass}
            onClick={() => onToggle(item.id)}
          >
            {item.label}
          </button>
        ) : (
          <span className={linkClass}>{item.label}</span>
        )}
      </div>
      {hasChildren && isExpanded && (
        <div className="tree-node-children">
          {item.children!.map((child) => (
            <TreeNode
              key={child.id}
              item={child}
              depth={depth + 1}
              expanded={expanded}
              onToggle={onToggle}
              pathname={pathname}
            />
          ))}
        </div>
      )}
    </div>
  );
}
