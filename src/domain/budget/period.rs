//! Budget periods: which dates a period covers and how it reads.

use chrono::{Datelike, Days, Months, NaiveDate};

use super::Period;
use crate::error::{Error, Result};
use crate::i18n;
use crate::tr;

const WEEKDAYS_IT: [&str; 7] = [
    "lunedì",
    "martedì",
    "mercoledì",
    "giovedì",
    "venerdì",
    "sabato",
    "domenica",
];
const WEEKDAYS_EN: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const MONTHS_IT: [&str; 12] = [
    "gennaio",
    "febbraio",
    "marzo",
    "aprile",
    "maggio",
    "giugno",
    "luglio",
    "agosto",
    "settembre",
    "ottobre",
    "novembre",
    "dicembre",
];
const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("validated period bounds")
}

/// A date in a budget definition (YYYY-MM-DD).
pub(super) fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| Error::invalid(format!("data non valida {s:?}: usa il formato AAAA-MM-GG")))
}

impl Period {
    /// Week, month, quarter and year repeat; rolling and custom periods do not.
    pub fn is_recurring(&self) -> bool {
        matches!(
            self,
            Self::Week { .. } | Self::Month { .. } | Self::Quarter | Self::Year { .. }
        )
    }

    pub(super) fn validate(&mut self) -> Result<()> {
        match self {
            Self::Week { start_weekday } => {
                if start_weekday.is_some_and(|d| !(1..=7).contains(&d)) {
                    crate::invalid!("start_weekday va da 1 (lunedì) a 7 (domenica)");
                }
            }
            Self::Month { start_day } => {
                if start_day.is_some_and(|d| !(1..=28).contains(&d)) {
                    crate::invalid!("start_day va da 1 a 28 (per i mesi corti)");
                }
            }
            Self::Quarter => {}
            Self::Year { start_month } => {
                if start_month.is_some_and(|m| !(1..=12).contains(&m)) {
                    crate::invalid!("start_month va da 1 a 12");
                }
            }
            Self::Rolling { days } => {
                if !(1..=366).contains(days) {
                    crate::invalid!("rolling: days va da 1 a 366");
                }
            }
            Self::Custom { from, to } => {
                let (f, t) = (parse_date(from)?, parse_date(to)?);
                if f > t {
                    crate::invalid!("il periodo finisce prima di iniziare ({from} → {to})");
                }
                if (t - f).num_days() > 3 * 366 {
                    crate::invalid!("periodo troppo lungo (massimo 3 anni)");
                }
                *from = f.to_string();
                *to = t.to_string();
            }
        }
        Ok(())
    }

    /// The period containing `d` (for `custom`, its fixed dates).
    pub fn range(&self, d: NaiveDate) -> Result<(NaiveDate, NaiveDate)> {
        Ok(match *self {
            Self::Week { start_weekday } => {
                let sw = start_weekday.unwrap_or(1);
                let back = (d.weekday().number_from_monday() + 7 - sw) % 7;
                let start = d - Days::new(u64::from(back));
                (start, start + Days::new(6))
            }
            Self::Month { start_day } => {
                let sd = start_day.unwrap_or(1);
                let base = if d.day() >= sd { d } else { d - Months::new(1) };
                let start = ymd(base.year(), base.month(), sd);
                (start, start + Months::new(1) - Days::new(1))
            }
            Self::Quarter => {
                let start = ymd(d.year(), (d.month() - 1) / 3 * 3 + 1, 1);
                (start, start + Months::new(3) - Days::new(1))
            }
            Self::Year { start_month } => {
                let sm = start_month.unwrap_or(1);
                let y = if d.month() >= sm {
                    d.year()
                } else {
                    d.year() - 1
                };
                let start = ymd(y, sm, 1);
                (start, start + Months::new(12) - Days::new(1))
            }
            Self::Rolling { days } => (d - Days::new(u64::from(days.max(1)) - 1), d),
            Self::Custom { ref from, ref to } => (parse_date(from)?, parse_date(to)?),
        })
    }

    /// Readable description in the current language, e.g. "ogni mese, dal giorno 27".
    pub fn label(&self) -> String {
        let en = i18n::is_en();
        let (days, months) = if en {
            (WEEKDAYS_EN, MONTHS_EN)
        } else {
            (WEEKDAYS_IT, MONTHS_IT)
        };
        match self {
            Self::Week {
                start_weekday: None | Some(1),
            } => tr!("ogni settimana", "every week"),
            Self::Week {
                start_weekday: Some(w),
            } => {
                let day = days[(*w as usize).clamp(1, 7) - 1];
                tr!("ogni settimana, da {day}", "every week, from {day}")
            }
            Self::Month {
                start_day: None | Some(1),
            } => tr!("ogni mese", "every month"),
            Self::Month { start_day: Some(d) } => {
                tr!("ogni mese, dal giorno {d}", "every month, from day {d}")
            }
            Self::Quarter => tr!("ogni trimestre", "every quarter"),
            Self::Year {
                start_month: None | Some(1),
            } => tr!("ogni anno", "every year"),
            Self::Year {
                start_month: Some(m),
            } => {
                let month = months[(*m as usize).clamp(1, 12) - 1];
                tr!("ogni anno, da {month}", "every year, from {month}")
            }
            Self::Rolling { days } => tr!("ultimi {days} giorni", "last {days} days"),
            Self::Custom { from, to } => match (parse_date(from), parse_date(to)) {
                (Ok(f), Ok(t)) => {
                    let (f, t) = (f.format("%d/%m/%Y"), t.format("%d/%m/%Y"));
                    tr!("dal {f} al {t}", "from {f} to {t}")
                }
                _ => tr!("dal {from} al {to}", "from {from} to {to}"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::testing::d;

    #[test]
    fn ranges() {
        let t = d("2026-11-12"); // Thursday
        let range = |p: Period, day| p.range(day).unwrap();
        assert_eq!(
            range(
                Period::Week {
                    start_weekday: None
                },
                t
            ),
            (d("2026-11-09"), d("2026-11-15"))
        );
        assert_eq!(
            range(
                Period::Week {
                    start_weekday: Some(6)
                },
                t
            ),
            (d("2026-11-07"), d("2026-11-13"))
        );
        assert_eq!(
            range(
                Period::Month {
                    start_day: Some(27)
                },
                t
            ),
            (d("2026-10-27"), d("2026-11-26"))
        );
        assert_eq!(
            range(
                Period::Month {
                    start_day: Some(27)
                },
                d("2026-11-27")
            ),
            (d("2026-11-27"), d("2026-12-26"))
        );
        assert_eq!(
            range(Period::Quarter, t),
            (d("2026-10-01"), d("2026-12-31"))
        );
        assert_eq!(
            range(
                Period::Year {
                    start_month: Some(4)
                },
                d("2026-02-01")
            ),
            (d("2025-04-01"), d("2026-03-31"))
        );
        assert_eq!(range(Period::Rolling { days: 30 }, t), (d("2026-10-14"), t));
    }
}
