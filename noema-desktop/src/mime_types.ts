// Supported MIME types for attachments
export const SUPPORTED_IMAGE_TYPES = [
  "image/png",
  "image/jpeg",
  "image/jpg",
  "image/gif",
  "image/webp",
];

export const SUPPORTED_AUDIO_TYPES = [
  "audio/mpeg",
  "audio/mp3",
  "audio/mp4",
  "audio/m4a",
  "audio/wav",
  "audio/webm",
  "audio/ogg",
];

export const SUPPORTED_TEXT_TYPES = [
  "text/plain",
  "text/markdown",
  "text/x-markdown",
];

export const SUPPORTED_DOCUMENT_TYPES = [
  "application/pdf",
];

export const SUPPORTED_ATTACHMENT_TYPES = [
  ...SUPPORTED_IMAGE_TYPES,
  ...SUPPORTED_AUDIO_TYPES,
  ...SUPPORTED_TEXT_TYPES,
  ...SUPPORTED_DOCUMENT_TYPES,
];

export function isImageType(mimeType: string): boolean {
  return SUPPORTED_IMAGE_TYPES.includes(mimeType.toLowerCase());
}

export function isAudioType(mimeType: string): boolean {
  return SUPPORTED_AUDIO_TYPES.includes(mimeType.toLowerCase());
}

export function isTextType(mimeType: string): boolean {
  return SUPPORTED_TEXT_TYPES.includes(mimeType.toLowerCase());
}

export function isPdfType(mimeType: string): boolean {
  return mimeType.toLowerCase() === "application/pdf";
}

export function isSupportedAttachmentType(mimeType: string): boolean {
  return SUPPORTED_ATTACHMENT_TYPES.includes(mimeType.toLowerCase());
}