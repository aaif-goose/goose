import React, { type ReactNode } from "react";
import OriginalPrimaryMenu from "@theme-original/Navbar/MobileSidebar/PrimaryMenu";
import NavbarItem from "@theme/NavbarItem";
import { useNavbarMobileSidebar } from "@docusaurus/theme-common/internal";
import {
  gdkNavbarItems,
  useGdkNavigation,
} from "@site/src/components/GdkNavigation";

export default function NavbarMobilePrimaryMenu(): ReactNode {
  const isGdk = useGdkNavigation();
  const mobileSidebar = useNavbarMobileSidebar();
  if (!isGdk) {
    return <OriginalPrimaryMenu />;
  }

  return (
    <ul className="menu__list">
      {gdkNavbarItems.map((item) => (
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
