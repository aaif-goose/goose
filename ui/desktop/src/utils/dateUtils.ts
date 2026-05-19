import type { SessionListItem } from '../acp/sessions';

export interface DateGroup {
  label: string;
  sessions: SessionListItem[];
  date: Date;
}

export function groupSessionsByDate(sessions: SessionListItem[]): DateGroup[] {
  const today = new Date();
  today.setHours(0, 0, 0, 0);

  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);

  const groups: { [key: string]: DateGroup } = {};

  sessions.forEach((session) => {
    const sessionDate = new Date(session.updatedAt);
    const sessionDateStart = new Date(sessionDate);
    sessionDateStart.setHours(0, 0, 0, 0);

    let label: string;
    let groupKey: string;

    if (sessionDateStart.getTime() === today.getTime()) {
      label = 'Today';
      groupKey = 'today';
    } else if (sessionDateStart.getTime() === yesterday.getTime()) {
      label = 'Yesterday';
      groupKey = 'yesterday';
    } else {
      // Format as "Monday, January 1" or "January 1" if it's not this year
      const currentYear = today.getFullYear();
      const sessionYear = sessionDateStart.getFullYear();

      if (sessionYear === currentYear) {
        label = sessionDateStart.toLocaleDateString('en-US', {
          weekday: 'long',
          month: 'long',
          day: 'numeric',
        });
      } else {
        label = sessionDateStart.toLocaleDateString('en-US', {
          month: 'long',
          day: 'numeric',
          year: 'numeric',
        });
      }
      groupKey = sessionDateStart.toISOString().split('T')[0];
    }

    if (!groups[groupKey]) {
      groups[groupKey] = {
        label,
        sessions: [],
        date: sessionDateStart,
      };
    }

    groups[groupKey].sessions.push(session);
  });

  // Convert to array and sort by date (newest first)
  return Object.values(groups).sort((a, b) => b.date.getTime() - a.date.getTime());
}
