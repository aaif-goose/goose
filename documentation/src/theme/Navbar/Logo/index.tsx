import React, { type ReactNode } from "react";
import Link from "@docusaurus/Link";
import GooseLogo from "@site/static/img/goose.svg";
import OriginalNavbarLogo from "@theme-original/Navbar/Logo";
import { useGdkNavigation } from "@site/src/components/GdkNavigation";
import styles from "./styles.module.css";

export default function NavbarLogo(): ReactNode {
  const isGdk = useGdkNavigation();
  return isGdk ? (
    <Link className="navbar__brand margin-right--md" to="/docs/gdk">
      <GooseLogo
        aria-hidden="true"
        width={28}
        height={28}
        className={`${styles.mark} margin-right--sm`}
      />
      <b className="navbar__title">GDK</b>
    </Link>
  ) : (
    <OriginalNavbarLogo />
  );
}
