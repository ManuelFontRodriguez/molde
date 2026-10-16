import data from './stocks.json';

export interface Dividend {
  ex_date: string;
  amount: string;
  currency: string;
  confirmed: boolean;
}

export interface Stock {
  ticker: string;
  name: string;
  sector: string;
  upcoming: Dividend | null;
}

export const cycleId: string = data.cycle_id;
export const stocks: Stock[] = data.stocks;

export const stockPath = (stock: Stock) => `/stocks/${stock.ticker}/`;
