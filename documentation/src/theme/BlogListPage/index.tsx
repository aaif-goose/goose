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

function AuthorDisplay({ authors }: { authors: string[] }) {
  if (!authors || authors.length === 0) return null;

  // Author data mapping
  const authorData: Record<string, { name: string; image_url?: string }> = {
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
    <div className={styles.postAuthors}>
      {authorsToDisplay.map((authorKey, index) => {
        const author = authorData[authorKey] || { name: authorKey };
        return (
          <div key={index} className={styles.authorInfo}>
            {author.image_url && (
              <img
                src={author.image_url}
                alt={author.name}
                className={styles.authorAvatar}
              />
            )}
            <span className={styles.authorName}>{author.name}</span>
          </div>
        );
      })}
      {hasMore && <span className={styles.authorName}>+{authors.length - 3} more</span>}
    </div>
  );
}

function FeaturedPost({ post }: { post: any }) {
  const postUrl = useBaseUrl(post.content.metadata.permalink);

  // Get image from frontmatter
  const image = post.content.frontMatter.image;
  let imageUrl = null;

  if (image) {
    // Simple path construction - images are directly in /img/blog/
    imageUrl = useBaseUrl(`/img/blog/${image}`);
  }

  const authors = post.content.frontMatter.authors || [];

  return (
    <article className={styles.featuredPost}>
      <div className={styles.featuredContent}>
        <h2 className={styles.featuredTitle}>
          <a href={postUrl}>{post.content.metadata.title}</a>
        </h2>

        <div className={styles.featuredMeta}>
          <AuthorDisplay authors={authors} />
          <span>{new Date(post.content.metadata.date).toLocaleDateString()}</span>
        </div>

        <div className={styles.featuredDescription}>
          {post.content.metadata.description || post.content.frontMatter.description}
        </div>

        <a href={postUrl} className={styles.featuredButton}>
          Read full article
        </a>
      </div>

      {imageUrl && (
        <div className={styles.featuredImage}>
          <img src={imageUrl} alt={post.content.metadata.title} />
        </div>
      )}
    </article>
  );
}

function BlogPostGrid({ posts }: { posts: any[] }) {
  return (
    <div className={styles.postsGrid}>
      {posts.map((post, index) => {
        const postUrl = useBaseUrl(post.content.metadata.permalink);

        // Get image from frontmatter
        const image = post.content.frontMatter.image;
        let imageUrl = null;

        if (image) {
          // Simple path construction - images are directly in /img/blog/
          imageUrl = useBaseUrl(`/img/blog/${image}`);
        }

        const authors = post.content.frontMatter.authors || [];

        return (
          <article key={index} className={styles.postCard}>
            {imageUrl && (
              <div className={styles.postImage}>
                <img src={imageUrl} alt={post.content.metadata.title} />
              </div>
            )}

            <div className={styles.postContent}>
              <div className={styles.postDate}>
                {new Date(post.content.metadata.date).toLocaleDateString()}
              </div>

              <h3 className={styles.postTitle}>
                <a href={postUrl}>{post.content.metadata.title}</a>
              </h3>

              <div className={styles.postAuthors}>
                <AuthorDisplay authors={authors} />
              </div>

              <div className={styles.postDescription}>
                {post.content.metadata.description || post.content.frontMatter.description}
              </div>
            </div>
          </article>
        );
      })}
    </div>
  );
}

function BlogListPageContent(props: Props): ReactNode {
  const { metadata, items, sidebar } = props;

  // Check if this is the first page
  const isFirstPage = !metadata.permalink.includes('/page/');

  // Filter valid items
  const validItems = items.filter(item =>
    item.content &&
    item.content.metadata &&
    item.content.metadata.title &&
    item.content.frontMatter
  );

  // Find featured posts (only show on first page)
  const featuredPosts = isFirstPage
    ? validItems.filter(item => item.content.frontMatter.featured === true)
    : [];

  // Get regular posts (exclude featured posts on first page)
  const regularPosts = isFirstPage
    ? validItems.filter(item => item.content.frontMatter.featured !== true)
    : validItems;

  return (
    <BlogLayout sidebar={undefined}>
      <div className={styles.blogContainer}>
        {/* Featured Posts Section */}
        {featuredPosts.length > 0 && (
          <div className={styles.featuredSection}>
            {featuredPosts.slice(0, 1).map((post, index) => (
              <FeaturedPost key={index} post={post} />
            ))}
          </div>
        )}

        {/* Regular Posts Grid */}
        {regularPosts.length > 0 && (
          <BlogPostGrid posts={regularPosts} />
        )}

        {/* Pagination */}
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