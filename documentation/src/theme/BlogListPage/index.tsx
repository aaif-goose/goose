import React, {type ReactNode} from 'react';
import clsx from 'clsx';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useBaseUrl from '@docusaurus/useBaseUrl';
import {
  PageMetadata,
  HtmlClassNameProvider,
  ThemeClassNames,
} from '@docusaurus/theme-common';
import BlogLayout from '@theme/BlogLayout';
import BlogListPaginator from '@theme/BlogListPaginator';
import SearchMetadata from '@theme/SearchMetadata';
import type {Props} from '@theme/BlogListPage';
import BlogListPageStructuredData from '@theme/BlogListPage/StructuredData';
import styles from './styles.module.css';

function BlogListPageMetadata(props: Props): ReactNode {
  const {metadata} = props;
  const {
    siteConfig: {title: siteTitle},
  } = useDocusaurusContext();
  const {blogDescription, blogTitle, permalink} = metadata;
  const isBlogOnlyMode = permalink === '/';
  const title = isBlogOnlyMode ? siteTitle : blogTitle;
  return (
    <>
      <PageMetadata title={title} description={blogDescription} />
      <SearchMetadata tag="blog_posts_list" />
    </>
  );
}

const getAuthorName = (author: any): string =>
  typeof author === 'string' ? author : (author.name || author.key || author);

function AuthorDisplay({ authors }: { authors: any[] }) {
  if (!authors?.length) return null;

  const authorsToDisplay = authors.slice(0, 3);
  const hasMore = authors.length > 3;
  const hasResolvedAuthors = authorsToDisplay.some(author =>
    typeof author === 'object' && (author.imageURL || author.image_url)
  );

  if (hasResolvedAuthors) {
    return (
      <div className={styles.postAuthors}>
        {authorsToDisplay.map((author, index) => (
          <div key={index} className={styles.authorInfo}>
            {(author.imageURL || author.image_url) && (
              <img
                src={author.imageURL || author.image_url}
                alt={getAuthorName(author)}
                className={styles.authorAvatar}
              />
            )}
            <span className={styles.authorName}>{getAuthorName(author)}</span>
          </div>
        ))}
        {hasMore && <span className={styles.authorName}>+{authors.length - 3} more</span>}
      </div>
    );
  }

  const authorNames = authorsToDisplay.map(getAuthorName);
  const displayText = authorNames.join(', ') + (hasMore ? `, +${authors.length - 3} more` : '');

  return (
    <div className={styles.postAuthors}>
      <span className={styles.authorName}>{displayText}</span>
    </div>
  );
}

function FeaturedPost({ post }: { post: any }) {
  const url = useBaseUrl(post.content.metadata.permalink);
  const imageUrl = post.content.frontMatter.image ? useBaseUrl(post.content.frontMatter.image) : null;
  const title = post.content.metadata.title;
  const formattedDate = post.content.metadata.formattedDate;
  const description = post.content.metadata.description || post.content.frontMatter.description;
  const authors = post.content?.metadata?.authors || post.content?.frontMatter?.authors || [];

  return (
    <article className={styles.featuredPost}>
      <div className={styles.featuredContent}>
        <div className={styles.featuredDate}>{formattedDate}</div>
        <h2 className={styles.featuredTitle}>
          <a href={url}>{title}</a>
        </h2>
        <AuthorDisplay authors={authors} />
        <div className={styles.featuredDescription}>{description}</div>
        <a href={url} className={styles.featuredButton}>Read full article</a>
      </div>
      {imageUrl && (
        <div className={styles.featuredImage}>
          <img src={imageUrl} alt={title} />
        </div>
      )}
    </article>
  );
}

function BlogPostCard({ post }: { post: any }) {
  const url = useBaseUrl(post.content.metadata.permalink);
  const imageUrl = post.content.frontMatter.image ? useBaseUrl(post.content.frontMatter.image) : null;
  const title = post.content.metadata.title;
  const formattedDate = post.content.metadata.formattedDate;
  const description = post.content.metadata.description || post.content.frontMatter.description;
  const authors = post.content?.metadata?.authors || post.content?.frontMatter?.authors || [];

  return (
    <article className={styles.postCard}>
      {imageUrl && (
        <div className={styles.postImage}>
          <img src={imageUrl} alt={title} />
        </div>
      )}
      <div className={styles.postContent}>
        <div className={styles.postDate}>{formattedDate}</div>
        <h3 className={styles.postTitle}>
          <a href={url}>{title}</a>
        </h3>
        <AuthorDisplay authors={authors} />
        <div className={styles.postDescription}>{description}</div>
      </div>
    </article>
  );
}

function BlogPostGrid({ posts }: { posts: any[] }) {
  return (
    <div className={styles.postsGrid}>
      {posts.map((post, index) => (
        <BlogPostCard key={index} post={post} />
      ))}
    </div>
  );
}

function BlogListPageContent(props: Props): ReactNode {
  const { metadata, items } = props;
  const isFirstPage = !metadata.permalink.includes('/page/');

  const validItems = items.filter(item =>
    item.content?.metadata?.title && item.content?.frontMatter
  );

  const featuredPosts = isFirstPage
    ? validItems.filter(item => item.content.frontMatter.featured === true)
    : [];

  const regularPosts = isFirstPage
    ? validItems.filter(item => item !== featuredPosts[0])
    : validItems;

  return (
    <BlogLayout sidebar={undefined}>
      <div className={styles.blogContainer}>
        {featuredPosts.length > 0 && (
          <div className={styles.featuredSection}>
            <FeaturedPost post={featuredPosts[0]} />
          </div>
        )}
        {regularPosts.length > 0 && <BlogPostGrid posts={regularPosts} />}
        <div className={styles.paginationWrapper}>
          <BlogListPaginator metadata={metadata} />
        </div>
      </div>
    </BlogLayout>
  );
}

export default function BlogListPage(props: Props): ReactNode {
  return (
    <HtmlClassNameProvider
      className={clsx(
        ThemeClassNames.wrapper.blogPages,
        ThemeClassNames.page.blogListPage,
      )}>
      <BlogListPageMetadata {...props} />
      <BlogListPageStructuredData {...props} />
      <BlogListPageContent {...props} />
    </HtmlClassNameProvider>
  );
}