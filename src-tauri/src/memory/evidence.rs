use crate::capture::text_cleanup;
use crate::memory::types::{CleanedEvidence, DroppedSpan, EvidenceSpan, OcrQualityStats};
use std::collections::HashMap;

fn ratio(n: usize, d: usize) -> f32 {
    if d == 0 {
        0.0
    } else {
        n as f32 / d as f32
    }
}

pub fn derive_ocr_quality_stats(
    raw_line_count: usize,
    kept_line_count: usize,
    dropped_line_count: usize,
    confidence_values: &[Option<(f32, usize)>],
    content_density_score: f32,
    ui_chrome_ratio: f32,
    low_signal_ratio: f32,
) -> OcrQualityStats {
    let mut min_conf: Option<f32> = None;
    let mut max_conf: Option<f32> = None;
    let mut sum = 0.0f32;
    let mut count = 0usize;
    let mut weighted_sum = 0.0f32;
    let mut weighted_count = 0usize;
    let mut unknown = 0usize;

    for item in confidence_values {
        match item {
            Some((value, chars)) => {
                let clamped = value.clamp(0.0, 1.0);
                min_conf = Some(min_conf.map_or(clamped, |v| v.min(clamped)));
                max_conf = Some(max_conf.map_or(clamped, |v| v.max(clamped)));
                sum += clamped;
                count += 1;
                weighted_sum += clamped * (*chars as f32);
                weighted_count += *chars;
            }
            None => unknown += 1,
        }
    }

    OcrQualityStats {
        raw_blocks: raw_line_count,
        kept_blocks: kept_line_count,
        dropped_blocks: dropped_line_count,
        confidence_min: min_conf,
        confidence_mean: (count > 0).then_some(sum / count as f32),
        confidence_weighted_by_chars: (weighted_count > 0)
            .then_some(weighted_sum / weighted_count as f32),
        confidence_max: max_conf,
        unknown_confidence_count: unknown,
        content_density_score: content_density_score.clamp(0.0, 1.0),
        ui_chrome_ratio: ui_chrome_ratio.clamp(0.0, 1.0),
        low_signal_ratio: low_signal_ratio.clamp(0.0, 1.0),
    }
}

pub fn clean_evidence_text(app_name: &str, raw_text: &str) -> CleanedEvidence {
    let high_signal = text_cleanup::build_high_signal_text_for_app(app_name, raw_text);
    let salient = text_cleanup::rank_salient_spans(&high_signal.text, app_name)
        .into_iter()
        .map(|span| EvidenceSpan {
            text: span.text,
            score: span.score,
            reason: "salient_span".to_string(),
        })
        .collect::<Vec<_>>();

    let dropped_noise = high_signal.stats.dropped_noise_lines;
    let dropped_low_signal = high_signal.stats.dropped_low_signal_lines;
    let dropped_total = dropped_noise + dropped_low_signal;
    let mut dropped_reason_counts = HashMap::new();
    if dropped_noise > 0 {
        dropped_reason_counts.insert("noise".to_string(), dropped_noise);
    }
    if dropped_low_signal > 0 {
        dropped_reason_counts.insert("low_signal".to_string(), dropped_low_signal);
    }

    let dropped_spans = vec![
        DroppedSpan {
            text: "<dropped_noise_lines>".to_string(),
            reason: "noise".to_string(),
            score: ratio(dropped_noise, high_signal.stats.total_lines),
        },
        DroppedSpan {
            text: "<dropped_low_signal_lines>".to_string(),
            reason: "low_signal".to_string(),
            score: ratio(dropped_low_signal, high_signal.stats.total_lines),
        },
    ]
    .into_iter()
    .filter(|span| span.score > 0.0)
    .collect::<Vec<_>>();

    let repeated_text_ratio = text_cleanup::estimate_noise_score(app_name, &high_signal.text)
        .clamp(0.0, 1.0);
    let ui_chrome_ratio = ratio(dropped_total, high_signal.stats.total_lines);
    let content_density_score = text_cleanup::salience_concentration(&high_signal.text, app_name)
        .clamp(0.0, 1.0);
    let contamination_score = (ui_chrome_ratio * 0.6 + repeated_text_ratio * 0.4).clamp(0.0, 1.0);
    let evidence_quality = ((1.0 - contamination_score) * 0.55
        + high_signal.stats.avg_line_score.clamp(0.0, 1.0) * 0.45)
        .clamp(0.0, 1.0);

    CleanedEvidence {
        clean_text: high_signal.text,
        salient_spans: salient,
        dropped_spans,
        dropped_reason_counts,
        evidence_quality,
        contamination_score,
        ui_chrome_ratio,
        repeated_text_ratio,
        content_density_score,
    }
}
