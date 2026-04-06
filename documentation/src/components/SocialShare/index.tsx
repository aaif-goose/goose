import React, { useState, useRef, useEffect, useMemo } from 'react';
import { Share2, Link, Check } from 'lucide-react';
import { IconTwitter } from '@site/src/components/icons/twitter';
import { IconLinkedIn } from '@site/src/components/icons/linkedin';
import { IconFacebook } from '@site/src/components/icons/facebook';
import { IconReddit } from '@site/src/components/icons/reddit';
import styles from './styles.module.css';

const TWITTER_VIA = 'goose_oss';

interface SocialShareProps {
  url: string;
  title: string;
}

const SocialShare: React.FC<SocialShareProps> = ({ url, title }) => {
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  const shareOptions = useMemo(() => {
    const encodedUrl = encodeURIComponent(url);
    const encodedTitle = encodeURIComponent(title);

    return [
      {
        name: 'Twitter / X',
        icon: <IconTwitter />,
        url: `https://twitter.com/intent/tweet?url=${encodedUrl}&text=${encodedTitle}&via=${TWITTER_VIA}`,
      },
      {
        name: 'LinkedIn',
        icon: <IconLinkedIn />,
        url: `https://www.linkedin.com/sharing/share-offsite/?url=${encodedUrl}`,
      },
      {
        name: 'Facebook',
        icon: <IconFacebook />,
        url: `https://www.facebook.com/sharer/sharer.php?u=${encodedUrl}`,
      },
      {
        name: 'Reddit',
        icon: <IconReddit />,
        url: `https://reddit.com/submit?url=${encodedUrl}&title=${encodedTitle}`,
      },
    ];
  }, [url, title]);

  // Close menu on outside click or Escape
  useEffect(() => {
    if (!open) return;

    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };

    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleEscape);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleEscape);
    };
  }, [open]);

  const openShareWindow = (shareUrl: string) => {
    window.open(shareUrl, '_blank', 'width=600,height=500,noopener,noreferrer');
    setOpen(false);
  };

  const handleCopyLink = async () => {
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => {
        setCopied(false);
        setOpen(false);
      }, 1500);
    } catch (err) {
      console.error('Failed to copy:', err);
    }
  };

  return (
    <div className={styles.wrapper} ref={menuRef}>
      <button
        onClick={() => setOpen((prev) => !prev)}
        className={styles.shareButton}
        aria-label="Share this post"
        aria-expanded={open}
        aria-haspopup="true"
        title="Share this post"
      >
        <Share2 size={16} />
        <span>Share</span>
      </button>

      {open && (
        <div className={styles.dropdown} role="menu">
          {shareOptions.map((option) => (
            <button
              key={option.name}
              className={styles.dropdownItem}
              onClick={() => openShareWindow(option.url)}
              role="menuitem"
            >
              {option.icon}
              <span>{option.name}</span>
            </button>
          ))}
          <div className={styles.divider} />
          <button
            className={styles.dropdownItem}
            onClick={handleCopyLink}
            role="menuitem"
          >
            {copied ? <Check size={16} /> : <Link size={16} />}
            <span>{copied ? 'Copied!' : 'Copy link'}</span>
          </button>
        </div>
      )}
    </div>
  );
};

export default SocialShare;
