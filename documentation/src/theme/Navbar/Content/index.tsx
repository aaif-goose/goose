import React, { type ReactNode } from "react";
import OriginalNavbarContent from "@theme-original/Navbar/Content";
import NavbarItem from "@theme/NavbarItem";
import NavbarLogo from "@theme/Navbar/Logo";
import NavbarColorModeToggle from "@theme/Navbar/ColorModeToggle";
import NavbarMobileSidebarToggle from "@theme/Navbar/MobileSidebar/Toggle";
import {
  oiKitNavbarItems,
  useOiKitNavigation,
} from "@site/src/components/OiKitNavigation";
import styles from "./styles.module.css";

export default function NavbarContent(): ReactNode {
  const isOiKit = useOiKitNavigation();
  if (!isOiKit) {
    return <OriginalNavbarContent />;
  }

  return (
    <div className="navbar__inner oi-kit-navigation">
      <div className="navbar__items">
        <NavbarMobileSidebarToggle />
        <NavbarLogo />
      </div>
      <div className="navbar__items navbar__items--right">
        {oiKitNavbarItems.map((item) => (
          <NavbarItem key={item.label} {...item} />
        ))}
        <NavbarColorModeToggle className={styles.colorModeToggle} />
      </div>
    </div>
  );
}
