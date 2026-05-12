import Link from "@docusaurus/Link";
import { IconDownload } from "@site/src/components/icons/download";

const WindowsDesktopInstallButtons = () => {
  return (
    <div>
      <p>Click one of the buttons below to download goose Desktop for Windows:</p>
      <div className="pill-button" style={{ display: "flex", gap: "0.75rem", flexWrap: "wrap" }}>
        <Link
          className="button button--primary button--lg"
          to="https://github.com/aaif-goose/goose/releases/download/stable/Goose-win32-x64.zip"
        >
          <IconDownload /> Windows
        </Link>
      </div>
    </div>
  );
};

export default WindowsDesktopInstallButtons;
