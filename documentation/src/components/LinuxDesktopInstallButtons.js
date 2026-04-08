import Link from "@docusaurus/Link";
import { IconDownload } from "@site/src/components/icons/download";
import { useState, useEffect } from "react";

const FALLBACK_URL = "https://github.com/aaif-goose/goose/releases/latest";

const LinuxDesktopInstallButtons = () => {
  const [downloadUrls, setDownloadUrls] = useState({
    deb: FALLBACK_URL,
    rpm: FALLBACK_URL,
    flatpak: FALLBACK_URL
  });

  useEffect(() => {
    const fetchLatestRelease = async () => {
      try {
        // Check cache first (1 hour expiry)
        const cached = localStorage.getItem('goose-release-cache');
        const cacheTime = localStorage.getItem('goose-release-cache-time');
        const now = Date.now();

        if (cached && cacheTime && (now - parseInt(cacheTime)) < 3600000) {
          // Use cached data if less than 1 hour old
          setDownloadUrls(JSON.parse(cached));
          return;
        }

        // Fetch latest release from GitHub API
        const response = await fetch('https://api.github.com/repos/aaif-goose/goose/releases/latest');
        if (!response.ok) throw new Error('API request failed');

        const release = await response.json();
        const assets = release.assets || [];

        // Find DEB, RPM, and Flatpak files
        const debAsset = assets.find(asset => asset.name.includes('.deb') && asset.name.includes('amd64'));
        const rpmAsset = assets.find(asset => asset.name.includes('.rpm') && asset.name.includes('x86_64'));
        const flatpakAsset = assets.find(asset => asset.name.endsWith('.flatpak'));

        const newUrls = {
          deb: debAsset?.browser_download_url || FALLBACK_URL,
          rpm: rpmAsset?.browser_download_url || FALLBACK_URL,
          flatpak: flatpakAsset?.browser_download_url || FALLBACK_URL
        };

        // Update state and cache
        setDownloadUrls(newUrls);
        localStorage.setItem('goose-release-cache', JSON.stringify(newUrls));
        localStorage.setItem('goose-release-cache-time', now.toString());
      } catch (error) {
        console.warn('Failed to fetch latest release, using fallback URLs:', error);
        // Fallback URLs are already set in initial state
      }
    };

    fetchLatestRelease();
  }, []);

  return (
    <div>
      <p>Click one of the buttons below to download goose Desktop for Linux:</p>
      <div className="pill-button" style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
        <Link
          className="button button--primary button--lg"
          to={downloadUrls.deb}
        >
          <IconDownload /> DEB Package (Ubuntu/Debian)
        </Link>
        <Link
          className="button button--primary button--lg"
          to={downloadUrls.rpm}
        >
          <IconDownload /> RPM Package (RHEL/Fedora)
        </Link>
        <Link
          className="button button--primary button--lg"
          to={downloadUrls.flatpak}
        >
          <IconDownload /> Flatpak (Universal)
        </Link>
      </div>
    </div>
  );
};

export default LinuxDesktopInstallButtons;
