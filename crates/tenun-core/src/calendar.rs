//! One calendar for every date (v5 build 04, V5-D6): the month a year starts in and the day a week starts on. Every
//! grain, period, period name, relative preset and year to date goes through here, so a number tile, a chart and a text
//! agree. The default (January, Monday) is the calendar Tenun always had.
//!
//! A fiscal year is named by the year it ends: with an April start, April 2026 to March 2027 is "FY 2027". Its
//! quarters count from its first month: April to June 2026 is "Q1 FY 2027". The key of a period stays the date of its
//! first day ("2026-04-01"), as before; only where a period starts, and its name, change.

use crate::query::Grain;
use crate::workspace::Locale;
use chrono::{Datelike, Duration, Months, NaiveDate};
use serde::{Deserialize, Serialize};
use std::cell::Cell;

thread_local! {
    /// The calendar of the engine call running on this thread (see [`with`]).
    static CURRENT: Cell<Calendar> = const { Cell::new(Calendar { year_start: 1, week_start: 1 }) };
}

/// The calendar of the engine call running on this thread: the workspace's, while a query or a tile runs; the default
/// otherwise. The engine sets it at its entry points, so the date helpers below need no calendar passed down.
pub fn current() -> Calendar {
    CURRENT.with(Cell::get)
}

/// Runs `f` with `cal` as this thread's calendar, and puts the one before back.
pub fn with<R>(cal: Calendar, f: impl FnOnce() -> R) -> R {
    let before = CURRENT.with(|c| c.replace(cal));
    let out = f();
    CURRENT.with(|c| c.set(before));
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calendar {
    /// The month the year starts in, 1 to 12. January: the calendar year.
    #[serde(rename = "yearStart", default = "one")]
    pub year_start: u32,
    /// The day the week starts on: 1 Monday, 6 Saturday, 7 Sunday (ISO numbers).
    #[serde(rename = "weekStart", default = "one")]
    pub week_start: u32,
}

fn one() -> u32 {
    1
}

impl Default for Calendar {
    fn default() -> Self {
        Calendar {
            year_start: 1,
            week_start: 1,
        }
    }
}

impl Calendar {
    /// The calendar as it is used: a start month outside 1–12 is January; a week start other than Saturday or Sunday
    /// is Monday.
    pub fn clean(self) -> Calendar {
        Calendar {
            year_start: if (1..=12).contains(&self.year_start) {
                self.year_start
            } else {
                1
            },
            week_start: if matches!(self.week_start, 6 | 7) {
                self.week_start
            } else {
                1
            },
        }
    }

    pub fn is_fiscal(&self) -> bool {
        self.year_start != 1
    }

    /// Months from January to the first month of the year.
    fn shift(&self) -> u32 {
        self.year_start - 1
    }

    /// Days a week's first day is before Monday: 0 for Monday, 1 for Sunday, 2 for Saturday.
    fn week_lead(&self) -> i64 {
        i64::from((8 - self.week_start) % 7)
    }

    /// The SQL of a grain over a date or time expression `r`: the first day of its period, as `YYYY-MM-DD`; a date
    /// part as a number. Parts keep their own numbers (month 1 is January, weekday 1 is Monday).
    pub fn grain_sql(&self, g: Grain, r: &str) -> String {
        let part = |f: &str| format!("CAST({f}({r}) AS DOUBLE)");
        let day = |e: String| format!("strftime({e}, '%Y-%m-%d')");
        let trunc = |u: &str| day(format!("date_trunc('{u}', {r})"));
        // A year or quarter that starts in another month: move the date back by the shift, cut, and move forward.
        let fiscal = |u: &str| {
            let k = self.shift();
            day(format!(
                "date_trunc('{u}', {r} - INTERVAL {k} MONTH) + INTERVAL {k} MONTH"
            ))
        };
        match g {
            Grain::Year if self.is_fiscal() => fiscal("year"),
            Grain::Quarter if self.is_fiscal() => fiscal("quarter"),
            Grain::Year => trunc("year"),
            Grain::Quarter => trunc("quarter"),
            Grain::Month => trunc("month"),
            Grain::Week if self.week_lead() > 0 => {
                let n = self.week_lead();
                day(format!(
                    "date_trunc('week', {r} + INTERVAL {n} DAY) - INTERVAL {n} DAY"
                ))
            }
            Grain::Week => trunc("week"),
            Grain::Day => trunc("day"),
            Grain::MonthOfYear => part("month"),
            Grain::Weekday => part("isodow"),
            Grain::Hour => part("hour"),
        }
    }

    /// The first day of the period of grain `g` that holds `d`.
    pub fn period_start(&self, d: NaiveDate, g: Grain) -> NaiveDate {
        let shifted = |months: u32| {
            // The date moved back by the shift, cut to the start of its calendar period, moved forward again.
            let back = d.checked_sub_months(Months::new(self.shift())).unwrap_or(d);
            let start =
                NaiveDate::from_ymd_opt(back.year(), back.month0() / months * months + 1, 1)
                    .unwrap_or(back);
            start
                .checked_add_months(Months::new(self.shift()))
                .unwrap_or(start)
        };
        match g {
            Grain::Year => shifted(12),
            Grain::Quarter => shifted(3),
            Grain::Month => NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap_or(d),
            Grain::Week => {
                let since = (i64::from(d.weekday().num_days_from_monday()) + self.week_lead()) % 7;
                d - Duration::days(since)
            }
            _ => d,
        }
    }

    /// The year a date's fiscal year ends in: the calendar year when the year starts in January.
    pub fn year_of(&self, d: NaiveDate) -> i32 {
        if self.is_fiscal() && d.month() >= self.year_start {
            d.year() + 1
        } else {
            d.year()
        }
    }

    /// The quarter of the year a date is in, 1 to 4, counted from the year's first month.
    pub fn quarter_of(&self, d: NaiveDate) -> u32 {
        (d.month0() + 12 - self.shift()) % 12 / 3 + 1
    }

    /// A relative preset as a half-open range of days, `lo ≤ day < hi`, ending with today (see
    /// `query::relative_range`). "This quarter", "last quarter" and "year to date" follow the year's first month.
    pub fn relative_range(&self, preset: &str, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
        let tomorrow = today + Duration::days(1);
        let month = today.with_day(1)?;
        let quarter = self.period_start(today, Grain::Quarter);
        Some(match preset {
            "last_7_days" => (today - Duration::days(6), tomorrow),
            "last_30_days" => (today - Duration::days(29), tomorrow),
            "this_month" => (month, tomorrow),
            "last_month" => (month.checked_sub_months(Months::new(1))?, month),
            "this_quarter" => (quarter, tomorrow),
            "last_quarter" => (quarter.checked_sub_months(Months::new(3))?, quarter),
            "ytd" => (self.period_start(today, Grain::Year), tomorrow),
            "last_12_months" => (tomorrow.checked_sub_months(Months::new(12))?, tomorrow),
            _ => return None,
        })
    }

    /// The name of a fiscal year or quarter ("FY 2027", "Q1 FY 2027"); `None` for any other period, or a calendar year,
    /// whose names do not change.
    pub fn fiscal_name(&self, start: NaiveDate, g: Grain, _loc: Locale) -> Option<String> {
        if !self.is_fiscal() {
            return None;
        }
        match g {
            Grain::Year => Some(format!("FY {}", self.year_of(start))),
            Grain::Quarter => Some(format!(
                "Q{} FY {}",
                self.quarter_of(start),
                self.year_of(start)
            )),
            _ => None,
        }
    }

    /// The SQL of `FISCALYEAR(d)` and `FISCALQUARTER(d)` over an expression.
    pub fn fiscal_year_sql(&self, r: &str) -> String {
        if self.is_fiscal() {
            format!(
                "(CASE WHEN month({r}) >= {} THEN year({r}) + 1 ELSE year({r}) END)",
                self.year_start
            )
        } else {
            format!("year({r})")
        }
    }

    pub fn fiscal_quarter_sql(&self, r: &str) -> String {
        format!("((month({r}) + 11 - {}) % 12 // 3 + 1)", self.shift())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    const APRIL: Calendar = Calendar {
        year_start: 4,
        week_start: 1,
    };

    #[test]
    fn an_april_year_names_its_years_by_the_year_they_end() {
        assert_eq!(
            APRIL.period_start(d("2026-03-31"), Grain::Year),
            d("2025-04-01")
        );
        assert_eq!(
            APRIL.period_start(d("2026-04-01"), Grain::Year),
            d("2026-04-01")
        );
        assert_eq!(APRIL.year_of(d("2026-03-31")), 2026);
        assert_eq!(APRIL.year_of(d("2026-04-01")), 2027);
        assert_eq!(APRIL.quarter_of(d("2026-03-31")), 4);
        assert_eq!(APRIL.quarter_of(d("2026-04-01")), 1);
        assert_eq!(APRIL.quarter_of(d("2026-12-15")), 3);
        assert_eq!(
            APRIL.period_start(d("2026-12-15"), Grain::Quarter),
            d("2026-10-01")
        );
        assert_eq!(
            APRIL
                .fiscal_name(d("2026-04-01"), Grain::Year, Locale::En)
                .as_deref(),
            Some("FY 2027")
        );
        assert_eq!(
            APRIL
                .fiscal_name(d("2026-04-01"), Grain::Quarter, Locale::En)
                .as_deref(),
            Some("Q1 FY 2027")
        );
        assert_eq!(
            Calendar::default().fiscal_name(d("2026-04-01"), Grain::Year, Locale::En),
            None
        );
        // Year to date on June 15: from April 1.
        assert_eq!(
            APRIL.relative_range("ytd", d("2026-06-15")),
            Some((d("2026-04-01"), d("2026-06-16")))
        );
        assert_eq!(
            APRIL.relative_range("last_quarter", d("2026-06-15")),
            Some((d("2026-01-01"), d("2026-04-01")))
        );
    }

    #[test]
    fn a_week_starts_on_its_day() {
        let sunday = Calendar {
            year_start: 1,
            week_start: 7,
        };
        let saturday = Calendar {
            year_start: 1,
            week_start: 6,
        };
        // Sunday October 11, 2026.
        assert_eq!(
            sunday.period_start(d("2026-10-11"), Grain::Week),
            d("2026-10-11")
        );
        assert_eq!(
            sunday.period_start(d("2026-10-17"), Grain::Week),
            d("2026-10-11")
        );
        assert_eq!(
            saturday.period_start(d("2026-10-11"), Grain::Week),
            d("2026-10-10")
        );
        assert_eq!(
            Calendar::default().period_start(d("2026-10-11"), Grain::Week),
            d("2026-10-05")
        );
    }

    #[test]
    fn the_default_calendar_writes_the_sql_tenun_always_wrote() {
        let c = Calendar::default();
        assert_eq!(
            c.grain_sql(Grain::Year, "x"),
            "strftime(date_trunc('year', x), '%Y-%m-%d')"
        );
        assert_eq!(
            c.grain_sql(Grain::Week, "x"),
            "strftime(date_trunc('week', x), '%Y-%m-%d')"
        );
        assert_eq!(
            c.grain_sql(Grain::Weekday, "x"),
            "CAST(isodow(x) AS DOUBLE)"
        );
    }
}
