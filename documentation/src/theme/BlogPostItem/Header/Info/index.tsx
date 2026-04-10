import React from 'react';
import Info from '@theme-original/BlogPostItem/Header/Info';
import type InfoType from '@theme/BlogPostItem/Header/Info';
import type { WrapperProps } from '@docusaurus/types';
import { useBlogPost } from '@docusaurus/plugin-content-blog/client';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import SocialShare from '@site/src/components/SocialShare';

interface Author {
  name: string;
  image_url?: string;
  title?: string;
}

type Props = WrapperProps<typeof InfoType>;

function buildPostUrl(siteUrl: string, permalink: string): string {
  const base = siteUrl.endsWith('/') ? siteUrl.slice(0, -1) : siteUrl;
  return `${base}${permalink}`;
}

function AuthorDisplay({ authors }: { authors: string[] }) {
  if (!authors || authors.length === 0) return null;

  // Author data - simplified mapping for the key authors
  const authorData: Record<string, Author> = {
    adewale: {
      name: "Adewale Abati",
      image_url: "https://avatars.githubusercontent.com/u/4003538?v=4"
    },
    angie: {
      name: "Angie Jones",
      image_url: "https://avatars.githubusercontent.com/u/15972783?v=4"
    },
    tania: {
      name: "Tania Chakraborty",
      image_url: "https://avatars.githubusercontent.com/u/126204004?v=4"
    },
    mic: {
      name: "Michael Neale",
      image_url: "https://avatars.githubusercontent.com/u/14976?v=4"
    }
  };

  const authorsToDisplay = authors.slice(0, 3);
  const hasMore = authors.length > 3;

  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
      {authorsToDisplay.map((authorKey, index) => {
        const author = authorData[authorKey] || { name: authorKey };

        return (
          <div key={index} style={{ display: 'flex', alignItems: 'center', gap: '0.25rem' }}>
            {author.image_url && (
              <img
                src={author.image_url}
                alt={author.name}
                style={{
                  width: '20px',
                  height: '20px',
                  borderRadius: '50%',
                  border: '1px solid var(--ifm-color-emphasis-200)'
                }}
              />
            )}
            <span style={{ fontSize: '0.85rem', color: 'var(--ifm-color-content-secondary)' }}>
              {author.name}
            </span>
          </div>
        );
      })}
      {hasMore && (
        <span style={{ fontSize: '0.85rem', color: 'var(--ifm-color-content-secondary)' }}>
          +{authors.length - 3} more
        </span>
      )}
    </div>
  );
}

export default function InfoWrapper(props: Props): JSX.Element {
  const { metadata, isBlogPostPage, frontMatter } = useBlogPost();
  const { siteConfig } = useDocusaurusContext();

  const postUrl = buildPostUrl(siteConfig.url, metadata.permalink);
  const authors = frontMatter.authors || [];

  return (
    <div style={{ display: 'flex', alignItems: 'center', flexWrap: 'wrap', gap: '0.5rem' }}>
      <Info {...props} />

      {authors.length > 0 && (
        <>
          <span style={{ margin: '0 0.125rem' }}> · </span>
          <AuthorDisplay authors={authors} />
        </>
      )}

      {isBlogPostPage && (
        <>
          <span style={{ margin: '0 0.125rem' }}> · </span>
          <SocialShare url={postUrl} title={metadata.title} />
        </>
      )}
    </div>
  );
}
