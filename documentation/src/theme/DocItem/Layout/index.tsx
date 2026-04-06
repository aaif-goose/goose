import React, {type ReactNode, useState, useEffect, useRef} from 'react';
import type LayoutType from '@theme/DocItem/Layout';
import type {WrapperProps} from '@docusaurus/types';
import {useDoc} from '@docusaurus/plugin-content-docs/client';
import clsx from 'clsx';
import {useWindowSize, ThemeClassNames} from '@docusaurus/theme-common';
import DocItemPaginator from '@theme/DocItem/Paginator';
import DocVersionBanner from '@theme/DocVersionBanner';
import DocVersionBadge from '@theme/DocVersionBadge';
import DocItemFooter from '@theme/DocItem/Footer';
import DocItemTOCMobile from '@theme/DocItem/TOC/Mobile';
import DocItemTOCDesktop from '@theme/DocItem/TOC/Desktop';
import DocBreadcrumbs from '@theme/DocBreadcrumbs';
import ContentVisibility from '@theme/ContentVisibility';
import Heading from '@theme/Heading';
import MDXContent from '@theme/MDXContent';
import {Copy, Check, ChevronDown, ExternalLink, FileCode, Bot} from 'lucide-react';
import layoutStyles from './styles.module.css';
import TurndownService from 'turndown';

type Props = WrapperProps<typeof LayoutType>;

// Constants for better maintainability
const COPY_FEEDBACK_DURATION = 2000;

// Component for the Copy Page button
function CopyPageButton(): ReactNode {
  const [copied, setCopied] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [isClient, setIsClient] = useState(false);
  
  // Ensure we're on the client side to avoid hydration issues
  useEffect(() => {
    setIsClient(true);
  }, []);
  
  const handleCopy = async () => {
    // Ensure we're on client side and clipboard API is available
    if (!isClient || typeof window === 'undefined' || !navigator.clipboard) {
      setError('Clipboard not supported in this browser');
      setTimeout(() => setError(null), COPY_FEEDBACK_DURATION);
      return;
    }

    setIsLoading(true);
    setError(null);
    
    try {
      // Find the article element that contains the main content
      const articleElement = document.querySelector('article');
      
      if (!articleElement) {
        throw new Error('Could not find article content');
      }

      // Clone the article to avoid modifying the actual DOM
      const clonedArticle = articleElement.cloneNode(true) as HTMLElement;
      
      // Remove elements we don't want in the markdown
      const elementsToRemove = [
        '.breadcrumbs',           // Breadcrumb navigation
        '.theme-doc-version-badge', // Version badge
        '.theme-doc-version-banner', // Version banner
        '.pagination-nav',        // Previous/Next navigation
        '.theme-doc-footer',      // Footer
        '.theme-doc-toc-mobile',  // Mobile TOC
        'button',                 // All buttons (including copy buttons)
        '.hash-link',             // Hash links on headings
      ];
      
      elementsToRemove.forEach(selector => {
        clonedArticle.querySelectorAll(selector).forEach(el => el.remove());
      });

      // Initialize Turndown service
      const turndownService = new TurndownService({
        headingStyle: 'atx',      // Use # for headings
        codeBlockStyle: 'fenced',  // Use ``` for code blocks
        bulletListMarker: '-',     // Use - for bullet lists
      });

      // Add custom rule for video embeds (iframes and video tags)
      turndownService.addRule('videoEmbeds', {
        filter: function (node) {
          if (node.nodeName === 'IFRAME') {
            const src = (node as HTMLElement).getAttribute('src') || '';
            // Check if it's a video embed (YouTube, Vimeo, etc.)
            return src.includes('youtube.com') || src.includes('vimeo.com') || src.includes('youtu.be');
          }
          if (node.nodeName === 'VIDEO') {
            return true;
          }
          return false;
        },
        replacement: function (content, node) {
          const element = node as HTMLElement;
          const title = element.getAttribute('title') || 'Video';
          const src = element.getAttribute('src') || '';
          
          // For YouTube embeds, convert to a watch URL
          let videoUrl = src;
          if (src.includes('youtube.com/embed/')) {
            const videoId = src.match(/youtube\.com\/embed\/([^?]+)/)?.[1];
            if (videoId) {
              videoUrl = `https://www.youtube.com/watch?v=${videoId}`;
            }
          }
          
          return `\n\n**🎥 [${title}](${videoUrl})**\n\n`;
        }
      });

      // Add custom rule for VideoCarousel component
      turndownService.addRule('videoCarousel', {
        filter: function (node) {
          if (node.nodeName !== 'DIV') return false;
          const element = node as HTMLElement;
          // Check if this is a carousel container
          return element.classList.contains('carousel-container');
        },
        replacement: function (content, node) {
          const carouselElement = node as HTMLElement;
          let markdown = '\n\n';
          
          // Find all video slides
          const slides = carouselElement.querySelectorAll('.swiper-slide');
          
          slides.forEach((slide, index) => {
            const iframe = slide.querySelector('iframe');
            const video = slide.querySelector('video');
            const descElement = slide.querySelector('div[style*="marginTop"]');
            
            if (iframe || video) {
              const element = (iframe || video) as HTMLElement;
              const title = element.getAttribute('title') || `Video ${index + 1}`;
              const src = element.getAttribute('src') || '';
              
              // Convert YouTube embed URLs to watch URLs
              let videoUrl = src;
              if (src.includes('youtube.com/embed/')) {
                const videoId = src.match(/youtube\.com\/embed\/([^?]+)/)?.[1];
                if (videoId) {
                  videoUrl = `https://www.youtube.com/watch?v=${videoId}`;
                }
              }
              
              markdown += `- **🎥 [${title}](${videoUrl})**`;
              
              // Add description if available
              if (descElement) {
                const description = descElement.textContent?.trim();
                if (description) {
                  markdown += `\n  ${description}`;
                }
              }
              
              markdown += '\n';
            }
          });
          
          return markdown + '\n';
        }
      });

      // Add custom rule for category sections with card grids
      turndownService.addRule('categorySections', {
        filter: function (node) {
          if (node.nodeName !== 'DIV') return false;
          const element = node as HTMLElement;
          const classList = Array.from(element.classList);
          return classList.some(className => className.includes('categorySection'));
        },
        replacement: function (content, node) {
          const sectionElement = node as HTMLElement;
          let markdown = '\n\n';
          
          // Get the section title (h2 with emoji)
          const titleElement = sectionElement.querySelector('h2');
          if (titleElement) {
            markdown += `## ${titleElement.textContent?.trim()}\n\n`;
          }
          
          // Find all card links in the grid
          const cardLinks = sectionElement.querySelectorAll('a');
          cardLinks.forEach(cardLink => {
            const href = cardLink.getAttribute('href') || '';
            const titleEl = cardLink.querySelector('h3');
            const descEl = cardLink.querySelector('p');
            
            const title = titleEl?.textContent?.trim() || '';
            const description = descEl?.textContent?.trim() || '';
            
            if (title) {
              markdown += `- **[${title}](${href})**`;
              if (description) {
                markdown += `: ${description}`;
              }
              markdown += '\n';
            }
          });
          
          return markdown + '\n';
        }
      });

      // Add custom rule for Card components to convert them to cleaner markdown
      turndownService.addRule('cardComponents', {
        filter: function (node) {
          if (node.nodeName !== 'A') return false;
          const element = node as HTMLElement;
          // Check if this is a card by looking for card-related classes
          const classList = Array.from(element.classList);
          return classList.some(className => className.includes('card'));
        },
        replacement: function (content, node) {
          const cardElement = node as HTMLElement;
          const href = cardElement.getAttribute('href') || '';
          
          // Try to find title and description elements by looking for h3 and p tags
          const titleElement = cardElement.querySelector('h3');
          const descElement = cardElement.querySelector('p');
          
          const title = titleElement?.textContent?.trim() || '';
          const description = descElement?.textContent?.trim() || '';
          
          // Format as a cleaner markdown structure
          let markdown = '\n';
          if (title) {
            markdown += `**[${title}](${href})**\n`;
          }
          if (description) {
            markdown += `${description}\n`;
          }
          return markdown;
        }
      });

      // Add custom rule for tabs to convert them to sections
      turndownService.addRule('tabsToSections', {
        filter: function (node) {
          return (
            node.nodeName === 'DIV' &&
            (node as HTMLElement).classList.contains('tabs-container')
          );
        },
        replacement: function (content, node) {
          const tabsContainer = node as HTMLElement;
          let markdown = '\n\n';
          
          // Find all tab buttons to get labels
          const tabButtons = Array.from(tabsContainer.querySelectorAll('[role="tab"]'));
          
          // Find all tab panels
          const tabPanels = Array.from(tabsContainer.querySelectorAll('[role="tabpanel"]'));
          
          // Match panels with buttons by index
          tabPanels.forEach((panel, index) => {
            const panelElement = panel as HTMLElement;
            
            // Get the tab label from the corresponding button (same index)
            const tabLabel = tabButtons[index]?.textContent?.trim() || 'Section';
            
            // Add the tab label as a heading
            markdown += `## ${tabLabel}\n\n`;
            
            // Convert the panel content to markdown
            const panelContent = turndownService.turndown(panelElement.innerHTML);
            markdown += panelContent + '\n\n';
          });
          
          return markdown;
        }
      });

      // Add custom rule for code blocks to preserve language
      turndownService.addRule('fencedCodeBlock', {
        filter: function (node) {
          return (
            node.nodeName === 'PRE' &&
            node.firstChild &&
            node.firstChild.nodeName === 'CODE'
          );
        },
        replacement: function (content, node) {
          const codeElement = node.firstChild as HTMLElement;
          const className = codeElement.className || '';
          const language = className.match(/language-(\w+)/)?.[1] || '';
          
          // Get the actual code content
          const code = codeElement.textContent || '';
          
          return '\n\n```' + language + '\n' + code + '\n```\n\n';
        }
      });

      // Convert HTML to markdown
      let markdown = turndownService.turndown(clonedArticle);
      
      // Clean up the markdown
      markdown = markdown
        .replace(/\n{3,}/g, '\n\n')  // Remove excessive newlines
        .trim();                      // Remove leading/trailing whitespace
      
      // Copy to clipboard
      await navigator.clipboard.writeText(markdown);
      setCopied(true);
      
      // Reset the "Copied" state after timeout
      setTimeout(() => {
        setCopied(false);
      }, COPY_FEEDBACK_DURATION);
    } catch (err) {
      setError('Failed to copy. Please try again.');
      setTimeout(() => setError(null), COPY_FEEDBACK_DURATION);
      console.error('Failed to copy text: ', err);
    } finally {
      setIsLoading(false);
    }
  };

  // Display error message if there's an error
  if (error) {
    return (
      <div className="flex items-center gap-1.5 px-3 py-1.5 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-md text-red-700 dark:text-red-300 text-sm">
        <span>{error}</span>
      </div>
    );
  }

  // Render button with consistent structure to avoid hydration issues
  // The button will be disabled until client-side JS loads
  return (
    <button
      onClick={handleCopy}
      className="flex items-center gap-1.5 px-3 py-1.5 bg-black dark:bg-white text-white dark:text-black rounded-md text-sm font-medium transition-all duration-200 ease-in-out hover:opacity-90 hover:-translate-y-px focus:outline-none focus:ring-2 focus:ring-black dark:focus:ring-white focus:ring-offset-2 active:translate-y-px disabled:opacity-60 disabled:cursor-not-allowed disabled:hover:opacity-60 disabled:hover:translate-y-0"
      aria-label={copied ? 'Page copied to clipboard' : 'Copy page to clipboard'}
      type="button"
      disabled={!isClient || isLoading}
    >
      {/* Copy/Check icon using Lucide React */}
      {copied ? (
        <Check 
          className="flex-shrink-0"
          size={16}
          aria-hidden="true"
        />
      ) : (
        <Copy 
          className="flex-shrink-0"
          size={16}
          aria-hidden="true"
        />
      )}
      {isLoading ? 'Copying...' : copied ? 'Copied' : 'Copy page'}
    </button>
  );
}

// New wrapper component that adds dropdown menu to copy button
function PageActionsMenu(): ReactNode {
  const {metadata} = useDoc();
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);
  
  // Close dropdown when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setDropdownOpen(false);
      }
    };
    
    if (dropdownOpen) {
      document.addEventListener('mousedown', handleClickOutside);
      return () => document.removeEventListener('mousedown', handleClickOutside);
    }
  }, [dropdownOpen]);
  
  // Handle keyboard navigation (Escape to close)
  useEffect(() => {
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setDropdownOpen(false);
      }
    };
    
    if (dropdownOpen) {
      document.addEventListener('keydown', handleEscape);
      return () => document.removeEventListener('keydown', handleEscape);
    }
  }, [dropdownOpen]);
  
  const handleViewMarkdown = () => {
    const source = metadata?.source?.replace('@site/', '') || '';
    window.open(`https://raw.githubusercontent.com/block/goose/refs/heads/main/documentation/${source}`, '_blank');
    setDropdownOpen(false);
  };
  
  return (
    <div className="relative inline-flex" ref={dropdownRef}>
      {/* Button group container - unified appearance */}
      <div className="flex items-center bg-black dark:bg-white rounded-md">
        {/* Original Copy Page Button - keep its original styling but remove right border radius */}
        <div className="[&>button]:rounded-r-none">
          <CopyPageButton />
        </div>
        
        {/* Divider */}
        <div className="w-px h-4 bg-gray-700 dark:bg-gray-300"></div>
        
        {/* Chevron Dropdown Trigger - attached to copy button */}
        <button
          onClick={() => setDropdownOpen(!dropdownOpen)}
          className="px-2 py-1.5 bg-black dark:bg-white text-white dark:text-black rounded-l-none rounded-r-md text-sm font-medium transition-all duration-200 ease-in-out hover:opacity-90 focus:outline-none focus:ring-2 focus:ring-black dark:focus:ring-white focus:ring-offset-2 flex items-center justify-center"
          aria-label="More page actions"
          aria-expanded={dropdownOpen}
          aria-haspopup="true"
        >
          <ChevronDown size={16} className={`transition-transform duration-200 ${dropdownOpen ? 'rotate-180' : ''}`} />
        </button>
      </div>
      
      {/* Dropdown Menu */}
      {dropdownOpen && (
        <div className="absolute right-0 top-full mt-1 w-56 bg-black dark:bg-white rounded-md shadow-lg border border-gray-700 dark:border-gray-300 z-50">
          <button
            onClick={handleViewMarkdown}
            className="w-full flex items-center justify-between gap-1.5 px-3 py-1.5 text-sm text-white dark:text-black hover:opacity-90 hover:-translate-y-px active:translate-y-px transition-all duration-200 ease-in-out font-medium bg-transparent rounded-md"
          >
            <div className="flex items-center gap-1.5">
              <FileCode size={16} className="flex-shrink-0" />
              <span>View as Markdown</span>
            </div>
            <ExternalLink size={16} className="flex-shrink-0" />
          </button>
          <a
            href="/goose/docs/mcp/goose-docs-mcp"
            className="w-full flex items-center justify-between gap-1.5 px-3 py-1.5 text-sm text-white dark:text-black hover:opacity-90 hover:-translate-y-px active:translate-y-px transition-all duration-200 ease-in-out bg-transparent rounded-b-md no-underline"
            onClick={() => setDropdownOpen(false)}
          >
            <div className="flex items-center gap-1.5 font-normal">
              <Bot size={16} className="flex-shrink-0" />
              <span>Install Docs MCP</span>
            </div>
            <ExternalLink size={16} className="flex-shrink-0" />
          </a>
          {/* Future menu items can be added here */}
        </div>
      )}
    </div>
  );
}

// Hook to determine if we should show the copy button
function useShouldShowCopyButton(): boolean {
  const {metadata} = useDoc();

  // Show copy button only on actual content pages (not category/index pages)
  // A content page should have a source file (.md file)
  const hasSource = metadata?.source && metadata.source.includes('.md');
  
  // Don't show on category pages (they typically have /category/ in the permalink)
  const isNotCategoryPage = !metadata?.permalink?.includes('/category/');
  
  return hasSource && isNotCategoryPage;
}

/**
 * Decide if the toc should be rendered, on mobile or desktop viewports
 */
function useDocTOC() {
  const {frontMatter, toc} = useDoc();
  const windowSize = useWindowSize();

  const hidden = frontMatter.hide_table_of_contents;
  const canRender = !hidden && toc.length > 0;

  const mobile = canRender ? <DocItemTOCMobile /> : undefined;

  const desktop =
    canRender && (windowSize === 'desktop' || windowSize === 'ssr') ? (
      <DocItemTOCDesktop />
    ) : undefined;

  return {
    hidden,
    mobile,
    desktop,
  };
}

// Custom Content component that includes the page actions menu
function CustomDocItemContent({children}: {children: ReactNode}): ReactNode {
  const shouldShowCopyButton = useShouldShowCopyButton();
  const {metadata, frontMatter, contentTitle} = useDoc();
  
  // Check if we should render a synthetic title (same logic as original DocItem/Content)
  const shouldRenderTitle = !frontMatter.hide_title && typeof contentTitle === 'undefined';
  const syntheticTitle = shouldRenderTitle ? metadata.title : null;

  return (
    <div className={clsx(ThemeClassNames.docs.docMarkdown, 'markdown')}>
      {syntheticTitle && (
        <header className="flex justify-between items-start mb-4 flex-col md:flex-row gap-2 md:gap-0">
          <Heading as="h1" className="m-0 flex-1">{syntheticTitle}</Heading>
          {shouldShowCopyButton && <PageActionsMenu />}
        </header>
      )}
      {!syntheticTitle && shouldShowCopyButton && (
        <div className="flex justify-end mb-4">
          <PageActionsMenu />
        </div>
      )}
      <MDXContent>{children}</MDXContent>
    </div>
  );
}

// Custom Layout component that replicates the original but with our custom content
function CustomDocItemLayout({children}: {children: ReactNode}): ReactNode {
  const docTOC = useDocTOC();
  const {metadata} = useDoc();
  
  return (
    <div className="row">
      <div className={clsx('col', !docTOC.hidden && 'col--9')}>
        <ContentVisibility metadata={metadata} />
        <DocVersionBanner />
        <div className={layoutStyles.docItemContainer}>
          <article>
            <DocBreadcrumbs />
            <DocVersionBadge />
            {docTOC.mobile}
            <CustomDocItemContent>{children}</CustomDocItemContent>
            <DocItemFooter />
          </article>
          <DocItemPaginator />
        </div>
      </div>
      {docTOC.desktop && <div className="col col--3">{docTOC.desktop}</div>}
    </div>
  );
}

export default function LayoutWrapper(props: Props): ReactNode {
  return <CustomDocItemLayout {...props} />;
}
