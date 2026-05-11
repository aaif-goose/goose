import {
  getSessionTags,
  addSessionTags,
  removeSessionTag,
  getAllTags as getAllTagsGenerated,
  autoTagSession as autoTagSessionGenerated,
} from '../api/sdk.gen';

export type { SessionTag, TagCount } from '../api/types.gen';

export async function getTagsForSession(sessionId: string) {
  const res = await getSessionTags({ path: { session_id: sessionId } });
  return res.data?.tags ?? [];
}

export async function addTagsToSession(
  sessionId: string,
  tags: string[],
  source: 'manual' | 'auto' = 'manual',
): Promise<void> {
  await addSessionTags({ path: { session_id: sessionId }, body: { tags, source } });
}

export async function removeTagFromSession(sessionId: string, tag: string): Promise<void> {
  await removeSessionTag({ path: { session_id: sessionId, tag } });
}

export async function getAllTags() {
  const res = await getAllTagsGenerated();
  return res.data?.tags ?? [];
}

export async function autoTagSession(sessionId: string) {
  const res = await autoTagSessionGenerated({ path: { session_id: sessionId } });
  return res.data?.tags ?? [];
}
