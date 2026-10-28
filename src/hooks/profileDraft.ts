import type { LoaderType } from '../types';

export interface CreatingProfileDraft {
  versionId: string;
  versionType?: string;
  loader: LoaderType;
  inputName: string;
}
