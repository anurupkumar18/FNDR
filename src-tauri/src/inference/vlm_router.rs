#[derive(Debug, Clone, PartialEq)]
pub enum VlmRouteDecision {
    SkipDuplicate,
    SkipGoodOcr,
    SkipLowValue,
    RunLightweightVlm,
    RunHeavyVlmExplicitOnly,
    FallbackOcrOnly { reason: String },
}

pub struct VlmRouteInput<'a> {
    pub ocr_text_len: usize,
    pub ocr_confidence: f32,
    pub ocr_block_count: usize,
    pub is_duplicate: bool,
    pub system_pressure_skip: bool,
    pub vlm_enabled: bool,
    pub vlm_model_id: Option<&'a str>,
    pub vlm_available: bool,
    pub vlm_calls_remaining: u32,
}

pub fn should_run_vlm(input: &VlmRouteInput) -> VlmRouteDecision {
    if input.is_duplicate {
        return VlmRouteDecision::SkipDuplicate;
    }

    if !input.vlm_enabled {
        return VlmRouteDecision::FallbackOcrOnly {
            reason: "vlm_disabled".to_string(),
        };
    }

    if input.system_pressure_skip {
        return VlmRouteDecision::FallbackOcrOnly {
            reason: "system_pressure".to_string(),
        };
    }

    // Good OCR: VLM adds diminishing returns when text is rich.
    if input.ocr_text_len >= 300 && input.ocr_block_count >= 10 && input.ocr_confidence >= 0.40 {
        return VlmRouteDecision::SkipGoodOcr;
    }

    // Low value: almost nothing to analyze visually.
    if input.ocr_text_len < 60 && input.ocr_block_count < 3 {
        return VlmRouteDecision::SkipLowValue;
    }

    if !input.vlm_available || input.vlm_calls_remaining == 0 {
        return VlmRouteDecision::FallbackOcrOnly {
            reason: "vlm_unavailable_or_budget_exhausted".to_string(),
        };
    }

    // Heavy VLM (Qwen3-VL 4B) only when explicitly requested — never default.
    if matches!(input.vlm_model_id, Some("qwen3-vl-4b")) {
        return VlmRouteDecision::RunHeavyVlmExplicitOnly;
    }

    VlmRouteDecision::RunLightweightVlm
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_input() -> VlmRouteInput<'static> {
        VlmRouteInput {
            ocr_text_len: 100,
            ocr_confidence: 0.48,
            ocr_block_count: 8,
            is_duplicate: false,
            system_pressure_skip: false,
            vlm_enabled: true,
            vlm_model_id: Some("smolvlm-500m"),
            vlm_available: true,
            vlm_calls_remaining: 10,
        }
    }

    #[test]
    fn skip_duplicate() {
        let mut inp = base_input();
        inp.is_duplicate = true;
        assert_eq!(should_run_vlm(&inp), VlmRouteDecision::SkipDuplicate);
    }

    #[test]
    fn skip_good_ocr() {
        let mut inp = base_input();
        inp.ocr_text_len = 600;
        inp.ocr_confidence = 0.50;
        inp.ocr_block_count = 20;
        assert_eq!(should_run_vlm(&inp), VlmRouteDecision::SkipGoodOcr);
    }

    #[test]
    fn skip_low_value_tiny_frame() {
        let mut inp = base_input();
        inp.ocr_text_len = 30;
        inp.ocr_block_count = 1;
        assert_eq!(should_run_vlm(&inp), VlmRouteDecision::SkipLowValue);
    }

    #[test]
    fn fallback_system_pressure() {
        let mut inp = base_input();
        inp.system_pressure_skip = true;
        assert!(matches!(
            should_run_vlm(&inp),
            VlmRouteDecision::FallbackOcrOnly { .. }
        ));
    }

    #[test]
    fn fallback_vlm_disabled() {
        let mut inp = base_input();
        inp.vlm_enabled = false;
        assert!(matches!(
            should_run_vlm(&inp),
            VlmRouteDecision::FallbackOcrOnly { .. }
        ));
    }

    #[test]
    fn run_lightweight_vlm_for_weak_ocr_frame() {
        let inp = base_input();
        assert_eq!(should_run_vlm(&inp), VlmRouteDecision::RunLightweightVlm);
    }

    #[test]
    fn heavy_vlm_only_when_explicitly_qwen() {
        let mut inp = base_input();
        inp.vlm_model_id = Some("qwen3-vl-4b");
        inp.ocr_text_len = 50;
        assert_eq!(
            should_run_vlm(&inp),
            VlmRouteDecision::RunHeavyVlmExplicitOnly
        );
    }

    #[test]
    fn fallback_when_budget_exhausted() {
        let mut inp = base_input();
        inp.vlm_calls_remaining = 0;
        assert!(matches!(
            should_run_vlm(&inp),
            VlmRouteDecision::FallbackOcrOnly { .. }
        ));
    }
}
