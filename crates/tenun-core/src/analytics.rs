//! Forecast (v5 build 08, V5-D10): exponential smoothing with an additive season and a damped trend (Holt-Winters),
//! fitted here, in Rust, from the tile's own result rows. No service, no network, no random start: the same rows give
//! the same forecast every time.
//!
//! The model, with season length `m` (none for a yearly grain):
//!
//! ```text
//! level   l(t) = α (y(t) − s(t−m)) + (1 − α) (l(t−1) + φ b(t−1))
//! trend   b(t) = β (l(t) − l(t−1)) + (1 − β) φ b(t−1)
//! season  s(t) = γ (y(t) − l(t−1) − φ b(t−1)) + (1 − γ) s(t−m)
//! ahead   ŷ(t+h) = l(t) + (φ + φ² + … + φʰ) b(t) + s(t+h−m⌈h/m⌉)
//! ```
//!
//! α, β, γ and φ are picked by the smallest sum of squared one-step errors: a grid, then a finer search around the
//! best point. The band is the 90 % range of those errors, widened by √h for step h.

use crate::engine::QueryResult;
use crate::query::Grain;
use crate::snapshot::{Tile, add_grain, iso, parse_day};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The periods of one season for a grain; `None`: no season (a year, an hour or a part of a date).
pub fn season_of(g: Grain) -> Option<usize> {
    match g {
        Grain::Month => Some(12),
        Grain::Quarter => Some(4),
        Grain::Week => Some(52),
        Grain::Day => Some(7),
        _ => None,
    }
}

/// The fewest finished periods a forecast needs: two full seasons and two more; six with no season.
pub fn needs(season: Option<usize>) -> usize {
    season.map_or(6, |m| 2 * m + 2)
}

/// The periods ahead a forecast shows when the tile does not say.
pub fn default_ahead(g: Grain) -> usize {
    match g {
        Grain::Month => 3,
        Grain::Quarter => 2,
        Grain::Week => 8,
        Grain::Day => 14,
        _ => 2,
    }
}

/// The 90 % range of a normal error: ± 1.645 times its spread.
const Z90: f64 = 1.645;

#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    /// The estimate for each step ahead, and its 90 % range.
    pub values: Vec<f64>,
    pub low: Vec<f64>,
    pub high: Vec<f64>,
    /// The smoothing that was fitted, for the reader who asks.
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
    pub phi: f64,
    /// The spread of the one-step errors of the fit.
    pub rmse: f64,
}

#[derive(Debug, Clone, Copy)]
struct Params {
    alpha: f64,
    beta: f64,
    gamma: f64,
    phi: f64,
}

/// The state after the last period, and the sum of squared one-step errors.
struct Run {
    level: f64,
    trend: f64,
    season: Vec<f64>,
    sse: f64,
    count: usize,
}

/// The starting level, trend and season: the mean of the first season, the change of the mean from the first season to
/// the second, over one season, and each period's distance from its season's mean, averaged over the first two.
fn start(y: &[f64], m: Option<usize>) -> (f64, f64, Vec<f64>) {
    match m {
        Some(m) => {
            let mean = |s: &[f64]| s.iter().sum::<f64>() / s.len() as f64;
            let (a, b) = (mean(&y[..m]), mean(&y[m..2 * m]));
            let season = (0..m)
                .map(|i| ((y[i] - a) + (y[m + i] - b)) / 2.0)
                .collect();
            (a, (b - a) / m as f64, season)
        }
        None => (y[0], y[1] - y[0], Vec::new()),
    }
}

fn run(y: &[f64], m: Option<usize>, p: Params) -> Run {
    let (mut level, mut trend, mut season) = start(y, m);
    let first = m.unwrap_or(1);
    let mut sse = 0.0;
    let mut count = 0;
    // The first season only sets the start; the fit is scored from the period after it.
    for (t, &v) in y.iter().enumerate().skip(first) {
        let s = m.map(|m| season[t % m]).unwrap_or(0.0);
        let guess = level + p.phi * trend + s;
        sse += (v - guess) * (v - guess);
        count += 1;
        let (before, slope) = (level, trend);
        level = p.alpha * (v - s) + (1.0 - p.alpha) * (before + p.phi * slope);
        trend = p.beta * (level - before) + (1.0 - p.beta) * p.phi * slope;
        if let Some(m) = m {
            season[t % m] = p.gamma * (v - before - p.phi * slope) + (1.0 - p.gamma) * s;
        }
    }
    Run {
        level,
        trend,
        season,
        sse,
        count,
    }
}

/// Fits the model to `y` (finished periods only, oldest first) and forecasts `ahead` periods. Refuses in words when
/// there are too few periods or a value is missing.
pub fn forecast(
    y: &[f64],
    season: Option<usize>,
    ahead: usize,
    what: &str,
) -> Result<Forecast, String> {
    let need = needs(season);
    if y.len() < need {
        return Err(format!(
            "A forecast needs at least {need} {what} of data; this chart has {}.",
            y.len()
        ));
    }
    if y.iter().any(|v| !v.is_finite()) {
        return Err("A forecast needs a value for every period; some are blank.".into());
    }
    let m = season;
    let score = |p: Params| {
        let r = run(y, m, p);
        r.sse
    };
    // A grid, then a finer search around its best point, three times.
    let alphas: Vec<f64> = (1..10).map(|i| i as f64 / 10.0).collect();
    let betas = [0.01, 0.05, 0.1, 0.2, 0.3];
    let gammas: Vec<f64> = if m.is_some() {
        vec![0.01, 0.05, 0.1, 0.2, 0.3, 0.5]
    } else {
        vec![0.0]
    };
    let phis = [0.8, 0.85, 0.9, 0.95, 0.98];
    let mut best = Params {
        alpha: 0.5,
        beta: 0.1,
        gamma: 0.1,
        phi: 0.9,
    };
    let mut best_sse = f64::INFINITY;
    for &alpha in &alphas {
        for &beta in &betas {
            for &gamma in &gammas {
                for &phi in &phis {
                    let p = Params {
                        alpha,
                        beta,
                        gamma,
                        phi,
                    };
                    let e = score(p);
                    if e < best_sse {
                        best_sse = e;
                        best = p;
                    }
                }
            }
        }
    }
    let clamp = |v: f64, lo: f64, hi: f64| v.clamp(lo, hi);
    let mut step = 0.05;
    for _ in 0..3 {
        let mut improved = true;
        while improved {
            improved = false;
            for k in 0..4 {
                for dir in [-1.0, 1.0] {
                    let mut p = best;
                    match k {
                        0 => p.alpha = clamp(p.alpha + dir * step, 0.01, 0.99),
                        1 => p.beta = clamp(p.beta + dir * step / 2.0, 0.001, 0.5),
                        2 if m.is_some() => p.gamma = clamp(p.gamma + dir * step, 0.001, 0.9),
                        3 => p.phi = clamp(p.phi + dir * step / 2.0, 0.8, 0.98),
                        _ => continue,
                    }
                    let e = score(p);
                    if e + 1e-12 < best_sse {
                        best_sse = e;
                        best = p;
                        improved = true;
                    }
                }
            }
        }
        step /= 2.0;
    }
    let r = run(y, m, best);
    let rmse = (r.sse / r.count.max(1) as f64).sqrt();
    let n = y.len();
    let floor = y.iter().all(|v| *v >= 0.0);
    let mut values = Vec::with_capacity(ahead);
    let mut low = Vec::with_capacity(ahead);
    let mut high = Vec::with_capacity(ahead);
    let mut damp = 0.0;
    for h in 1..=ahead {
        damp += best.phi.powi(h as i32);
        let s = m.map(|m| r.season[(n + h - 1) % m]).unwrap_or(0.0);
        let v = r.level + damp * r.trend + s;
        let w = Z90 * rmse * (h as f64).sqrt();
        let (mut v, mut lo, mut hi) = (v, v - w, v + w);
        // A number that is never below 0 (sales, visits) is not forecast below 0.
        if floor {
            v = v.max(0.0);
            lo = lo.max(0.0);
            hi = hi.max(0.0);
        }
        values.push(v);
        low.push(lo);
        high.push(hi);
    }
    Ok(Forecast {
        values,
        low,
        high,
        alpha: best.alpha,
        beta: best.beta,
        gamma: best.gamma,
        phi: best.phi,
        rmse,
    })
}

/// What a line or area tile asks for: the periods ahead, and whether the range is drawn (v5 build 08).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TileForecast {
    #[serde(default)]
    pub periods: Option<u32>,
    #[serde(default)]
    pub band: Option<bool>,
}

/// The forecast of a tile's result: `rows` are `[key, value, low, high]`, or `[key, series, value, low, high]` with a
/// split. A forecast that cannot be made has no rows and says why in `note`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForecastInfo {
    pub rows: Vec<Vec<Value>>,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub split: bool,
}

/// One series of a chart: its value of the split field (blank without a split), and its (period, value) points.
type Series = (Value, Vec<(String, Option<f64>)>);

/// The most series a split chart forecasts, one each.
pub const MAX_SERIES: usize = 4;

fn periods_word(g: Grain) -> &'static str {
    match g {
        Grain::Month => "months",
        Grain::Quarter => "quarters",
        Grain::Week => "weeks",
        Grain::Day => "days",
        _ => "years",
    }
}

/// Adds the forecast to a line or area tile over time. The unfinished last period is left out of the fit, and the
/// forecast starts with it.
pub fn attach(tile: &Tile, grain: Grain, res: &mut QueryResult) {
    let Some(ask) = &tile.forecast else { return };
    let refuse = |note: String| ForecastInfo {
        rows: Vec::new(),
        method: String::new(),
        note: Some(note),
        split: false,
    };
    if tile.measures.len() != 1 {
        res.forecast = Some(refuse(format!(
            "A forecast reads one value; this chart has {}.",
            tile.measures.len()
        )));
        return;
    }
    let nd = res
        .columns
        .iter()
        .take_while(|c| c.role == "dimension")
        .count();
    let split = nd >= 2;
    let partial = res.partial.as_ref().map(|p| p.key.clone());
    // Each series in the order of its periods: (period, value).
    let mut series: Vec<Series> = Vec::new();
    for r in &res.rows {
        let (Some(k), Some(v)) = (r.first().and_then(Value::as_str), r.get(nd)) else {
            continue;
        };
        if partial.as_deref() == Some(k) {
            continue;
        }
        let name = if split {
            r.get(1).cloned().unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        match series.iter_mut().find(|(n, _)| *n == name) {
            Some((_, s)) => s.push((k.to_string(), v.as_f64())),
            None => series.push((name, vec![(k.to_string(), v.as_f64())])),
        }
    }
    if series.len() > MAX_SERIES {
        res.forecast = Some(refuse(format!(
            "A forecast draws for up to {MAX_SERIES} series; this chart has {}.",
            series.len()
        )));
        return;
    }
    let season = season_of(grain);
    let ahead = ask
        .periods
        .map_or(default_ahead(grain), |n| n.clamp(1, 36) as usize)
        + usize::from(partial.is_some());
    let mut rows = Vec::new();
    let mut method = String::new();
    for (name, mut s) in series {
        s.sort_by(|a, b| a.0.cmp(&b.0));
        let Some(last) = s.last().and_then(|(k, _)| parse_day(k)) else {
            continue;
        };
        let y: Vec<f64> = s.iter().map(|(_, v)| v.unwrap_or(f64::NAN)).collect();
        match forecast(&y, season, ahead, periods_word(grain)) {
            Ok(f) => {
                method = format!(
                    "Exponential smoothing with a damped trend{} (α {:.2}, β {:.2}, γ {:.2}, φ {:.2}), fitted to {} {}.",
                    season.map_or(String::new(), |m| format!(
                        " and a season of {m} {}",
                        periods_word(grain)
                    )),
                    f.alpha,
                    f.beta,
                    f.gamma,
                    f.phi,
                    y.len(),
                    periods_word(grain)
                );
                for h in 0..ahead {
                    let key = json!(iso(add_grain(last, grain, h as i32 + 1)));
                    let mut row = vec![key];
                    if split {
                        row.push(name.clone());
                    }
                    row.extend([json!(f.values[h]), json!(f.low[h]), json!(f.high[h])]);
                    rows.push(row);
                }
            }
            Err(note) => {
                res.forecast = Some(refuse(note));
                return;
            }
        }
    }
    res.forecast = Some(ForecastInfo {
        rows,
        method,
        note: None,
        split,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pure_season_is_forecast_within_one_percent() {
        // A level of 100 with a yearly wave of ± 20, for four years.
        let wave = |t: usize| 100.0 + 20.0 * (2.0 * std::f64::consts::PI * t as f64 / 12.0).sin();
        let y: Vec<f64> = (0..48).map(wave).collect();
        let f = forecast(&y, Some(12), 12, "months").unwrap();
        for (h, v) in f.values.iter().enumerate() {
            let want = wave(48 + h);
            assert!(
                (v - want).abs() / want < 0.01,
                "step {}: {v} against {want}",
                h + 1
            );
        }
    }

    #[test]
    fn a_straight_line_goes_on_near_the_line() {
        let y: Vec<f64> = (0..10).map(|t| 50.0 + 5.0 * t as f64).collect();
        let f = forecast(&y, None, 3, "years").unwrap();
        for (h, v) in f.values.iter().enumerate() {
            let want = 50.0 + 5.0 * (10 + h) as f64;
            // Damped: a little under the line, and never by more than the trend of the steps.
            assert!(
                *v <= want + 1e-9 && want - v < 5.0 * (h + 1) as f64,
                "step {}: {v} against {want}",
                h + 1
            );
        }
    }

    #[test]
    fn too_few_periods_is_said_in_words_and_the_same_rows_give_the_same_answer() {
        let y = vec![1.0; 14];
        assert_eq!(
            forecast(&y, Some(12), 3, "months").unwrap_err(),
            "A forecast needs at least 26 months of data; this chart has 14."
        );
        let y: Vec<f64> = (0..30).map(|t| (t % 12) as f64 + t as f64 / 3.0).collect();
        assert_eq!(
            forecast(&y, Some(12), 3, "months"),
            forecast(&y, Some(12), 3, "months")
        );
    }

    #[test]
    fn the_band_widens_with_each_step_and_stays_above_zero_for_counts() {
        let y: Vec<f64> = (0..30).map(|t| 5.0 + ((t * 7) % 5) as f64).collect();
        let f = forecast(&y, Some(12), 4, "months").unwrap();
        for h in 1..4 {
            assert!(f.high[h] - f.low[h] > f.high[h - 1] - f.low[h - 1] + 1e-9);
        }
        assert!(f.low.iter().all(|v| *v >= 0.0));
        // Small counts with a wide spread: the range would reach below 0, and stops at 0.
        let y: Vec<f64> = (0..30).map(|t| [0.0, 3.0, 0.0, 2.0, 1.0][t % 5]).collect();
        let f = forecast(&y, Some(12), 6, "months").unwrap();
        assert!(f.low.iter().all(|v| *v >= 0.0), "{:?}", f.low);
        assert!(f.low.contains(&0.0), "{:?}", f.low);
    }
}
