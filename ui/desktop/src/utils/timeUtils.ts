import { currentLocale } from '../i18n';

export function formatMessageTimestamp(timestamp?: number): string {
  const date = timestamp ? new Date(timestamp * 1000) : new Date();
  const now = new Date();

  // Format time using locale's default hour cycle
  const timeStr = date.toLocaleTimeString(currentLocale, {
    hour: 'numeric',
    minute: '2-digit',
  });

  // Check if the message is from today
  if (
    date.getDate() === now.getDate() &&
    date.getMonth() === now.getMonth() &&
    date.getFullYear() === now.getFullYear()
  ) {
    return timeStr;
  }

  // If not today, format as localized date + time
  const dateStr = date.toLocaleDateString(currentLocale, {
    month: '2-digit',
    day: '2-digit',
    year: 'numeric',
  });

  return `${dateStr} ${timeStr}`;
}
