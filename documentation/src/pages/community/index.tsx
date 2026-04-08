import type { ReactNode } from "react";
import React from "react";
import Link from "@docusaurus/Link";
import Layout from "@theme/Layout";
import Heading from "@theme/Heading";

// Import community data
import communityConfig from "./data/config.json";
import april2025Data from "./data/april-2025.json";
import may2025Data from "./data/may-2025.json";
import june2025Data from "./data/june-2025.json";
import july2025Data from "./data/july-2025.json";
import august2025Data from "./data/august-2025.json";
import september2025Data from "./data/september-2025.json";
import october2025Data from "./data/october-2025.json";
import november2025Data from "./data/november-2025.json";
import communityContentData from "./data/community-content.json";

// Create a data map for easy access
const communityDataMap = {
  "april-2025": april2025Data,
  "may-2025": may2025Data,
  "june-2025": june2025Data,
  "july-2025": july2025Data,
  "august-2025": august2025Data,
  "september-2025": september2025Data,
  "october-2025": october2025Data,
  "november-2025": november2025Data,
};

function UpcomingEventsSection() {
  return (
    <section className="w-full flex flex-col items-center gap-8 my-8">
      <div className="text-center">
        <Heading as="h1">📆 Upcoming Events</Heading>
        <p>Join us for livestreams, workshops, and discussions about goose and open source projects.</p>
      </div>
      
      {/* Embedded Calendar */}
      <iframe
        src="https://calget.com/c/t7jszrie"
        className="w-full h-[600px] border-0 rounded-lg"
        title="Goose Community Calendar"
      />
      
      {/* Call to Action */}
      <p className="italic text-textStandard">
        Have ideas for future events? Reach out to the team on <Link href="https://discord.gg/goose-oss">Discord</Link>. 
        You may also add this calendar to yours via{' '}
        <Link href="https://calendar.google.com/calendar/embed?src=c_b2b8367dac536ebf757b2745fcc5fbff2099f6c574bc13f83d16423db2dd5535%40group.calendar.google.com&ctz=America%2FNew_York">
          this link
        </Link>.
      </p>
    </section>
  );
}

function CommunityAllStarsSection() {
  const [activeMonth, setActiveMonth] = React.useState(communityConfig.defaultMonth);
  
  const currentData = communityDataMap[activeMonth];

  return (
    <section className="w-full flex flex-col items-center gap-8 my-8">
      {/* Header with Month Dropdown */}
      <div className="w-full flex flex-col items-center gap-4">
        <div className="text-center w-full">
          <Heading as="h1">🏆 Community All-Stars</Heading>
          <p>Every month, we take a moment and celebrate the open source community. Here are the top contributors and community champions!</p>
        </div>
        
        {/* Month Dropdown */}
        <div className="flex items-center gap-2">
          <label htmlFor="month-select" className="text-sm font-medium whitespace-nowrap">
            📅 Select Month:
          </label>
          <select 
            id="month-select"
            className="button button--secondary"
            value={activeMonth}
            onChange={(e) => setActiveMonth(e.target.value)}
            style={{
              padding: '0.5rem 1rem',
              fontSize: '0.9rem',
              cursor: 'pointer',
              minWidth: '150px'
            }}
          >
            {communityConfig.availableMonths.map((month) => (
              <option key={month.id} value={month.id}>
                {month.display}
              </option>
            ))}
          </select>
        </div>
      </div>

      {/* Community All-Stars Cards */}
      <div className="flex flex-wrap justify-center gap-4 w-full px-4">
        {currentData.communityStars.map((contributor, index) => (
          <StarsCard key={index} contributor={contributor} />
        ))}
      </div>
      
      <div className="text-center">
        <p>
          Thank you all for contributing! ❤️
        </p>
      </div>
      
      {/* Want to be featured section */}
      <div className="text-center">
        <Heading as="h2">Want to be featured?</Heading>
      </div>
      
      <div className="card max-w-xl">
        <div className="card__header text-center">
          <div className="avatar avatar--vertical">
            <div className="w-16 h-16 rounded-full bg-blue-400 flex items-center justify-center text-2xl text-blue-500">
              ⭐
            </div>
          </div>
        </div>
        <div className="card__body text--center">
          <div className="mb-4">
            <strong>Your Name Here</strong>
            <br />
            <small>Future Community Star</small>
          </div>
          <div className="text-sm">
            Want to be a Community All Star? Just start contributing on{' '}
            <Link href="https://github.com/aaif-goose/goose">GitHub</Link>, helping others on{' '}
            <Link href="https://discord.gg/goose-oss">Discord</Link>, or share your 
            goose projects with the community! You can check out the{' '}
            <Link href="https://github.com/aaif-goose/goose/blob/main/CONTRIBUTING.md">contributing guide</Link>{' '}
            for more tips.
          </div>
        </div>
      </div>
    </section>
  );
}

function CommunityContentSpotlightSection() {
  const [contentFilter, setContentFilter] = React.useState('all');
  const [currentPage, setCurrentPage] = React.useState(0);
  
  const filteredSubmissions = React.useMemo(() => {
    if (contentFilter === 'all') return communityContentData.submissions;
    if (contentFilter === 'hacktoberfest') {
      return communityContentData.submissions.filter(content => 
        content.hacktoberfest || content.tags?.includes('hacktoberfest')
      );
    }
    return communityContentData.submissions.filter(content => content.type === contentFilter);
  }, [contentFilter]);

  // Reset to first page when filter changes
  React.useEffect(() => {
    setCurrentPage(0);
  }, [contentFilter]);

  const filterOptions = [
    { id: 'all', label: 'All Content' },
    { id: 'hacktoberfest', label: 'Hacktoberfest 2025' },
    { id: 'blog', label: '📝 Blog Posts' },
    { id: 'video', label: '🎥 Videos' }
  ];

  const itemsPerPage = 3;
  const totalPages = Math.ceil(filteredSubmissions.length / itemsPerPage);
  const currentItems = filteredSubmissions.slice(
    currentPage * itemsPerPage, 
    (currentPage + 1) * itemsPerPage
  );

  return (
    <section className="w-full flex flex-col items-center gap-8 my-8">
      {/* Header with Filter Dropdown */}
      <div className="w-full flex flex-col items-center gap-4">
        <div className="text-center w-full">
          <Heading as="h1">{communityContentData.title}</Heading>
          <p>{communityContentData.description}</p>
        </div>
        
        {/* Filter Dropdown */}
        <div className="flex items-center gap-2">
          <label htmlFor="content-filter-select" className="text-sm font-medium whitespace-nowrap">
            🔍 Filter:
          </label>
          <select 
            id="content-filter-select"
            className="button button--secondary"
            value={contentFilter}
            onChange={(e) => setContentFilter(e.target.value)}
            style={{
              padding: '0.5rem 1rem',
              fontSize: '0.9rem',
              cursor: 'pointer',
              minWidth: '180px'
            }}
          >
            {filterOptions.map((filter) => (
              <option key={filter.id} value={filter.id}>
                {filter.label}
              </option>
            ))}
          </select>
        </div>
      </div>
      
      {/* Content Grid - Single Row with Pagination */}
      <div className="w-full max-w-6xl">
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {currentItems.map((content) => (
            <ContentCard key={content.url} content={content} />
          ))}
        </div>
        
        {filteredSubmissions.length === 0 && (
          <div className="text-center py-8">
            <p className="text-textSubtle">No content found for this filter.</p>
          </div>
        )}
        
        {/* Pagination Controls */}
        {totalPages > 1 && (
          <div className="flex items-center justify-center gap-4 mt-6">
            <button
              className="button button--secondary"
              onClick={() => setCurrentPage(prev => Math.max(0, prev - 1))}
              disabled={currentPage === 0}
              style={{
                opacity: currentPage === 0 ? 0.5 : 1,
                cursor: currentPage === 0 ? 'not-allowed' : 'pointer'
              }}
            >
              ← Previous
            </button>
            
            <span className="text-sm text-textSubtle">
              Page {currentPage + 1} of {totalPages}
            </span>
            
            <button
              className="button button--secondary"
              onClick={() => setCurrentPage(prev => Math.min(totalPages - 1, prev + 1))}
              disabled={currentPage === totalPages - 1}
              style={{
                opacity: currentPage === totalPages - 1 ? 0.5 : 1,
                cursor: currentPage === totalPages - 1 ? 'not-allowed' : 'pointer'
              }}
            >
              Next →
            </button>
          </div>
        )}
      </div>
    </section>
  );
}

function ContentCard({ content }): ReactNode {
  const getTypeIcon = (type: string) => {
    switch (type) {
      case 'blog': return '📝';
      case 'video': return '🎥';
      case 'tutorial': return '📚';
      case 'case-study': return '📊';
      default: return '📄';
    }
  };

  const formatDate = (dateString: string) => {
    return new Date(dateString).toLocaleDateString('en-US', { 
      month: 'short', 
      day: 'numeric' 
    });
  };

  return (
    <div className="card h-full transition-all duration-200 hover:shadow-lg hover:-translate-y-1">
      {/* Thumbnail */}
      <div className="card__image relative">
        <img
          src={content.thumbnail || 'https://images.unsplash.com/photo-1516321318423-f06f85e504b3?w=400&h=225&fit=crop&crop=entropy&auto=format'}
          alt={content.title}
          className="w-full h-48 object-cover"
          loading="lazy"
        />
      </div>
      
      {/* Content */}
      <div className="card__body">
        <div className="flex items-start gap-2 mb-2">
          <span className="text-lg">{getTypeIcon(content.type)}</span>
          <h3 className="text-lg font-semibold line-clamp-2 flex-1">
            <Link href={content.url} className="text-inherit hover:text-primary">
              {content.title}
            </Link>
          </h3>
        </div>
        
        {/* Author and Date */}
        <div className="flex items-center justify-between text-sm text-textSubtle mb-3">
          <div className="flex items-center gap-2">
            <img
              src={`https://github.com/${content.author.handle}.png`}
              alt={content.author.name}
              className="w-6 h-6 rounded-full"
            />
            <Link href={`https://github.com/${content.author.handle}`} className="hover:text-primary">
              @{content.author.handle}
            </Link>
          </div>
          <span>📅 {formatDate(content.submittedDate)}</span>
        </div>
        

      </div>
      

    </div>
  );
}

export function StarsCard({contributor}): ReactNode {
  return (
    <div className="w-full sm:w-[calc(50%-0.5rem)] md:w-[calc(33.333%-0.67rem)] lg:w-[calc(20%-0.8rem)] max-w-[280px]">
      <div 
        className="h-full border-2 border-borderSubtle rounded-2xl cursor-pointer hover:shadow-xl hover:border-[var(--ifm-color-primary-dark)] transition-all"
      >
        <div className="card__header text-center">
          <div className="avatar avatar--vertical">
            {contributor.avatarUrl ? (
              <img
                className="avatar__photo avatar__photo--lg"
                src={contributor.avatarUrl}
                alt={contributor.name}
              />
            ) : contributor.handle !== 'TBD' ? (
              <img
                className="avatar__photo avatar__photo--lg"
                src={`https://github.com/${contributor.handle}.png`}
                alt={contributor.name}
              />
            ) : (
              <div className="w-16 h-16 rounded-full bg-gray-200 flex items-center justify-center text-xl text-textSubtle">
                ?
              </div>
            )}
          </div>
        </div>
        <div className="card__body text-center">
          <div className="mb-2">
            <strong>
              {contributor.handle !== 'TBD' ? (
                <Link href={`https://github.com/${contributor.handle}`}>
                  {contributor.name} (@{contributor.handle})
                </Link>
              ) : (
                `${contributor.name}`
              )}
            </strong>
          </div>
        </div>
      </div>
    </div>
  );
};

export default function Community(): ReactNode {
  return (
    <Layout 
      title="Community" 
      description="Join the goose community - connect with developers, contribute to the project, and help shape the future of AI-powered development tools."
    >
      <main className="container">
        <UpcomingEventsSection />
        <CommunityAllStarsSection />
        <CommunityContentSpotlightSection />
      </main>
    </Layout>
  );
}
