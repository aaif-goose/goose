import { useState } from 'react';
import { useTextAnimator } from '../../hooks/use-text-animator';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  readyToGetStarted: {
    id: 'greeting.readyToGetStarted',
    defaultMessage: 'Ready to get started?',
  },
  whatToWorkOn: {
    id: 'greeting.whatToWorkOn',
    defaultMessage: 'What would you like to work on?',
  },
  readyToBuild: {
    id: 'greeting.readyToBuild',
    defaultMessage: 'Ready to build something amazing?',
  },
  whatToExplore: {
    id: 'greeting.whatToExplore',
    defaultMessage: 'What would you like to explore?',
  },
  whatsOnYourMind: {
    id: 'greeting.whatsOnYourMind',
    defaultMessage: "What's on your mind?",
  },
  whatShallWeCreate: {
    id: 'greeting.whatShallWeCreate',
    defaultMessage: 'What shall we create today?',
  },
  whatProjectNeedsAttention: {
    id: 'greeting.whatProjectNeedsAttention',
    defaultMessage: 'What project needs attention?',
  },
  whatToTackle: {
    id: 'greeting.whatToTackle',
    defaultMessage: 'What would you like to tackle?',
  },
  whatNeedsToBeDone: {
    id: 'greeting.whatNeedsToBeDone',
    defaultMessage: 'What needs to be done?',
  },
  whatsThePlan: {
    id: 'greeting.whatsThePlan',
    defaultMessage: "What's the plan for today?",
  },
  readyToCreateGreat: {
    id: 'greeting.readyToCreateGreat',
    defaultMessage: 'Ready to create something great?',
  },
  whatCanBeBuilt: {
    id: 'greeting.whatCanBeBuilt',
    defaultMessage: 'What can be built today?',
  },
  whatsNextChallenge: {
    id: 'greeting.whatsNextChallenge',
    defaultMessage: "What's the next challenge?",
  },
  whatProgress: {
    id: 'greeting.whatProgress',
    defaultMessage: 'What progress can be made?',
  },
  whatToAccomplish: {
    id: 'greeting.whatToAccomplish',
    defaultMessage: 'What would you like to accomplish?',
  },
  whatTaskAwaits: {
    id: 'greeting.whatTaskAwaits',
    defaultMessage: 'What task awaits?',
  },
  whatsTheMission: {
    id: 'greeting.whatsTheMission',
    defaultMessage: "What's the mission today?",
  },
  whatCanBeAchieved: {
    id: 'greeting.whatCanBeAchieved',
    defaultMessage: 'What can be achieved?',
  },
  whatProjectReadyToBegin: {
    id: 'greeting.whatProjectReadyToBegin',
    defaultMessage: 'What project is ready to begin?',
  },
});

interface GreetingProps {
  className?: string;
  forceRefresh?: boolean;
}

export function Greeting({
  className = 'mt-1 text-4xl font-light animate-in fade-in duration-300',
  forceRefresh = false,
}: GreetingProps) {
  const intl = useIntl();

  const messageDescriptors = [
    i18n.readyToGetStarted,
    i18n.whatToWorkOn,
    i18n.readyToBuild,
    i18n.whatToExplore,
    i18n.whatsOnYourMind,
    i18n.whatShallWeCreate,
    i18n.whatProjectNeedsAttention,
    i18n.whatToTackle,
    i18n.whatToExplore,
    i18n.whatNeedsToBeDone,
    i18n.whatsThePlan,
    i18n.readyToCreateGreat,
    i18n.whatCanBeBuilt,
    i18n.whatsNextChallenge,
    i18n.whatProgress,
    i18n.whatToAccomplish,
    i18n.whatTaskAwaits,
    i18n.whatsTheMission,
    i18n.whatCanBeAchieved,
    i18n.whatProjectReadyToBegin,
  ];

  // Using lazy initializer to generate random greeting on each component instance
  const greeting = useState(() => {
    const randomMessageIndex = Math.floor(Math.random() * messageDescriptors.length);
    return messageDescriptors[randomMessageIndex];
  })[0];

  const greetingText = intl.formatMessage(greeting);
  const messageRef = useTextAnimator({ text: greetingText });

  return (
    <h1 className={className} key={forceRefresh ? Date.now() : undefined}>
      <span ref={messageRef}>{greetingText}</span>
    </h1>
  );
}
