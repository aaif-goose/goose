import React, { type ReactNode } from "react";
import Link from "@docusaurus/Link";
import GooseLogo from "@site/static/img/goose.svg";
import OriginalNavbarLogo from "@theme-original/Navbar/Logo";
import { useOiKitNavigation } from "@site/src/components/OiKitNavigation";
import styles from "./styles.module.css";

export default function NavbarLogo(): ReactNode {
  const isOiKit = useOiKitNavigation();
  return isOiKit ? (
    <Link className="navbar__brand margin-right--md" to="/docs/oi-kit">
      <GooseLogo
        aria-hidden="true"
        width={28}
        height={28}
        className={`${styles.mark} margin-right--sm`}
      />
      <b className="navbar__title">oi-kit</b>
    </Link>
  ) : (
    <OriginalNavbarLogo />
  );
}
