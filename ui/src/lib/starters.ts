// Starter dashboards (0.7 build 04, AD-D8): the engine guesses a column for each role, the person confirms
// each guess on one screen, then the dashboard is made. Pure, so the node tests load it.

import type { Column, StarterRole } from './api';
import { t } from './i18n.ts';

/**
 * The words of the starters, in the app's language. The engine sends them in English; each is written out
 * here so that the key test sees every one.
 */
const words = (): Record<string, string> => ({
  Sales: t('Sales'),
  'What sold, where and when: totals, a trend by month, the best products.': t('What sold, where and when: totals, a trend by month, the best products.'),
  'Budget against actual': t('Budget against actual'),
  'What was planned and what was spent, by account, by unit and by month.': t('What was planned and what was spent, by account, by unit and by month.'),
  Stock: t('Stock'),
  'What is in store, where it is, and which items run low.': t('What is in store, where it is, and which items run low.'),
  Attendance: t('Attendance'),
  'Who came, who did not, and why: by day, by person and by unit.': t('Who came, who did not, and why: by day, by person and by unit.'),
  Survey: t('Survey'),
  'The average score of each question and each group, and how the scores spread.': t('The average score of each question and each group, and how the scores spread.'),
  // Roles.
  Date: t('Date'),
  Amount: t('Amount'),
  Product: t('Product'),
  Quantity: t('Quantity'),
  Region: t('Region'),
  Account: t('Account'),
  Budget: t('Budget'),
  Actual: t('Actual'),
  Unit: t('Unit'),
  Item: t('Item'),
  Warehouse: t('Warehouse'),
  Category: t('Category'),
  Value: t('Value'),
  Person: t('Person'),
  Status: t('Status'),
  Question: t('Question'),
  Score: t('Score'),
  Group: t('Group'),
  Respondent: t('Respondent'),
  'The day of the sale.': t('The day of the sale.'),
  'What the sale brought in.': t('What the sale brought in.'),
  'What was sold: a product, an item or a service.': t('What was sold: a product, an item or a service.'),
  'How many were sold.': t('How many were sold.'),
  'Where it was sold: a region, a city, a store or a channel.': t('Where it was sold: a region, a city, a store or a channel.'),
  'What the money is for: an account, a cost line or an activity.': t('What the money is for: an account, a cost line or an activity.'),
  'The money planned.': t('The money planned.'),
  'The money spent.': t('The money spent.'),
  'The month or day the money belongs to.': t('The month or day the money belongs to.'),
  'Who spends it: a department, a unit or a project.': t('Who spends it: a department, a unit or a project.'),
  'What is kept: an item, a product or a material.': t('What is kept: an item, a product or a material.'),
  'How many are in store.': t('How many are in store.'),
  'Where it is kept: a warehouse, a store or a shelf.': t('Where it is kept: a warehouse, a store or a shelf.'),
  'The kind of item.': t('The kind of item.'),
  'What the stock is worth.': t('What the stock is worth.'),
  'The day.': t('The day.'),
  'Who: a name or a staff number.': t('Who: a name or a staff number.'),
  'Present, absent, sick, on leave…': t('Present, absent, sick, on leave…'),
  'A department, a class or a team.': t('A department, a class or a team.'),
  'What was asked: a question, an aspect or an indicator.': t('What was asked: a question, an aspect or an indicator.'),
  'The answer as a number, such as 1 to 5.': t('The answer as a number, such as 1 to 5.'),
  'A group to compare: an outlet, a service or a region.': t('A group to compare: an outlet, a service or a region.'),
  'Who answered: a name or a number.': t('Who answered: a name or a number.'),
  'When the answer was given.': t('When the answer was given.'),
});

/** A starter's word in the app's language. A word this list does not know reads as it came. */
export const starterWord = (english: string): string => words()[english] ?? english;

/** Every English word the list knows (for the tests). */
export const starterWords = (): string[] => Object.keys(words());

/** The columns that can play a role: dates for a date, numbers for a number, any column for a group. */
export function columnsFor(role: Pick<StarterRole, 'kind'>, columns: readonly Column[]): Column[] {
  return columns.filter((c) => {
    if (c.hidden) return false;
    if (role.kind === 'date') return c.dtype === 'date' || c.dtype === 'datetime';
    if (role.kind === 'number') return c.dtype === 'number';
    return true;
  });
}

/** The first required role with no column, or null when the dashboard can be made. */
export function missingRole(roles: readonly StarterRole[], mapping: Record<string, string>): StarterRole | null {
  return roles.find((r) => r.required && !mapping[r.name]) ?? null;
}
