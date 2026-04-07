import React from 'react';
import { FolderTree, MessageSquare, Code } from 'lucide-react';
import { defineMessages, useIntl } from '../i18n';

interface PopularChatTopicsProps {
  append: (text: string) => void;
}

interface ChatTopic {
  id: string;
  icon: React.ReactNode;
  description: string;
  prompt: string;
}

const i18n = defineMessages({
  heading: {
    id: 'popularChatTopics.heading',
    defaultMessage: 'Popular chat topics',
  },
  start: {
    id: 'popularChatTopics.start',
    defaultMessage: 'Start',
  },
  organizePhotos: {
    id: 'popularChatTopics.organizePhotos',
    defaultMessage:
      'Organize the photos on my desktop into neat little folders by subject matter',
  },
  governmentForms: {
    id: 'popularChatTopics.governmentForms',
    defaultMessage:
      'Describe in detail how various forms of government works and rank each by units of geese',
  },
  tamagotchiGame: {
    id: 'popularChatTopics.tamagotchiGame',
    defaultMessage:
      'Develop a tamagotchi game that lives on my computer and follows a pixelated styling',
  },
});

export default function PopularChatTopics({ append }: PopularChatTopicsProps) {
  const intl = useIntl();

  const POPULAR_TOPICS: ChatTopic[] = [
    {
      id: 'organize-photos',
      icon: <FolderTree className="w-5 h-5" />,
      description: intl.formatMessage(i18n.organizePhotos),
      prompt: intl.formatMessage(i18n.organizePhotos),
    },
    {
      id: 'government-forms',
      icon: <MessageSquare className="w-5 h-5" />,
      description: intl.formatMessage(i18n.governmentForms),
      prompt: intl.formatMessage(i18n.governmentForms),
    },
    {
      id: 'tamagotchi-game',
      icon: <Code className="w-5 h-5" />,
      description: intl.formatMessage(i18n.tamagotchiGame),
      prompt: intl.formatMessage(i18n.tamagotchiGame),
    },
  ];

  const handleTopicClick = (prompt: string) => {
    append(prompt);
  };

  return (
    <div className="absolute bottom-0 left-0 p-6 max-w-md">
      <h3 className="text-text-secondary text-sm mb-1">
        {intl.formatMessage(i18n.heading)}
      </h3>
      <div className="space-y-1">
        {POPULAR_TOPICS.map((topic) => (
          <div
            key={topic.id}
            className="flex items-center justify-between py-1.5 hover:bg-background-secondary rounded-md cursor-pointer transition-colors"
            onClick={() => handleTopicClick(topic.prompt)}
          >
            <div className="flex items-center gap-3 flex-1 min-w-0">
              <div className="flex-shrink-0 text-text-secondary">{topic.icon}</div>
              <div className="flex-1 min-w-0">
                <p className="text-text-primary text-sm leading-tight">{topic.description}</p>
              </div>
            </div>
            <div className="flex-shrink-0 ml-4">
              <button
                className="text-sm text-text-secondary hover:text-text-primary transition-colors cursor-pointer"
                onClick={(e) => {
                  e.stopPropagation();
                  handleTopicClick(topic.prompt);
                }}
              >
                {intl.formatMessage(i18n.start)}
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
