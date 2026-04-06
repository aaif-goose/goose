import { useState, useEffect } from 'react';
import { BaseModal } from './ui/BaseModal';
import MarkdownContent from './MarkdownContent';
import { ANNOUNCEMENTS_ENABLED } from '../updates';
import packageJson from '../../package.json';
import { getAnnouncementContent } from '../../announcements/content';
import { Button } from './ui/button';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  gotIt: {
    id: 'announcementModal.gotIt',
    defaultMessage: 'Got it!',
  },
});

interface AnnouncementMeta {
  id: string;
  version: string;
  title: string;
  file: string;
}

// Simple version comparison function for semantic versioning (x.y.z)
// Returns: -1 if a < b, 0 if a === b, 1 if a > b
function compareVersions(a: string, b: string): number {
  const parseVersion = (version: string) => version.split('.').map((part) => parseInt(part, 10));

  const versionA = parseVersion(a);
  const versionB = parseVersion(b);

  for (let i = 0; i < Math.max(versionA.length, versionB.length); i++) {
    const partA = versionA[i] || 0;
    const partB = versionB[i] || 0;

    if (partA < partB) return -1;
    if (partA > partB) return 1;
  }

  return 0;
}

export default function AnnouncementModal() {
  const intl = useIntl();
  const [showAnnouncementModal, setShowAnnouncementModal] = useState(false);
  const [combinedAnnouncementContent, setCombinedAnnouncementContent] = useState<string | null>(
    null
  );
  const [unseenAnnouncements, setUnseenAnnouncements] = useState<AnnouncementMeta[]>([]);

  // Load announcements and check for unseen ones
  useEffect(() => {
    const loadAnnouncements = async () => {
      // Only proceed if announcements are enabled
      if (!ANNOUNCEMENTS_ENABLED) {
        return;
      }

      try {
        // Load the announcements index
        const indexModule = await import('../../announcements/index.json');
        const announcements = indexModule.default as AnnouncementMeta[];

        // Get current app version
        const currentVersion = packageJson.version;

        // Filter announcements to only include those for current version or earlier
        const applicableAnnouncements = announcements.filter((announcement) => {
          // Simple version comparison - assumes semantic versioning
          const announcementVersion = announcement.version;
          return compareVersions(announcementVersion, currentVersion) <= 0;
        });

        // Get list of seen announcement IDs
        const seenAnnouncementIds = await window.electron.getSetting('seenAnnouncementIds');

        // Find ALL unseen announcements (in order)
        const unseenAnnouncementsList = applicableAnnouncements.filter(
          (announcement) => !seenAnnouncementIds.includes(announcement.id)
        );

        if (unseenAnnouncementsList.length > 0) {
          // Load content for all unseen announcements
          const contentPromises = unseenAnnouncementsList.map(async (announcement) => {
            const content = getAnnouncementContent(announcement.file);
            return { announcement, content };
          });

          const loadedAnnouncements = await Promise.all(contentPromises);
          const validAnnouncements = loadedAnnouncements.filter(({ content }) => content);

          if (validAnnouncements.length > 0) {
            // Combine all announcement content with separators
            const combinedContent = validAnnouncements
              .map(({ content }) => content)
              .join('\n\n---\n\n');

            setUnseenAnnouncements(validAnnouncements.map(({ announcement }) => announcement));
            setCombinedAnnouncementContent(combinedContent);
            setShowAnnouncementModal(true);
          }
        }
      } catch (error) {
        console.warn('No announcements found or failed to load:', error);
      }
    };

    loadAnnouncements();
  }, []);

  const handleCloseAnnouncement = async () => {
    if (unseenAnnouncements.length === 0) return;

    // Get existing seen announcement IDs
    const seenAnnouncementIds = await window.electron.getSetting('seenAnnouncementIds');

    // Add all unseen announcement IDs to the seen list
    const newSeenIds = [...seenAnnouncementIds];
    unseenAnnouncements.forEach((announcement) => {
      if (!newSeenIds.includes(announcement.id)) {
        newSeenIds.push(announcement.id);
      }
    });

    await window.electron.setSetting('seenAnnouncementIds', newSeenIds);
    setShowAnnouncementModal(false);
  };

  // Don't render anything if there are no announcements to show
  if (!combinedAnnouncementContent || unseenAnnouncements.length === 0) {
    return null;
  }

  return (
    <BaseModal
      isOpen={showAnnouncementModal}
      title={
        unseenAnnouncements.length === 1
          ? unseenAnnouncements[0].title
          : `${unseenAnnouncements.length}`
      }
      actions={
        <div className="flex justify-end pb-4">
          <Button
            variant="ghost"
            onClick={handleCloseAnnouncement}
            className="w-full h-[60px] rounded-none border-b border-border-primary bg-transparent hover:bg-background-secondary text-text-primary font-medium text-md"
          >
            {intl.formatMessage(i18n.gotIt)}
          </Button>
        </div>
      }
    >
      <div className="max-h-96 overflow-y-auto -mx-12">
        <div className="px-4 py-10">
          <MarkdownContent content={combinedAnnouncementContent} />
        </div>
      </div>
    </BaseModal>
  );
}
