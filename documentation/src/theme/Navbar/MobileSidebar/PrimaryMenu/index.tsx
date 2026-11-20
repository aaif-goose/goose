import React, { type ReactNode } from "react";
import OriginalPrimaryMenu from "@theme-original/Navbar/MobileSidebar/PrimaryMenu";
import NavbarItem from "@theme/NavbarItem";
import { useNavbarMobileSidebar } from "@docusaurus/theme-common/internal";
import {
  oiKitNavbarItems,
  useOiKitNavigation,
} from "@site/src/components/OiKitNavigation";

export default function NavbarMobilePrimaryMenu(): ReactNode {
  const isOiKit = useOiKitNavigation();
  const mobileSidebar = useNavbarMobileSidebar();
  if (!isOiKit) {
    return <OriginalPrimaryMenu />;
  }

  return (
    <ul className="menu__list">
      {oiKitNavbarItems.map((item) => (
        <NavbarItem
          key={item.label}
          {...item}
          mobile
          onClick={() => mobileSidebar.toggle()}
        />
      ))}
    </ul>
  );
}
