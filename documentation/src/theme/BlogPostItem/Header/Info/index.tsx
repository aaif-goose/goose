import React from 'react';
import Info from '@theme-original/BlogPostItem/Header/Info';
import type InfoType from '@theme/BlogPostItem/Header/Info';
import type { WrapperProps } from '@docusaurus/types';
import { useBlogPost } from '@docusaurus/plugin-content-blog/client';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import SocialShare from '@site/src/components/SocialShare';

type Props = WrapperProps<typeof InfoType>;

function buildPostUrl(siteUrl: string, permalink: string): string {
  const base = siteUrl.endsWith('/') ? siteUrl.slice(0, -1) : siteUrl;
  return `${base}${permalink}`;
}

export default function InfoWrapper(props: Props): JSX.Element {
  const { metadata, isBlogPostPage } = useBlogPost();
  const { siteConfig } = useDocusaurusContext();

  const postUrl = buildPostUrl(siteConfig.url, metadata.permalink);

  return (
    <div style={{ display: 'flex', alignItems: 'center', flexWrap: 'wrap', gap: '0.25rem' }}>
      <Info {...props} />
      {isBlogPostPage && (
        <>
          <span style={{ margin: '0 0.125rem' }}> · </span>
          <SocialShare url={postUrl} title={metadata.title} />
        </>
      )}
    </div>
  );
}
