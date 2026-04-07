import React from 'react';
import { BookOpen } from 'lucide-react';

type ContentType = 'video' | 'blog' | 'topic';

interface ContentCardProps {
  type: ContentType;
  title: string;
  description: string;
  thumbnailUrl?: string; // meta url or ES6 import for blogs
  linkUrl: string;
  date?: string;
  duration?: string; // e.g. '6:04' for videos and '5 min read' for blogs
  size?: 'large' | 'compact';
}

const styles = {
  cardContainer: {
    display: 'flex',
    flexDirection: 'row' as const,
    width: '100%',
    border: '1px solid var(--ifm-color-emphasis-200)',
    borderRadius: '12px',
    textDecoration: 'none',
    color: 'inherit',
    overflow: 'hidden',
    background: 'var(--ifm-background-color)',
    transition: 'box-shadow 0.2s ease, transform 0.2s ease',
    marginBottom: '1rem',
  },
  cardContainerLarge: {
    width: '100%',
    maxWidth: '500px',
    aspectRatio: '16/9',
  },
  cardContainerCompact: {
    width: '100%',
    maxWidth: '350px',
    aspectRatio: '16/9',
  },
  cardHover: {
    boxShadow: '0 4px 12px rgba(0,0,0,0.1)',
    transform: 'translateY(-2px)',
  },

  mainArea: {
    flex: 1,
    display: 'flex',
    flexDirection: 'column' as const,
  },
  thumbnailWrapper: {
    position: 'relative' as const,
    width: '100%',
    height: '100%',
    paddingBottom: 0,
    overflow: 'hidden' as const,
    background: 'var(--ifm-color-emphasis-100)',
  },
  thumbnail: {
    position: 'absolute' as const,
    top: 0,
    left: 0,
    width: '100%',
    height: '100%',
    objectFit: 'cover' as const,
  },
  placeholderLogo: {
    position: 'absolute' as const,
    top: '50%',
    left: '50%',
    transform: 'translate(-50%, -50%)',
    width: '64px',
    height: '64px',
    opacity: 0.6,
  },

  hoverOverlay: {
    position: 'absolute' as const,
    top: 0,
    left: 0,
    right: 0,
    bottom: 0,
    background: 'rgba(0, 0, 0, 0.9)',
    color: 'white',
    padding: '1.25rem',
    display: 'flex',
    flexDirection: 'column' as const,
    justifyContent: 'center',
    opacity: 0,
    transition: 'opacity 0.3s ease',
    zIndex: 10,
    borderRadius: '12px',
  },
  hoverOverlayVisible: {
    opacity: 1,
  },
  hoverTitle: {
    fontSize: '1.1rem',
    fontWeight: '600' as const,
    marginBottom: '0.5rem',
    color: 'white',
  },
  hoverDescription: {
    fontSize: '0.875rem',
    lineHeight: '1.4',
    marginBottom: '0.75rem',
    color: 'rgba(255, 255, 255, 0.9)',
  },
  hoverMetadata: {
    fontSize: '0.75rem',
    color: 'rgba(255, 255, 255, 0.8)',
    fontWeight: '600' as const,
    display: 'flex',
    justifyContent: 'space-between',
    marginTop: 'auto',
  },

  hoverBookIcon: {
    width: '20px',
    height: '20px',
    color: 'white',
    marginLeft: '4px',
  },
};

export default function ContentCard({
  type,
  title,
  description,
  thumbnailUrl,
  linkUrl,
  date,
  duration,
  size = 'compact',
}: ContentCardProps) {
  const [isHovering, setIsHovering] = React.useState(false);
  const [isTouchDevice, setIsTouchDevice] = React.useState(false);
  const isCompact = size === 'compact';
  const showHoverOverlay = true;

  // Detect touch device on mount
  React.useEffect(() => {
    setIsTouchDevice('ontouchstart' in window || navigator.maxTouchPoints > 0);
  }, []);

  const containerStyle = {
    ...styles.cardContainer,
    ...(isCompact ? styles.cardContainerCompact : styles.cardContainerLarge),
    ...(isHovering ? styles.cardHover : {}),
    position: 'relative' as const,
  };

  const thumbnailWrapperStyle = styles.thumbnailWrapper;

  const hoverOverlayStyle = {
    ...styles.hoverOverlay,
    ...(isHovering && showHoverOverlay && !isTouchDevice ? styles.hoverOverlayVisible : {}),
    ...(size === 'large' ? {
      padding: '2.00rem',
    } : {}),
  };

  const hoverTitleStyle = {
    ...styles.hoverTitle,
    ...(size === 'large' ? {
      fontSize: '1.4rem',
    } : {}),
  };

  const hoverDescriptionStyle = {
    ...styles.hoverDescription,
    ...(size === 'large' ? {
      fontSize: '1.1rem',
    } : {}),
  };

  const hoverMetadataStyle = {
    ...styles.hoverMetadata,
    ...(size === 'large' ? {
      fontSize: '0.9rem',
    } : {}),
  };

  const formatDate = (dateString: string) => {
    const [year, month, day] = dateString.split('-').map(Number);
    const date = new Date(year, month - 1, day);
    return date.toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric'
    });
  };

  if (type === 'topic') {
    const topicOverlayStyle = {
      ...styles.hoverOverlay,
      opacity: 1,
      ...(size === 'large' ? {
        padding: '2.00rem',
      } : {}),
    };

    return (
      <a
        href={linkUrl}
        style={containerStyle}
        onMouseEnter={() => !isTouchDevice && setIsHovering(true)}
        onMouseLeave={() => !isTouchDevice && setIsHovering(false)}
      >
        <div style={styles.mainArea}>
          <div style={topicOverlayStyle}>
            <h3 style={hoverTitleStyle}>{title}</h3>
            <p style={hoverDescriptionStyle}>{description}</p>
            <div style={hoverMetadataStyle}>
              <div>
                <span>DOCUMENTATION</span>
              </div>
              <div></div>
              <div style={{ display: 'flex', alignItems: 'center' }}>
                <BookOpen style={styles.hoverBookIcon} />
              </div>
            </div>
          </div>
        </div>
      </a>
    );
  }

  return (
    <a
      href={linkUrl}
      style={containerStyle}
      onMouseEnter={() => !isTouchDevice && setIsHovering(true)}
      onMouseLeave={() => !isTouchDevice && setIsHovering(false)}
    >

      <div style={styles.mainArea}>
        <div style={thumbnailWrapperStyle}>
          {thumbnailUrl ? (
            <img
              style={styles.thumbnail}
              src={thumbnailUrl}
              alt={`Thumbnail for ${title}`}
            />
          ) : (
            <img
              style={styles.placeholderLogo}
              src="/img/goose.svg"
              alt="Goose logo placeholder"
            />
          )}
        </div>
      </div>

      {showHoverOverlay && !isTouchDevice && (
        <div style={hoverOverlayStyle}>
          <h3 style={hoverTitleStyle}>{title}</h3>
          <p style={hoverDescriptionStyle}>{description}</p>
          <div style={hoverMetadataStyle}>
            <div>
              <span>{type.toUpperCase()}</span>
            </div>
            <div>
              {date && <span>{formatDate(date)}</span>}
            </div>
            <div>
              {duration && <span>{duration}</span>}
              {type === 'blog' && !duration && <span>5 min read</span>}
            </div>
          </div>
        </div>
      )}
    </a>
  );
}
