export interface Coordinates {
  latitude: number;
  longitude: number;
}

export interface Photo {
  path: string;
  location?: Location;
  error?: string;
  src: string;
  height?: number;
  width?: number;
  loaded?: boolean;
}

export interface Location {
  Existing?: Coordinates;
  Suggested?: [Coordinates, LocationAccuracy];
}

export interface LocationAccuracy {
  meters: number,
  seconds: number,
}
