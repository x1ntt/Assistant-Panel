export interface InterfaceMeta {
  id: string;
  name: string;
  description: string;
  status: string;
  isFilter: boolean;
}

export interface TrafficRow extends InterfaceMeta {
  speedIn: number | null;
  speedOut: number | null;
  totalBytesIn: string;
  totalBytesOut: string;
  highlighted: boolean;
  pinned: boolean;
  error: string | null;
}

export interface Snapshot {
  revision: number;
  updatedAt: number;
  enabled: boolean;
  error: string | null;
  interfaces: TrafficRow[];
  configError: string | null;
  warning: string | null;
}
