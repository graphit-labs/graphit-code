import type { FilePublicationInput } from '@/api/hubNative'
export interface PublicationMetadataDraft {
  name: string; description: string; author: string; tags: string; expected_sha256: string; set_latest: boolean
  dependencies: FilePublicationInput['publication']['dependencies']
}
export const emptyPublicationMetadata = (): PublicationMetadataDraft => ({ name: '', description: '', author: '', tags: '', expected_sha256: '', set_latest: false, dependencies: undefined })
