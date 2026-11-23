import { createContext, useContext } from "react";
import { useLocation } from "@docusaurus/router";
import useBaseUrl from "@docusaurus/useBaseUrl";
import type { Props as NavbarItemConfig } from "@theme/NavbarItem";

export const OiKitBlogContext = createContext(false);

export function useOiKitNavigation(): boolean {
  const { pathname } = useLocation();
  const oiKitPath = useBaseUrl("/docs/oi-kit");
  const blogPath = useBaseUrl("/blog/tags/oi-kit");
  const isOiKitPost = useContext(OiKitBlogContext);

  return (
    isOiKitPost ||
    [oiKitPath, blogPath].some(
      (path) => pathname === path || pathname.startsWith(`${path}/`),
    )
  );
}

export const oiKitNavbarItems = [
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
