import { useState, useEffect } from 'react';
import { Button } from '../ui/button';
import { defineMessages, useIntl } from '../../i18n';

const i18n = defineMessages({
  activitiesLabel: {
    id: 'recipeActivityEditor.activitiesLabel',
    defaultMessage: 'Activities',
  },
  activitiesDescription: {
    id: 'recipeActivityEditor.activitiesDescription',
    defaultMessage: 'The top-line prompts and activity buttons that will display in the recipe chat window.',
  },
  messageLabel: {
    id: 'recipeActivityEditor.messageLabel',
    defaultMessage: 'Message',
  },
  messageDescription: {
    id: 'recipeActivityEditor.messageDescription',
    defaultMessage: 'A formatted message that will appear at the top of the recipe. Supports markdown formatting.',
  },
  messagePlaceholder: {
    id: 'recipeActivityEditor.messagePlaceholder',
    defaultMessage: 'Enter a user facing introduction message for your recipe (supports **bold**, *italic*, `code`, etc.)',
  },
  activityButtonsLabel: {
    id: 'recipeActivityEditor.activityButtonsLabel',
    defaultMessage: 'Activity Buttons',
  },
  activityButtonsDescription: {
    id: 'recipeActivityEditor.activityButtonsDescription',
    defaultMessage: 'Clickable buttons that will appear below the message to help users interact with your recipe.',
  },
  addNewActivityPlaceholder: {
    id: 'recipeActivityEditor.addNewActivityPlaceholder',
    defaultMessage: 'Add new activity...',
  },
  addActivity: {
    id: 'recipeActivityEditor.addActivity',
    defaultMessage: 'Add activity',
  },
});

export default function RecipeActivityEditor({
  activities = [],
  setActivities,
  onBlur,
}: {
  activities?: string[];
  setActivities: (prev: string[]) => void;
  onBlur?: () => void;
}) {
  const intl = useIntl();
  const [newActivity, setNewActivity] = useState('');
  const [messageContent, setMessageContent] = useState('');

  // Extract message content from activities on component mount and when activities change
  useEffect(() => {
    const messageActivity = activities.find((activity) =>
      activity.toLowerCase().startsWith('message:')
    );
    if (messageActivity) {
      setMessageContent(messageActivity.replace(/^message:/i, ''));
    }
  }, [activities]);

  // Get activities that are not messages
  const nonMessageActivities = activities.filter(
    (activity) => !activity.toLowerCase().startsWith('message:')
  );

  const handleAddActivity = () => {
    if (newActivity.trim()) {
      setActivities([...activities, newActivity.trim()]);
      setNewActivity('');
      // Trigger parameter extraction after adding activity
      if (onBlur) {
        onBlur();
      }
    }
  };

  const handleRemoveActivity = (activity: string) => {
    setActivities(activities.filter((a) => a !== activity));
  };

  const handleMessageChange = (value: string) => {
    setMessageContent(value);

    // Update activities array - remove existing message and add new one if not empty
    const otherActivities = activities.filter(
      (activity) => !activity.toLowerCase().startsWith('message:')
    );

    if (value.length > 0) {
      setActivities([`message:${value}`, ...otherActivities]);
    } else {
      setActivities(otherActivities);
    }
  };

  return (
    <div>
      <label htmlFor="activities" className="block text-md text-text-primary mb-2 font-bold">
        {intl.formatMessage(i18n.activitiesLabel)}
      </label>
      <p className="text-sm text-text-secondary space-y-2 pb-4">
        {intl.formatMessage(i18n.activitiesDescription)}
      </p>

      {/* Message Field */}
      <div>
        <label htmlFor="message" className="block text-sm font-medium text-text-primary mb-2">
          {intl.formatMessage(i18n.messageLabel)}
        </label>
        <p className="text-xs text-text-secondary mb-2">
          {intl.formatMessage(i18n.messageDescription)}
        </p>
        <textarea
          id="message"
          value={messageContent}
          onChange={(e) => handleMessageChange(e.target.value)}
          onBlur={onBlur}
          className="w-full px-4 py-3 border rounded-lg bg-background-primary text-text-primary placeholder:text-text-secondary focus:outline-none focus:ring-2 focus:ring-border-secondary resize-vertical"
          placeholder={intl.formatMessage(i18n.messagePlaceholder)}
          rows={3}
          autoCorrect="off"
          autoCapitalize="off"
          spellCheck="false"
        />
      </div>

      {/* Regular Activities */}
      <div className="space-y-4">
        <div>
          <label className="block text-sm font-medium text-text-primary mb-2">
            {intl.formatMessage(i18n.activityButtonsLabel)}
          </label>
          <p className="text-xs text-text-secondary mb-3">
            {intl.formatMessage(i18n.activityButtonsDescription)}
          </p>
        </div>

        {nonMessageActivities.length > 0 && (
          <div className="flex flex-wrap gap-3">
            {nonMessageActivities.map((activity, index) => (
              <div
                key={index}
                className="inline-flex items-center bg-background-primary border-2 border-border-primary rounded-full px-4 py-2 text-sm text-text-primary"
                title={activity.length > 100 ? activity : undefined}
              >
                <span>{activity.length > 100 ? activity.slice(0, 100) + '...' : activity}</span>
                <Button
                  type="button"
                  onClick={() => handleRemoveActivity(activity)}
                  variant="ghost"
                  size="sm"
                  className="ml-2 text-text-primary hover:text-text-secondary transition-colors p-0 h-auto"
                >
                  ×
                </Button>
              </div>
            ))}
          </div>
        )}
        <div className="flex gap-2 mt-3">
          <input
            type="text"
            value={newActivity}
            onChange={(e) => setNewActivity(e.target.value)}
            onKeyPress={(e) => e.key === 'Enter' && handleAddActivity()}
            onBlur={onBlur}
            className="flex-1 px-3 py-2 border border-border-primary rounded-lg bg-background-primary text-text-primary focus:outline-none focus:ring-2 focus:ring-blue-500 text-sm"
            placeholder={intl.formatMessage(i18n.addNewActivityPlaceholder)}
          />
          <button
            type="button"
            onClick={handleAddActivity}
            disabled={!newActivity.trim()}
            className="px-4 py-2 bg-blue-500 text-white rounded-lg text-sm hover:bg-blue-600 transition-colors disabled:bg-gray-400 disabled:cursor-not-allowed"
          >
            {intl.formatMessage(i18n.addActivity)}
          </button>
        </div>
      </div>
    </div>
  );
}
