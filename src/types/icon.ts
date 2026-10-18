export interface IconMetadata {
  name: string;
  displayName: string;
  category: string;
  keywords: string[];
  defaultColor?: string;
  shape?: "tile";
}

export interface IconPreset {
  [key: string]: IconMetadata;
}

