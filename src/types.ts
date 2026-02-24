export type CardInfo = {
  uid: string;
  atqa: string;
  sak: string;
  cardType: string;
};

export type SectorPreview = {
  sector: number;
  blocks: string[];
  ascii: string[];
};

export type DumpSector = {
  sector: number;
  blocks: string[];
};

export type DumpFile = {
  format: string;
  uid: string;
  cardType: string;
  sectors: DumpSector[];
};
