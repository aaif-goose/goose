import { createContext, useContext } from "react";
import { useLocation } from "@docusaurus/router";
import useBaseUrl from "@docusaurus/useBaseUrl";
import type { Props as NavbarItemConfig } from "@theme/NavbarItem";

export const GdkBlogContext = createContext(false);

export function useGdkNavigation(): boolean {
  const { pathname } = useLocation();
  const gdkPath = useBaseUrl("/docs/oi-kit");
  const blogPath = useBaseUrl("/blog/tags/oi-kit");
  const isGdkPost = useContext(GdkBlogContext);

  return (
    isGdkPost ||
    [gdkPath, blogPath].some(
      (path) => pathname === path || pathname.startsWith(`${path}/`),
    )
  );
}

export const gdkNavbarItems = [
  {
    to: "/blog/tags/oi-kit",
    label: "Blog",
    activeBasePath: "blog",
  },
  {
    href: "https://github.com/aaif-goose/goose",
    label: "GitHub",
  },
  { to: "/", label: "Back to goose", exact: true },
] satisfies NavbarItemConfig[];
